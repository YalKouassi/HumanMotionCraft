//! The models page headless (roadmap 196000), in temporary folders with made-up names: every model
//! is listed, a run never writes over one, the main model is a choice that loses nothing, and a set
//! made of others follows them. Python is never started here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use ac_review::models::Places;
use ac_review::review::Review;
use ac_review::training_set::{ClipStatus, SetClip, TrainingSet};
use serde_json::{Map, Value, json};

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ac-review-models-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn p(v: Value) -> Map<String, Value> {
    match v {
        Value::Object(m) => m,
        _ => panic!("params are an object"),
    }
}

/// A review with every place it writes under `dir`.
fn review(dir: &Path) -> Review {
    let data = dir.join("training/data/rig");
    std::fs::create_dir_all(&data).unwrap();
    let mut r = Review::open(&data, &dir.join("decisions.json")).unwrap();
    r.training.folder = Some(dir.join("sets"));
    r.places = Places {
        models: dir.join("training/data/models"),
        training: dir.join("training"),
        installed: dir.join("installed"),
        manifest: Some(dir.join("MANIFEST.json")),
        library: None,
    };
    std::fs::create_dir_all(&r.places.models).unwrap();
    r
}

fn write_card(models: &Path, tag: &str, card: Value) {
    let dir = models.join(tag);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("model.json"), card.to_string()).unwrap();
    std::fs::write(dir.join("best.pt"), b"weights").unwrap();
}

fn save_set(r: &Review, name: &str, public: bool, clips: &[SetClip]) {
    let mut s = TrainingSet::new(name, public).unwrap();
    for c in clips {
        s.clips.insert(c.id.clone(), c.clone());
    }
    s.save(&r.training.folder.clone().unwrap()).unwrap();
}

fn clip(id: &str, keep: bool, labels: &[&str], cuts: &[[usize; 2]]) -> SetClip {
    let mut c = SetClip::new(id);
    if !keep {
        c.status = ClipStatus::Reject;
    }
    c.labels = labels.iter().map(|l| l.to_string()).collect();
    c.cuts = cuts.to_vec();
    c
}

/// A job that looks to be running: this test's own process.
fn pretend_a_job_runs(r: &Review, tag: &str) {
    let exe = std::env::current_exe().unwrap();
    let job = json!({
        "kind": "train", "tag": tag, "set": "s", "minutes": 10.0, "steps": null,
        "pid": std::process::id(), "image": exe.file_name().unwrap().to_string_lossy(),
        "started": 0, "log": r.places.models.join("nothing.log"),
    });
    std::fs::write(r.places.models.join("current-job.json"), job.to_string()).unwrap();
}

#[test]
fn every_model_is_listed_with_what_it_is() {
    let dir = scratch("list");
    let mut r = review(&dir);
    let models = r.places.models.clone();
    // before this page: no "stopped" and no named set
    write_card(
        &models,
        "older",
        json!({"tag": "older", "steps": 500, "minutes": 180.8, "styles": ["any", "walk"],
               "sets": [{"name": "made-up"}, {"name": "other"}],
               "best": {"step": 400, "score": 0.5}, "evaluation": {"score": 0.6}}),
    );
    write_card(
        &models,
        "cut-short",
        json!({"tag": "cut-short", "steps": 7, "minutes": 0.1, "stopped": true, "styles": [],
               "training_set": {"name": "mine"}, "evaluation": {"score": null}}),
    );
    // forced to stop: a checkpoint, no card
    std::fs::create_dir_all(models.join("forced")).unwrap();
    std::fs::write(models.join("forced/best.pt"), b"weights").unwrap();
    std::fs::create_dir_all(models.join("broken")).unwrap();
    std::fs::write(models.join("broken/model.json"), "not json").unwrap();
    // beside the folders, not models
    std::fs::write(models.join("a-run.log"), "text").unwrap();
    std::fs::write(
        r.places.manifest.as_ref().unwrap(),
        json!({"models": [{"tag": "parts", "source": "training/data/models/older/best.pt (a run)"}]})
            .to_string(),
    )
    .unwrap();

    let v = r.run("review.models", &p(json!({}))).unwrap();
    let rows = v["rows"].as_array().unwrap();
    let row = |t: &str| {
        rows.iter()
            .find(|r| r["tag"] == t)
            .unwrap_or_else(|| panic!("{t}: {v}"))
    };
    assert_eq!(rows.len(), 4, "{v}");
    assert_eq!(row("older")["state"], "finished");
    assert_eq!(row("older")["set"], "made-up, other");
    assert_eq!(row("older")["minutes"], 180.8);
    assert_eq!(row("older")["steps"], 500);
    assert_eq!(row("older")["styles"], 2);
    assert_eq!(row("older")["score"], 0.6);
    assert_eq!(row("older")["best_score"], 0.5);
    assert_eq!(row("older")["shipped"], true);
    assert_eq!(row("cut-short")["state"], "stopped early");
    assert_eq!(row("cut-short")["set"], "mine");
    assert_eq!(row("cut-short")["usable"], true);
    assert_eq!(row("forced")["state"], "unfinished");
    assert_eq!(row("forced")["usable"], false);
    assert!(row("broken")["problem"].as_str().unwrap().contains("model.json"));
    assert_eq!(row("broken")["usable"], false);
    assert_eq!(v["opens_with"], "the shipped model");
    assert_eq!(v["everyone_gets"], "parts, made from older");
    assert!(v["job"]["job"].is_null());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_run_is_refused_before_it_starts_when_it_would_write_over_a_model_or_can_not_read_its_set() {
    let dir = scratch("refuse");
    let mut r = review(&dir);
    let models = r.places.models.clone();
    write_card(&models, "taken", json!({"tag": "taken"}));
    save_set(&r, "good", false, &[clip("library:G/c", true, &[], &[])]);
    save_set(&r, "none-kept", false, &[clip("a/b", false, &[], &[])]);
    let start = |r: &mut Review, set: &str, tag: &str, minutes: f64| {
        r.run(
            "review.train_start",
            &p(json!({"set": set, "tag": tag, "minutes": minutes})),
        )
    };
    assert!(
        start(&mut r, "good", "taken", 10.0)
            .unwrap_err()
            .contains("never writes over")
    );
    assert!(
        start(&mut r, "good", "parts", 10.0)
            .unwrap_err()
            .contains("everyone gets")
    );
    for bad in ["", "../up", "a/b", "with space", "-flag"] {
        assert!(
            start(&mut r, "good", bad, 10.0)
                .unwrap_err()
                .contains("can't name"),
            "{bad:?}"
        );
    }
    for minutes in [0.0, 1441.0, f64::INFINITY] {
        assert!(
            r.run(
                "review.train_start",
                &p(json!({"set": "good", "tag": "new", "minutes": minutes}))
            )
            .is_err()
        );
    }
    assert!(
        start(&mut r, "none-kept", "new", 10.0)
            .unwrap_err()
            .contains("keeps no clips")
    );
    assert!(start(&mut r, "no-such-set", "new", 10.0).is_err());
    let e = start(&mut r, "good", "new", 10.0).unwrap_err();
    assert!(e.contains("no library"), "{e}");
    r.places.library = Some(dir.join("lib"));
    let e = start(&mut r, "good", "new", 10.0).unwrap_err();
    assert!(e.contains("no Python"), "{e}");
    let e = r
        .run("review.train_check", &p(json!({"set": "good"})))
        .unwrap_err();
    assert!(e.contains("no Python"), "{e}");

    // none of that made a folder or a job, or changed the model that was there
    assert!(!models.join("new").exists());
    assert!(!models.join("current-job.json").exists());
    assert_eq!(std::fs::read(models.join("taken/best.pt")).unwrap(), b"weights");

    // one job at a time
    pretend_a_job_runs(&r, "other");
    let e = start(&mut r, "good", "new", 10.0).unwrap_err();
    assert!(e.contains("is running"), "{e}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn stopping_asks_the_run_and_says_when_nothing_runs() {
    let dir = scratch("stop");
    let mut r = review(&dir);
    let stop = r.places.models.join("the-run.stop");
    let e = r.run("review.train_stop", &p(json!({}))).unwrap_err();
    assert!(e.contains("nothing is running"), "{e}");
    assert!(!stop.exists());
    pretend_a_job_runs(&r, "the-run");
    let v = r.run("review.train_stop", &p(json!({}))).unwrap();
    assert!(v["said"].as_str().unwrap().contains("save"), "{v}");
    assert!(stop.exists(), "the trainer looks for this file each step");
    let s = r.run("review.job_status", &p(json!({}))).unwrap();
    assert_eq!(s["running"], true, "{s}");
    assert_eq!(s["minutes_left"], Value::Null, "no step yet");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_finished_job_says_how_it_ended_from_what_it_left() {
    let dir = scratch("ended");
    let mut r = review(&dir);
    let models = r.places.models.clone();
    let log = models.join("x.log");
    std::fs::write(&log, "=== started\nTraceback: it broke\n").unwrap();
    let job = |kind: &str, tag: &str| {
        // a process that isn't there
        let v = json!({"kind": kind, "tag": tag, "set": null, "minutes": null, "steps": null,
                       "pid": u32::MAX - 1, "image": "gone.exe", "started": 0, "log": log});
        std::fs::write(models.join("current-job.json"), v.to_string()).unwrap();
    };
    job("train", "run");
    let s = r.run("review.job_status", &p(json!({}))).unwrap();
    assert_eq!(s["running"], false);
    assert_eq!(s["ended_well"], false, "no card: it didn't finish");
    assert!(s["tail"].to_string().contains("it broke"), "{s}");
    write_card(&models, "run", json!({"tag": "run", "stopped": true}));
    let s = r.run("review.job_status", &p(json!({}))).unwrap();
    assert_eq!(s["ended_well"], true);
    assert!(s["ended"].as_str().unwrap().contains("stopped early"), "{s}");
    write_card(&models, "run", json!({"tag": "run", "stopped": false}));
    let s = r.run("review.job_status", &p(json!({}))).unwrap();
    assert!(s["ended"].as_str().unwrap().contains("finished"), "{s}");
    job("check", "a-set");
    let s = r.run("review.job_status", &p(json!({}))).unwrap();
    assert_eq!(s["ended_well"], false, "{s}");
    std::fs::write(&log, "=== started\n--dry-run: nothing trained\n").unwrap();
    let s = r.run("review.job_status", &p(json!({}))).unwrap();
    assert_eq!(s["ended_well"], true, "{s}");
    job("main", "run");
    let s = r.run("review.job_status", &p(json!({}))).unwrap();
    assert_eq!(
        s["ended_well"], false,
        "its log doesn't say it was made main: {s}"
    );
    std::fs::write(&log, "=== started\nrun is the main model of a folder\n").unwrap();
    // choosing another since doesn't make the export a failure
    r.run("review.model_main", &p(json!({"tag": null}))).unwrap();
    let s = r.run("review.job_status", &p(json!({}))).unwrap();
    assert_eq!(s["ended_well"], true, "{s}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_main_model_is_a_choice_that_loses_nothing() {
    let dir = scratch("main");
    let mut r = review(&dir);
    let models = r.places.models.clone();
    let installed = r.places.installed.clone();
    write_card(&models, "one", json!({"tag": "one"}));
    write_card(&models, "two", json!({"tag": "two"}));
    std::fs::create_dir_all(models.join("forced")).unwrap();
    for tag in ["one", "two"] {
        std::fs::create_dir_all(installed.join(tag)).unwrap();
        std::fs::write(installed.join(tag).join("weights.json"), "{}").unwrap();
    }
    let main = |r: &mut Review, tag: Value| r.run("review.model_main", &p(json!({ "tag": tag })));
    let e = main(&mut r, json!("forced")).unwrap_err();
    assert!(e.contains("no card"), "{e}");
    assert!(main(&mut r, json!("no-such")).is_err());
    assert!(
        main(&mut r, json!("parts"))
            .unwrap_err()
            .contains("everyone gets")
    );
    assert!(main(&mut r, json!("../up")).is_err());
    assert!(
        r.run("review.model_main", &p(json!({}))).is_err(),
        "null is asked for, not assumed"
    );

    // already exported: at once, with no job
    let v = main(&mut r, json!("one")).unwrap();
    assert_eq!(v["exporting"], false);
    let listed = r.run("review.models", &p(json!({}))).unwrap();
    assert_eq!(listed["opens_with"], "one");
    let is_main = |l: &Value, t: &str| {
        l["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["tag"] == t)
            .unwrap()["is_main"]
            .clone()
    };
    assert_eq!(is_main(&listed, "one"), true);
    assert_eq!(is_main(&listed, "two"), false);

    // another: both still there
    main(&mut r, json!("two")).unwrap();
    let listed = r.run("review.models", &p(json!({}))).unwrap();
    assert_eq!(listed["opens_with"], "two");
    assert_eq!(is_main(&listed, "one"), false);
    assert!(installed.join("one/weights.json").is_file());
    assert!(models.join("one/model.json").is_file());

    // the shipped one
    main(&mut r, Value::Null).unwrap();
    let listed = r.run("review.models", &p(json!({}))).unwrap();
    assert_eq!(listed["opens_with"], "the shipped model");
    assert_eq!(listed["main_is_shipped"], true);
    assert!(installed.join("two/weights.json").is_file());

    // the choice's folder gone: said, not hidden
    std::fs::write(installed.join("main.json"), r#"{"tag":"gone"}"#).unwrap();
    let listed = r.run("review.models", &p(json!({}))).unwrap();
    assert!(
        listed["main_problem"].as_str().unwrap().contains("gone"),
        "{listed}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn two_models_compare_gap_by_gap_and_say_when_they_can_not() {
    let dir = scratch("compare");
    let mut r = review(&dir);
    let models = r.places.models.clone();
    let card = |tag: &str, sets: Value, validated: &str| {
        json!({"tag": tag, "best": {"score": 0.4}, "validated_on": validated,
               "evaluation": {"score": 0.5, "sets": sets, "holdout": {}},
               "metrics": {"11-20": {"model_m": 0.0125, "interp_m": 0.02},
                           "5-10": {"model_m": 0.005, "interp_m": 0.01}}})
    };
    write_card(&models, "a", card("a", json!(["x"]), "held-out subjects"));
    write_card(&models, "b", card("b", json!(["x"]), "held-out subjects"));
    write_card(
        &models,
        "c",
        card("c", json!(["y"]), "training takes (a quick run)"),
    );
    write_card(&models, "d", json!({"tag": "d"}));
    std::fs::create_dir_all(models.join("forced")).unwrap();
    let v = r
        .run("review.models_compare", &p(json!({"a": "a", "b": "b"})))
        .unwrap();
    let gaps = v["gaps"].as_array().unwrap();
    assert_eq!(gaps[0]["frames"], "5-10", "by number, not by text");
    assert_eq!(gaps[1]["frames"], "11-20");
    assert_eq!(gaps[1]["a_mm"], 12.5);
    assert_eq!(gaps[1]["b_plain_mm"], 20.0);
    assert_eq!(v["a"]["score"], 0.5);
    assert_eq!(v["notes"], json!([]));
    let v = r
        .run("review.models_compare", &p(json!({"a": "a", "b": "c"})))
        .unwrap();
    let notes = v["notes"].to_string();
    assert!(notes.contains("c was measured on clips it trained on"), "{notes}");
    assert!(notes.contains("different clips"), "{notes}");
    // a card with no numbers shows gaps as missing, not as zero
    let v = r
        .run("review.models_compare", &p(json!({"a": "a", "b": "d"})))
        .unwrap();
    assert!(v["gaps"][0]["b_mm"].is_null(), "{v}");
    let e = r
        .run("review.models_compare", &p(json!({"a": "a", "b": "forced"})))
        .unwrap_err();
    assert!(e.contains("never finished"), "{e}");
    assert!(
        r.run("review.models_compare", &p(json!({"a": "a", "b": "../up"})))
            .is_err()
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_set_made_of_others_follows_them() {
    let dir = scratch("combine");
    let mut r = review(&dir);
    let folder = dir.join("sets");
    save_set(
        &r,
        "walk",
        true,
        &[
            clip("library:G/both", true, &["walk"], &[[2, 4]]),
            clip("library:G/out-here", false, &["walk"], &[]),
            clip("library:G/only-walk", true, &["walk"], &[]),
        ],
    );
    save_set(
        &r,
        "run",
        true,
        &[
            clip("library:G/both", true, &["run", "walk"], &[[5, 6], [9, 9]]),
            clip("library:G/out-here", true, &["run"], &[]),
        ],
    );
    save_set(&r, "private", false, &[clip("library:G/mine", true, &[], &[])]);
    std::fs::create_dir_all(r.places.training.join("review")).unwrap();
    std::fs::write(
        r.places.training.join("review/natural-takes.json"),
        json!({"include": ["freeset/take_a", "library:G/only-walk"], "exclude": []}).to_string(),
    )
    .unwrap();

    let combine = |r: &mut Review, v: Value| r.run("review.set_combine", &p(v));
    let v = combine(
        &mut r,
        json!({"name": "base", "from": ["walk", "run"], "free": true}),
    )
    .unwrap();
    assert_eq!(v["keep"], 3, "{v}");
    assert_eq!(v["reject"], 1, "{v}");
    assert_eq!(v["free_clips"], 2);
    let base = TrainingSet::load(&folder, "base").unwrap();
    assert!(base.public, "all its sources are");
    assert_eq!(base.set_version, 1);
    let both = &base.clips["library:G/both"];
    assert_eq!(both.labels, ["walk", "run"]);
    assert_eq!(both.cuts, [[2, 6], [9, 9]]);
    assert_eq!(base.clips["library:G/out-here"].status, ClipStatus::Reject);
    assert!(base.clips["freeset/take_a"].labels.is_empty());
    assert_eq!(
        base.clips["library:G/only-walk"].labels,
        ["walk"],
        "the free list adds, never replaces"
    );
    assert!(folder.join("base.combine.json").is_file());
    let (listed, problems) = ac_review::training_set::list(&folder);
    let names: Vec<&str> = listed.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        ["base", "private", "run", "walk"],
        "what it's made from is no set"
    );
    assert!(problems.is_empty(), "{problems:?}");

    // the same again changes nothing, so its version still means its clips
    let v = combine(
        &mut r,
        json!({"name": "base", "from": ["walk", "run"], "free": true}),
    )
    .unwrap();
    assert_eq!(v["changed"], false);
    assert_eq!(v["set_version"], 1);

    // a later reject in a style's set is out of the next run, and open or not it is what's read
    r.run("review.set_open", &p(json!({"name": "base"}))).unwrap();
    assert!(r.open_recipe().is_some());
    let mut walk = TrainingSet::load(&folder, "walk").unwrap();
    walk.clips.get_mut("library:G/only-walk").unwrap().status = ClipStatus::Reject;
    walk.set_version += 1;
    walk.save(&folder).unwrap();
    std::fs::write(folder.join("base.undo.json"), "[]").unwrap();
    r.places.library = Some(dir.join("lib"));
    // the run itself stops at no Python, after the set is rebuilt
    let e = r
        .run(
            "review.train_start",
            &p(json!({"set": "base", "tag": "new", "minutes": 5})),
        )
        .unwrap_err();
    assert!(e.contains("no Python"), "{e}");
    let base = TrainingSet::load(&folder, "base").unwrap();
    assert_eq!(base.set_version, 2);
    assert_eq!(base.clips["library:G/only-walk"].status, ClipStatus::Reject);
    assert_eq!(
        r.training.active.as_ref().unwrap().set_version,
        2,
        "the one open follows"
    );
    assert!(!folder.join("base.undo.json").exists());

    // private unless all are public, unless told
    combine(&mut r, json!({"name": "mixed", "from": ["walk", "private"]})).unwrap();
    assert!(!TrainingSet::load(&folder, "mixed").unwrap().public);

    // never over a set made by hand, from itself, or from a combined set
    let e = combine(&mut r, json!({"name": "walk", "from": ["run"]})).unwrap_err();
    assert!(e.contains("made by hand"), "{e}");
    assert!(
        TrainingSet::load(&folder, "walk")
            .unwrap()
            .clips
            .contains_key("library:G/only-walk")
    );
    let e = combine(&mut r, json!({"name": "base", "from": ["base"]})).unwrap_err();
    assert!(e.contains("itself"), "{e}");
    let e = combine(&mut r, json!({"name": "other", "from": ["base"]})).unwrap_err();
    assert!(e.contains("combined"), "{e}");
    assert!(combine(&mut r, json!({"name": "other", "from": []})).is_err());
    assert!(combine(&mut r, json!({"name": "other", "from": ["no-such"]})).is_err());
    assert!(combine(&mut r, json!({"name": "../up", "from": ["walk"], "public": true})).is_err());
    std::fs::remove_dir_all(&dir).ok();
}
