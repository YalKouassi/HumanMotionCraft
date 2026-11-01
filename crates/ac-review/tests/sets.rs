//! Training sets headless, on a small library written here (generated clips only): the library
//! reads as sets and plays, sets are made, duplicated and labelled through `review.*`, and a private
//! clip stays out of the decisions file and out of any git checkout.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use ac_core::clip::{Clip, Interp, Key};
use ac_review::review::{Filter, Review, Show};
use ac_review::training_set::TrainingSet;
use serde_json::{Map, Value, json};

const RIG: [&str; 3] = ["hips", "spine", "head"];
/// A turn of the head: 0.6² + 0.8² = 1.
const TURN: [f64; 4] = [0.0, 0.0, 0.6, 0.8];

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ac-review-sets-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// `frames` frames at `fps`, the bones in the opposite order to the rig's, the head turned.
fn write_clip(root: &Path, group: &str, name: &str, frames: i64, fps: f64) {
    let bones: Vec<String> = RIG.iter().rev().map(|b| b.to_string()).collect();
    let mut c = Clip::new(bones, fps);
    c.name = name.into();
    c.group = group.into();
    let id = [0.0, 0.0, 0.0, 1.0];
    for f in [0, frames - 1] {
        c.set_key(Key::new(f, vec![TURN, id, id], [0.0, 0.0, 1.0], Interp::Linear))
            .unwrap();
    }
    let dir = root.join("animations").join(group);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(format!("{name}.clip.json")),
        ac_core::clipfile::save_clip(&c),
    )
    .unwrap();
}

fn p(v: Value) -> Map<String, Value> {
    match v {
        Value::Object(m) => m,
        _ => panic!("params are an object"),
    }
}

/// A review of one library: G has walk_01 (30 frames at 30 fps), walk_02 (60 at 30) and
/// hop (45 at 15); H has run (120 at 60). `sets`: the training sets' folder.
fn review(dir: &Path, sets: &Path) -> Review {
    let lib = dir.join("lib");
    write_clip(&lib, "G", "walk_01", 30, 30.0);
    write_clip(&lib, "G", "walk_02", 60, 30.0);
    write_clip(&lib, "G", "hop", 45, 15.0);
    write_clip(&lib, "H", "run", 120, 60.0);
    let data = dir.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let mut r = Review::open(&data, &dir.join("decisions.json")).unwrap();
    let rig: Vec<String> = RIG.iter().map(|b| b.to_string()).collect();
    r.add_library(&lib, &rig);
    r.training.folder = Some(sets.to_path_buf());
    r
}

#[test]
fn a_librarys_clips_read_as_sets_and_play_in_the_rigs_bone_order() {
    let dir = scratch("play");
    let r = review(&dir, &dir.join("sets"));
    let (s, t) = r.find("library:G/walk_02").unwrap();
    let set = &r.sets[s];
    assert!(set.unpacked());
    assert_eq!(set.takes[t].frames, 60);
    assert_eq!(set.takes[t].fps, 30.0);
    let (hs, ht) = r.find("library:G/hop").unwrap();
    assert_eq!(r.sets[hs].takes[ht].fps, 15.0);
    let m = set.motion(t).unwrap();
    assert_eq!((m.rotations.len(), m.roots.len()), (60, 60));
    // the file has the head first, the rig has it last
    let head = m.rotations[10][2];
    for (a, b) in head.iter().zip(TURN) {
        assert!((a - b).abs() < 1e-9, "{head:?}");
    }
    assert!((m.rotations[10][0][3] - 1.0).abs() < 1e-9);
    assert!((m.roots[10][2] - 1.0).abs() < 1e-9);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_clip_on_another_rig_says_so_when_shown() {
    let dir = scratch("otherrig");
    let lib = dir.join("lib");
    write_clip(&lib, "G", "a", 10, 30.0);
    let (sets, _) = ac_review::store::open_library(&lib, &["hips".to_string(), "tail".to_string()]);
    let e = sets[0].motion(0).err().unwrap();
    assert!(e.contains("tail"), "{e}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn sets_are_made_duplicated_and_labels_copied() {
    let dir = scratch("make");
    let folder = dir.join("sets");
    let mut r = review(&dir, &folder);
    let v = r.run("review.set_new", &p(json!({"name": "a"}))).unwrap();
    assert_eq!(
        (v["name"].as_str(), v["set_version"].as_u64()),
        (Some("a"), Some(1))
    );
    assert!(
        r.run("review.set_new", &p(json!({"name": "a"}))).is_err(),
        "no overwriting"
    );
    assert!(r.run("review.set_new", &p(json!({"name": "../a"}))).is_err());

    let saved = |id: &str, labels: Value, r: &mut Review| {
        r.run("review.set_clip", &p(json!({"id": id, "labels": labels})))
            .unwrap()
    };
    saved("library:G/walk_01", json!(["walk"]), &mut r);
    saved("library:G/hop", json!(["hop"]), &mut r);
    let on_disk = TrainingSet::load(&folder, "a").unwrap();
    // 1 at the start, one for each change
    assert_eq!(on_disk.set_version, 3);
    assert_eq!(on_disk.clips.len(), 2);

    let v = r.run("review.set_duplicate", &p(json!({"as": "b"}))).unwrap();
    assert_eq!(
        (v["name"].as_str(), v["clips"].as_u64(), v["set_version"].as_u64()),
        (Some("b"), Some(2), Some(1))
    );
    saved("library:G/walk_01", json!(["changed"]), &mut r);
    saved("library:H/run", json!(["run"]), &mut r);
    assert_eq!(
        TrainingSet::load(&folder, "a").unwrap(),
        on_disk,
        "the copy is its own"
    );

    // copied: walk_01 and hop are in both, run is only in b; hop's label is the same already
    let v = r.run("review.set_copy_labels", &p(json!({"from": "a"}))).unwrap();
    assert_eq!(v, json!({"matched": 2, "changed": 1, "not_in_source": 1}));
    let b = TrainingSet::load(&folder, "b").unwrap();
    assert_eq!(b.clips["library:G/walk_01"].labels, ["walk"]);
    assert_eq!(b.clips["library:H/run"].labels, ["run"]);

    let listed = r.run("review.sets", &Map::new()).unwrap();
    assert_eq!(listed["sets"], json!(["a", "b"]));
    assert_eq!(listed["active"]["name"], "b");

    // with the set open the queue and the status come from it; closed, from the decisions file
    let q = r.queue(&Filter {
        show: Show::InSet,
        ..Filter::default()
    });
    assert_eq!(q.len(), 3);
    r.run("review.set_open", &p(json!({"name": null}))).unwrap();
    assert!(
        r.queue(&Filter {
            show: Show::InSet,
            ..Filter::default()
        })
        .is_empty()
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn clips_are_kept_rejected_cut_and_removed_and_nonsense_refused() {
    let dir = scratch("clips");
    let mut r = review(&dir, &dir.join("sets"));
    assert!(
        r.run(
            "review.set_clip",
            &p(json!({"id": "library:G/hop", "status": "keep"}))
        )
        .is_err(),
        "no set open"
    );
    r.run("review.set_new", &p(json!({"name": "a"}))).unwrap();
    let id = "library:G/hop";
    for bad in [
        json!({"id": "library:G/none", "status": "keep"}),
        json!({"id": id, "status": "maybe"}),
        json!({"id": id, "cuts": [[0, 45]]}),
        json!({"id": id, "cuts": [[0, 44]]}),
    ] {
        assert!(r.run("review.set_clip", &p(bad.clone())).is_err(), "{bad}");
    }
    assert!(r.training.active.as_ref().unwrap().clips.is_empty());
    r.run(
        "review.set_clip",
        &p(json!({"id": id, "cuts": [[5, 9], [8, 12]]})),
    )
    .unwrap();
    let (s, t) = r.find(id).unwrap();
    assert_eq!(r.status_of(s, t).map(|x| x.word()), Some("cut"));
    let c = &r.training.active.as_ref().unwrap().clips[id];
    assert_eq!(c.cuts, [[5, 12]]);
    r.run("review.set_clip", &p(json!({"id": id, "status": "reject"})))
        .unwrap();
    assert_eq!(r.status_of(s, t).map(|x| x.word()), Some("reject"));
    let v = r.run("review.set_remove", &p(json!({"id": id}))).unwrap();
    assert_eq!(v["changed"], 1);
    assert_eq!(r.status_of(s, t), None);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_private_clip_never_reaches_the_decisions_file_or_a_checkout() {
    let dir = scratch("private");
    let repo = dir.join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    let folder = repo.join("sets");
    let mut r = review(&dir, &folder);

    let e = r
        .run(
            "review.decide",
            &p(json!({"id": "library:G/hop", "status": "ok"})),
        )
        .unwrap_err();
    assert!(e.contains("training set"), "{e}");
    assert!(!dir.join("decisions.json").exists());

    // a set with no private clips may live there; adding one is refused and leaves it as it was
    r.run("review.set_new", &p(json!({"name": "a"}))).unwrap();
    let before = std::fs::read_to_string(folder.join("a.set.json")).unwrap();
    let e = r
        .run(
            "review.set_clip",
            &p(json!({"id": "library:G/hop", "status": "keep"})),
        )
        .unwrap_err();
    assert!(e.contains("private"), "{e}");
    let a = r.training.active.as_ref().unwrap();
    assert!(a.clips.is_empty());
    assert_eq!(a.set_version, 1);
    assert_eq!(
        std::fs::read_to_string(folder.join("a.set.json")).unwrap(),
        before
    );

    // nor can a duplicate carry one in
    let outside = dir.join("outside");
    let mut held = TrainingSet::new("held", false).unwrap();
    held.clips.insert(
        "library:G/hop".into(),
        ac_review::training_set::SetClip::new("library:G/hop"),
    );
    held.save(&outside).unwrap();
    r.training.folder = Some(outside);
    r.open_set(Some("held")).unwrap();
    r.training.folder = Some(folder.clone());
    let e = r
        .run("review.set_duplicate", &p(json!({"as": "copy"})))
        .unwrap_err();
    assert!(e.contains("private"), "{e}");
    assert!(!folder.join("copy.set.json").exists());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_group_decision_is_one_undo_step_and_a_clip_can_override_it() {
    let dir = scratch("group");
    let mut r = review(&dir, &dir.join("sets"));
    r.run("review.set_new", &p(json!({"name": "a"}))).unwrap();
    // walk_01 and walk_02 differ by their number; hop is its own group
    // minutes: walk 30/30 + 60/30 = 3 s = 0.05; hop 45/15 = 3 s = 0.05; run 120/60 = 2 s
    let g = r.run("review.groups", &Map::new()).unwrap();
    let rows = g["groups"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    let walk = rows.iter().find(|x| x["name"] == "walk").unwrap();
    assert_eq!(
        (walk["pack"].as_str(), walk["clips"].as_u64()),
        (Some("G"), Some(2))
    );
    assert!((walk["minutes"].as_f64().unwrap() - 0.05).abs() < 1e-9);
    let key = walk["group"].as_str().unwrap().to_string();

    // the ones before it stay when the group is undone
    r.run(
        "review.set_clip",
        &p(json!({"id": "library:G/hop", "status": "reject"})),
    )
    .unwrap();
    let v = r
        .run("review.set_group", &p(json!({"group": key, "status": "reject"})))
        .unwrap();
    assert_eq!(v["changed"], 2);
    let status = |r: &Review, id: &str| {
        let (s, t) = r.find(id).unwrap();
        r.status_of(s, t).map(|x| x.word())
    };
    assert_eq!(status(&r, "library:G/walk_01"), Some("reject"));
    r.run(
        "review.set_clip",
        &p(json!({"id": "library:G/walk_02", "status": "keep"})),
    )
    .unwrap();
    assert_eq!(
        status(&r, "library:G/walk_02"),
        Some("ok"),
        "one clip overrides its group"
    );
    assert_eq!(status(&r, "library:G/walk_01"), Some("reject"));

    // back: the override, then the whole group at one stroke
    assert_eq!(r.run("review.undo", &Map::new()).unwrap()["undone"], 1);
    assert_eq!(r.run("review.undo", &Map::new()).unwrap()["undone"], 2);
    assert_eq!(status(&r, "library:G/walk_01"), None);
    assert_eq!(status(&r, "library:G/walk_02"), None);
    assert_eq!(status(&r, "library:G/hop"), Some("reject"));
    // and the file says the same
    let on_disk = TrainingSet::load(&dir.join("sets"), "a").unwrap();
    assert_eq!(on_disk.clips.len(), 1);
    assert!(
        r.run("review.set_group", &p(json!({"group": "nope", "status": "keep"})))
            .is_err()
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn many_clips_labelled_at_once_are_one_undo_step() {
    let dir = scratch("mass");
    let mut r = review(&dir, &dir.join("sets"));
    r.run("review.set_new", &p(json!({"name": "a"}))).unwrap();
    let ids = json!(["library:G/hop", "library:H/run"]);
    r.run(
        "review.set_clip",
        &p(json!({"ids": ids, "add_labels": ["x", "y"]})),
    )
    .unwrap();
    r.run("review.set_clip", &p(json!({"ids": ids, "remove_labels": ["x"]})))
        .unwrap();
    let labels = |r: &Review| {
        r.training.active.as_ref().unwrap().clips["library:H/run"]
            .labels
            .clone()
    };
    assert_eq!(labels(&r), ["y"]);
    assert_eq!(r.run("review.undo", &Map::new()).unwrap()["undone"], 2);
    assert_eq!(labels(&r), ["x", "y"]);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn totals_and_the_source_warning_through_the_commands() {
    let dir = scratch("totals");
    let folder = dir.join("sets");
    let mut r = review(&dir, &folder);
    r.run("review.set_new", &p(json!({"name": "a", "public": true})))
        .unwrap();
    // walk_01 1 s, run 2 s, hop rejected
    let ids = json!(["library:G/walk_01", "library:H/run"]);
    r.run("review.set_clip", &p(json!({"ids": ids, "labels": ["move"]})))
        .unwrap();
    r.run(
        "review.set_clip",
        &p(json!({"id": "library:G/hop", "status": "reject"})),
    )
    .unwrap();

    // nothing labelled: all Just me, so warned
    let t = r.run("review.totals", &Map::new()).unwrap();
    assert_eq!(t["unmarked"]["clips"], 2);
    assert_eq!(t["rejected"], 1);
    assert!((t["labels"]["move"]["minutes"].as_f64().unwrap() - 3.0 / 60.0).abs() < 1e-12);
    let w = t["warnings"][0].as_str().unwrap();
    assert!(w.contains("3 Just me clips (3 not labelled"), "{w}");

    // H chosen as Just me, G still unlabelled
    r.run("review.usage", &p(json!({"pack": "H", "usage": "just-me"})))
        .unwrap();
    let t = r.run("review.totals", &Map::new()).unwrap();
    let w = t["warnings"][0].as_str().unwrap();
    assert!(w.contains("3 Just me clips (2 not labelled"), "{w}");

    // both train only: nothing to warn of
    for pack in ["G", "H"] {
        r.run("review.usage", &p(json!({"pack": pack, "usage": "train-only"})))
            .unwrap();
    }
    let t = r.run("review.totals", &Map::new()).unwrap();
    assert_eq!(t["warnings"], json!([]));
    assert_eq!(t["train_only"]["clips"], 2);
    assert_eq!(t["just_me"]["clips"], 0);

    // one clip's own label wins over its pack's
    r.run(
        "review.usage",
        &p(json!({"id": "library:H/run", "usage": "just-me"})),
    )
    .unwrap();
    let t = r.run("review.totals", &Map::new()).unwrap();
    assert_eq!(t["just_me"]["clips"], 1);
    assert_eq!(t["warnings"].as_array().unwrap().len(), 1);

    // no longer public: no warning
    r.run("review.set_public", &p(json!({"public": false}))).unwrap();
    assert_eq!(
        r.run("review.totals", &Map::new()).unwrap()["warnings"],
        json!([])
    );

    // a usage file that won't read is said, and nothing leaves Just me
    std::fs::write(folder.join("usage.json"), "not json").unwrap();
    let t = r.run("review.totals", &Map::new()).unwrap();
    assert_eq!(t["train_only"]["clips"], 0);
    assert_eq!(t["warnings"].as_array().unwrap().len(), 1);
    assert!(
        r.run("review.usage", &p(json!({"pack": "H", "usage": "train-only"})))
            .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(folder.join("usage.json")).unwrap(),
        "not json"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_styles_list_plays_what_its_name_suggests_and_rejected_clips_leave_it() {
    let dir = scratch("styles");
    let sets = dir.join("sets");
    let mut r = review(&dir, &sets);
    r.run("review.set_new", &p(json!({"name": "a"}))).unwrap();
    let list = |r: &mut Review| {
        let v = r
            .run("review.list", &p(json!({"show": "all", "style": "walk"})))
            .unwrap();
        v["takes"].as_array().unwrap().len()
    };
    // walk_01 and walk_02 are suggested; hop and run aren't
    assert_eq!(list(&mut r), 2);
    let rows = r.run("review.styles", &Map::new()).unwrap();
    let walk = rows["styles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "walk")
        .unwrap()
        .clone();
    assert_eq!(
        (walk["clips"].as_u64(), walk["labelled"].as_u64()),
        (Some(2), Some(0))
    );
    assert!(walk["includes"].to_string().contains("side steps"));

    r.run(
        "review.set_clip",
        &p(json!({"id": "library:G/walk_02", "status": "reject"})),
    )
    .unwrap();
    assert_eq!(list(&mut r), 1);

    // Keep the rest: what is left is kept and labelled, rejects stay rejected, one undo step
    assert_eq!(
        r.run("review.style_keep", &p(json!({"style": "walk"}))).unwrap()["changed"],
        1
    );
    let set = r.training.active.as_ref().unwrap();
    assert_eq!(set.clips["library:G/walk_01"].labels, ["walk"]);
    assert_eq!(
        set.clips["library:G/walk_02"].status,
        ac_review::training_set::ClipStatus::Reject
    );
    assert_eq!(r.run("review.undo", &Map::new()).unwrap()["undone"], 1);

    // a clip a person labelled otherwise is no longer suggested
    r.run(
        "review.set_clip",
        &p(json!({"id": "library:G/walk_01", "labels": ["run"]})),
    )
    .unwrap();
    assert_eq!(list(&mut r), 0);
    assert!(r.run("review.style_keep", &p(json!({"style": "nope"}))).is_err());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_unsorted_pile_is_library_clips_no_name_places() {
    let dir = scratch("unsorted");
    let sets = dir.join("sets");
    write_clip(&dir.join("lib"), "G", "Take_17", 30, 30.0);
    write_clip(&dir.join("lib"), "G", "Sword_Take", 30, 30.0);
    let mut r = review(&dir, &sets);
    r.run("review.set_new", &p(json!({"name": "a"}))).unwrap();
    let names = |r: &mut Review, style: &str| -> Vec<String> {
        let v = r
            .run(
                "review.list",
                &p(json!({"show": "all", "style": style, "mine": true})),
            )
            .unwrap();
        v["takes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap().to_string())
            .collect()
    };
    let pile = names(&mut r, "unsorted");
    assert!(pile.contains(&"library:G/Take_17".to_string()), "{pile:?}");
    assert!(
        !pile.contains(&"library:G/Sword_Take".to_string()),
        "a prop stays out"
    );

    // sent somewhere, it leaves the pile
    r.run(
        "review.set_clip",
        &p(json!({"id": "library:G/Take_17", "status": "keep", "labels": ["walk"]})),
    )
    .unwrap();
    assert!(!names(&mut r, "unsorted").contains(&"library:G/Take_17".to_string()));
    assert!(names(&mut r, "walk").contains(&"library:G/Take_17".to_string()));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn undo_outlasts_the_session_and_a_bad_file_only_loses_the_steps() {
    let dir = scratch("undo");
    let sets = dir.join("sets");
    let mut r = review(&dir, &sets);
    r.run("review.set_new", &p(json!({"name": "a"}))).unwrap();
    r.run(
        "review.set_clip",
        &p(json!({"id": "library:G/hop", "status": "reject"})),
    )
    .unwrap();

    let mut again = review(&dir, &sets);
    again.open_set(Some("a")).unwrap();
    let undone = again.run("review.undo", &Map::new()).unwrap();
    assert_eq!(undone["undone"], 1);
    assert_eq!(again.training.active.as_ref().unwrap().clips.len(), 0);

    std::fs::write(sets.join("a.undo.json"), "not json").unwrap();
    let mut third = review(&dir, &sets);
    third.open_set(Some("a")).unwrap();
    assert_eq!(third.run("review.undo", &Map::new()).unwrap()["undone"], 0);
}
