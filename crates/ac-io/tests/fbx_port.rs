//! `clip.port` and files of several takes, on the generated tree `fixtures/data/port` (written by
//! `python -m ac_train.fbxwrite`, every name in it made up). `PackA/Walks/takes.fbx` is the 40 frames
//! of `walk-yup-75.fbx` under unreadable joint names, in two takes of 20: its `.meta` names the bones
//! and cuts First (take 1, frames 0 to 9), Second start (take 2, 0 to 9), Second end (take 2, 10 to 19).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ac_core::Skeleton;
use ac_core::clipfile;
use ac_core::gait;
use ac_io::fbx::port::MANIFEST;
use ac_io::fbx::unity_meta;
use ac_io::fbx::{import_bytes, open_bytes, read, scene};
use serde_json::{Value, json};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn data() -> PathBuf {
    repo().join("fixtures/data/port")
}

fn skeleton() -> Skeleton {
    Skeleton::from_json(&std::fs::read_to_string(repo().join("assets/rig/default_rig.json")).unwrap())
        .unwrap()
}

fn none() -> BTreeMap<String, String> {
    BTreeMap::new()
}

fn takes_file() -> Vec<u8> {
    std::fs::read(data().join("PackA/Walks/takes.fbx")).unwrap()
}

fn takes_meta() -> String {
    std::fs::read_to_string(data().join("PackA/Walks/takes.fbx.meta")).unwrap()
}

#[test]
fn a_file_lists_its_takes_and_reads_the_one_asked_for() {
    let doc = read(&takes_file()).unwrap();
    assert_eq!(scene::take_names(&doc), ["Take 001", "Take 002"]);
    let first = scene::motion(&doc).unwrap();
    assert_eq!((first.take.as_str(), first.frames), ("Take 001", 20));
    let second = scene::motion_of(&doc, Some("Take 002")).unwrap();
    assert_eq!((second.take.as_str(), second.frames), ("Take 002", 20));
    assert_ne!(
        first.positions[0], second.positions[0],
        "another take, other poses"
    );
    let e = scene::motion_of(&doc, Some("Nope")).unwrap_err();
    assert!(e.contains("Nope") && e.contains("Take 002"), "{e}");
}

#[test]
fn a_clips_take_name_picks_its_take() {
    let sk = skeleton();
    let m = unity_meta::parse(&takes_meta()).unwrap();
    let s = open_bytes(&takes_file(), "takes", &none(), Some(&m)).unwrap();
    assert_eq!(s.clips().len(), 3);
    // the frames of the whole walk each cut must land on: take 2 starts at the walk's frame 20.
    // Within 0.02 m, as each clip finds its own floor (see fbx_meta.rs).
    let whole = import_bytes(
        &std::fs::read(repo().join("fixtures/data/walk-yup-75.fbx")).unwrap(),
        &sk,
        "w",
        &none(),
    )
    .unwrap();
    for (clip, take, at) in [(0, "Take 001", 0), (1, "Take 002", 20), (2, "Take 002", 30)] {
        let got = s.clip(&sk, &s.clips()[clip]).unwrap();
        assert_eq!(got.clip.length(), 10);
        assert_eq!(got.report["source"]["take"], take);
        for (f, key) in got.clip.keys().iter().enumerate() {
            let want = whole.clip.keys()[f + at].root;
            for (got, want) in key.root.iter().zip(&want) {
                let off = (got - want).abs() * sk.hip_height;
                assert!(off < 0.02, "clip {clip} frame {f}: {off} m");
            }
        }
    }
    let notes = s.clip(&sk, &s.clips()[1]).unwrap().report["notes"].to_string();
    assert!(notes.contains("\\\"Take 002\\\" was read"), "{notes}");
}

#[test]
fn a_take_only_later_clips_name_needs_no_first_and_a_stale_name_is_refused() {
    let sk = skeleton();
    // no clip of the first take
    let text = takes_meta().replace("takeName: Take 001", "takeName: Take 002");
    let m = unity_meta::parse(&text).unwrap();
    let s = open_bytes(&takes_file(), "takes", &none(), Some(&m)).unwrap();
    assert_eq!(
        s.clip(&sk, &s.clips()[0]).unwrap().report["source"]["take"],
        "Take 002"
    );
    // a name the file doesn't have refuses that clip, says the takes, and the others still import
    let text = takes_meta().replace("takeName: Take 001", "takeName: Gone");
    let m = unity_meta::parse(&text).unwrap();
    let s = open_bytes(&takes_file(), "takes", &none(), Some(&m)).unwrap();
    let e = s.clip(&sk, &s.clips()[0]).unwrap_err();
    assert!(
        e.contains("Gone") && e.contains("Take 001") && e.contains("Take 002"),
        "{e}"
    );
    assert_eq!(s.clip(&sk, &s.clips()[1]).unwrap().clip.length(), 10);
}

fn lines(manifest: &Path) -> BTreeMap<String, Value> {
    std::fs::read_to_string(manifest)
        .unwrap()
        .lines()
        .skip(1)
        .map(|l| {
            let v: Value = serde_json::from_str(l).unwrap();
            (v["source"].as_str().unwrap().to_string(), v)
        })
        .collect()
}

fn params(to: &Path) -> Value {
    json!({
        "list": data().join("list.txt").to_string_lossy(),
        "root": data().to_string_lossy(),
        "to": to.to_string_lossy(),
    })
}

#[test]
fn every_source_gets_one_outcome_and_the_checks_hold() {
    let (mut s, lib) = common::session("fbx-port-all");
    let to = lib.join("port");
    let r = s.run("clip.port", &params(&to)).unwrap();
    assert_eq!(r["sources"], 9, "{r}");
    assert_eq!(r["imported"], 5, "{r}");
    assert_eq!(r["duplicate"], 1, "{r}");
    assert_eq!(r["refused"], 2, "{r}");
    assert_eq!(r["pending"], 1, "{r}");
    assert_eq!(r["unreachable"], json!(["PackA/Bad/missing.fbx"]));
    // walk 1, takes 3, turned 1, hang 1, sunk 1
    assert_eq!(r["clips"], 7, "{r}");

    let m = lines(&to.join(MANIFEST));
    assert_eq!(m.len(), 8, "the missing file has no line");
    let dup = &m["PackB/Walks/walk.fbx"];
    assert_eq!(dup["outcome"], "duplicate");
    assert_eq!(dup["of"], "PackA/Walks/walk.fbx");
    let sunk = &m["PackA/Bad/sunk.fbx"];
    assert_eq!(sunk["outcome"], "imported");
    assert!(
        m["PackA/Bad/swapped.fbx"]["reason"]
            .as_str()
            .unwrap()
            .contains("swapped"),
        "{m:?}"
    );
    assert!(
        m["PackA/Bad/not-humanoid.fbx"]["reason"]
            .as_str()
            .unwrap()
            .contains("not a humanoid")
    );

    // grouped by pack and action, no clip of what was left out
    let takes = &m["PackA/Walks/takes.fbx"];
    assert_eq!(takes["pack"], "PackA");
    assert_eq!(takes["action"], "Walks");
    assert_eq!(
        takes["clips"][1]["path"],
        "animations/PackA - Walks/takes - Second start.clip.json"
    );
    assert_eq!(takes["clips"][1]["take"], "Take 002");
    assert!(to.join("animations/PackA - Walks/walk.clip.json").is_file());
    assert!(
        !to.join("animations").read_dir().unwrap().any(|d| d
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("PackB")),
        "a duplicate writes nothing"
    );
    assert!(
        !to.join("animations/PackA - Bad/swapped.clip.json").exists(),
        "what failed writes nothing"
    );

    let sk = skeleton();
    let load = |rel: &str| clipfile::load_clip(&std::fs::read_to_string(to.join(rel)).unwrap()).unwrap();
    let first_joints = |rel: &str| {
        let c = load(rel);
        sk.fk(&c.keys()[0].rotations, c.keys()[0].root).unwrap().1
    };

    // the turned file started about a quarter turn off, and now starts facing -Y and still walks ahead
    let turned = &m["PackA/Turns/turned.fbx"]["clips"][0];
    assert!(
        (turned["turned"].as_f64().unwrap().abs() - 90.0).abs() < 30.0,
        "{turned}"
    );
    let yaw = gait::facing(&sk, &first_joints("animations/PackA - Turns/turned.clip.json")).unwrap();
    assert!(
        gait::wrap(yaw + std::f64::consts::FRAC_PI_2).abs() < 1e-6,
        "{yaw}"
    );
    // and the walk that faced -Y within the limit stays as it was
    assert_eq!(m["PackA/Walks/walk.fbx"]["clips"][0]["turned"], 0.0);

    // the hanging file keeps its height: raised 0.5 m, so its hips stay about 0.5 m over the walk's
    let hang = &m["PackA/Hangs/hang.fbx"]["clips"][0];
    assert_eq!(hang["hanging"], true, "{hang}");
    assert_eq!(hang["floor"], 0.0, "{hang}");
    let up = first_joints("animations/PackA - Hangs/hang.clip.json")[0][2]
        - first_joints("animations/PackA - Walks/walk.clip.json")[0][2];
    assert!((up - 0.5).abs() < 0.02, "{up}");
    assert_eq!(m["PackA/Walks/walk.fbx"]["clips"][0]["hanging"], false);

    // the sunk file was lowered 0.5 m: moved up by that, so it stands where the walk stands
    let sunk = &sunk["clips"][0];
    assert_eq!(sunk["hanging"], false, "{sunk}");
    assert!((sunk["raised"].as_f64().unwrap() - 0.5).abs() < 0.02, "{sunk}");
    assert!(sunk["notes"].to_string().contains("moved up"), "{sunk}");
    assert_eq!(m["PackA/Walks/walk.fbx"]["clips"][0]["raised"], 0.0);
    assert_eq!(hang["raised"], 0.0, "hanging stays as it is");
    let c = load("animations/PackA - Bad/sunk.clip.json");
    let rots: Vec<_> = c.keys().iter().map(|k| k.rotations.clone()).collect();
    let roots: Vec<_> = c.keys().iter().map(|k| k.root).collect();
    let floor = ac_core::retarget::floor_offset(&sk, &rots, &roots).unwrap();
    assert!(floor.abs() < 1e-3, "{floor}");
    let off = first_joints("animations/PackA - Bad/sunk.clip.json")[0][2]
        - first_joints("animations/PackA - Walks/walk.clip.json")[0][2];
    assert!(off.abs() < 0.02, "{off}");
    std::fs::remove_dir_all(lib).unwrap();
}

#[test]
fn a_run_again_changes_nothing_and_a_deleted_clip_stays_deleted() {
    let (mut s, lib) = common::session("fbx-port-again");
    let to = lib.join("port");
    s.run("clip.port", &params(&to)).unwrap();
    let manifest = to.join(MANIFEST);
    let before = std::fs::read(&manifest).unwrap();
    let walk = to.join("animations/PackA - Walks/walk.clip.json");
    let stamp = std::fs::metadata(&manifest).unwrap().modified().unwrap();
    // the owner deletes a bad clip as they go
    std::fs::remove_file(&walk).unwrap();

    let r = s.run("clip.port", &params(&to)).unwrap();
    assert_eq!(r["new"], 0, "{r}");
    assert_eq!(r["skipped"], 8, "{r}");
    assert_eq!(r["pending"], 1, "{r}");
    assert_eq!(r["clips"], 7, "{r}");
    assert_eq!(std::fs::read(&manifest).unwrap(), before);
    assert_eq!(std::fs::metadata(&manifest).unwrap().modified().unwrap(), stamp);
    assert!(!walk.exists());
    std::fs::remove_dir_all(lib).unwrap();
}

#[test]
fn a_retry_that_changes_nothing_writes_nothing_and_a_deleted_clip_stays_deleted() {
    let (mut s, lib) = common::session("fbx-port-retry-same");
    let to = lib.join("port");
    s.run("clip.port", &params(&to)).unwrap();
    let manifest = to.join(MANIFEST);
    let before = std::fs::read(&manifest).unwrap();
    let walk = to.join("animations/PackA - Walks/walk.clip.json");
    std::fs::remove_file(&walk).unwrap();

    let mut p = params(&to);
    p["retry_refused"] = json!(true);
    let r = s.run("clip.port", &p).unwrap();
    // the 2 refused are refused the same way: 6 finished + 2 unchanged; missing.fbx is still away
    assert_eq!(r["new"], 0, "{r}");
    assert_eq!(r["skipped"], 8, "{r}");
    assert_eq!(r["refused"], 2, "{r}");
    assert_eq!(r["imported"], 5, "{r}");
    assert_eq!(r["duplicate"], 1, "{r}");
    assert_eq!(std::fs::read(&manifest).unwrap(), before);
    assert!(!walk.exists(), "a deleted clip stays deleted");
    std::fs::remove_dir_all(lib).unwrap();
}

/// A tree of two copies of the walk cut in two, with a `.meta` that refuses each, and the params.
fn refused_tree(lib: &Path) -> Value {
    let list = lib.join("from/list.txt");
    let good = std::fs::read_to_string(repo().join("fixtures/data/walk-cut.fbx.meta")).unwrap();
    for pack in ["P", "Q"] {
        let dir = lib.join("from").join(pack).join("A");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(repo().join("fixtures/data/walk-cut.fbx"), dir.join("w.fbx")).unwrap();
        std::fs::write(
            dir.join("w.fbx.meta"),
            good.replace("boneName: Bone23", "boneName: Gone"),
        )
        .unwrap();
    }
    std::fs::write(&list, "P/A/w.fbx\nQ/A/w.fbx\n").unwrap();
    json!({
        "list": list.to_string_lossy(),
        "root": lib.join("from").to_string_lossy(),
        "to": lib.join("port").to_string_lossy(),
    })
}

#[test]
fn a_retry_replaces_a_refused_outcome_and_leaves_a_duplicate() {
    let (mut s, lib) = common::session("fbx-port-retry");
    let mut p = refused_tree(&lib);
    let r = s.run("clip.port", &p).unwrap();
    assert_eq!((&r["refused"], &r["duplicate"]), (&json!(1), &json!(1)), "{r}");

    // the cause is mended; without the flag a refused source stays as it was
    let good = std::fs::read(repo().join("fixtures/data/walk-cut.fbx.meta")).unwrap();
    std::fs::write(lib.join("from/P/A/w.fbx.meta"), &good).unwrap();
    assert_eq!(s.run("clip.port", &p).unwrap()["new"], 0);

    p["retry_refused"] = json!(true);
    let r = s.run("clip.port", &p).unwrap();
    assert_eq!(r["new"], 1, "{r}");
    assert_eq!(r["imported"], 1, "{r}");
    assert_eq!(r["refused"], 0, "{r}");
    assert_eq!(r["duplicate"], 1, "a duplicate isn't tried again: {r}");
    assert_eq!(r["clips"], 2, "{r}");
    let manifest = lib.join("port").join(MANIFEST);
    let text = std::fs::read_to_string(&manifest).unwrap();
    assert_eq!(text.lines().count(), 4, "the first line and 3 outcomes: {text}");
    assert_eq!(
        lines(&manifest)["P/A/w.fbx"]["outcome"],
        "imported",
        "the last line wins"
    );

    // a later run reads the last line too: delete a clip, and neither form brings it back
    let clip = lib.join("port/animations/P - A/w - Walk start.clip.json");
    std::fs::remove_file(&clip).unwrap();
    for retry in [true, false] {
        p["retry_refused"] = json!(retry);
        let r = s.run("clip.port", &p).unwrap();
        assert_eq!((&r["new"], &r["skipped"]), (&json!(0), &json!(2)), "{r}");
        assert_eq!(r["imported"], 1, "{r}");
    }
    assert!(!clip.exists());
    std::fs::remove_dir_all(lib).unwrap();
}

#[test]
fn a_source_tried_again_is_not_a_duplicate_of_itself() {
    let (mut s, lib) = common::session("fbx-port-retry-self");
    let from = lib.join("from/P/A");
    std::fs::create_dir_all(&from).unwrap();
    std::fs::copy(data().join("PackA/Walks/walk.fbx"), from.join("w.fbx")).unwrap();
    let list = lib.join("from/list.txt");
    std::fs::write(&list, "P/A/w.fbx\n").unwrap();
    let mut p = json!({
        "list": list.to_string_lossy(),
        "root": lib.join("from").to_string_lossy(),
        "to": lib.join("port").to_string_lossy(),
    });
    s.run("clip.port", &p).unwrap();
    // an older importer refused this very content: only the importer has changed since
    let manifest = lib.join("port").join(MANIFEST);
    let first = lines(&manifest)["P/A/w.fbx"].clone();
    let refused = json!({
        "source": "P/A/w.fbx",
        "outcome": "refused",
        "reason": "an older importer's reason",
        "content": first["content"],
    });
    std::fs::write(&manifest, format!("{{\"port_manifest\":1}}\n{refused}\n")).unwrap();
    std::fs::remove_dir_all(lib.join("port/animations")).unwrap();

    p["retry_refused"] = json!(true);
    let r = s.run("clip.port", &p).unwrap();
    assert_eq!(r["imported"], 1, "{r}");
    assert_eq!(r["duplicate"], 0, "{r}");
    std::fs::remove_dir_all(lib).unwrap();
}

#[test]
fn a_stopped_run_resumes_and_a_cut_off_line_is_done_again() {
    let (mut s, lib) = common::session("fbx-port-resume");
    let to = lib.join("port");
    let mut p = params(&to);
    p["limit"] = json!(2);
    let r = s.run("clip.port", &p).unwrap();
    assert_eq!(r["new"], 2, "{r}");
    assert_eq!(r["pending"], 7, "{r}");

    let manifest = to.join(MANIFEST);
    let whole = std::fs::read_to_string(&manifest).unwrap();
    // a run stopped in the middle of its third line
    std::fs::write(&manifest, format!("{whole}{{\"source\":\"PackA/Wa")).unwrap();
    let r = s.run("clip.port", &params(&to)).unwrap();
    assert_eq!(r["torn"], true, "{r}");
    assert_eq!(r["skipped"], 2, "{r}");
    assert_eq!(r["new"], 6, "{r}");
    let text = std::fs::read_to_string(&manifest).unwrap();
    assert!(text.starts_with(&whole));
    assert_eq!(text.lines().count(), 9, "the first line and 8 outcomes: {text}");
    assert_eq!(lines(&manifest).len(), 8);

    // a line that isn't the last is our own file broken: refused, and left alone
    let broken = text.replacen("\"source\"", "\"source", 1);
    std::fs::write(&manifest, &broken).unwrap();
    let e = s.run("clip.port", &params(&to)).unwrap_err().to_string();
    assert!(e.contains("line 2"), "{e}");
    assert_eq!(std::fs::read_to_string(&manifest).unwrap(), broken);
    std::fs::remove_dir_all(lib).unwrap();
}

#[test]
fn a_file_that_comes_back_is_done_then() {
    let (mut s, lib) = common::session("fbx-port-back");
    let from = lib.join("from/P/A");
    std::fs::create_dir_all(&from).unwrap();
    let list = lib.join("from/list.txt");
    // a repeated line and a blank one count once and not at all; an odd file is refused
    std::fs::write(&list, "\u{feff}P/A/w.fbx\r\n\r\nP/A/w.fbx\r\nP/A/notes.txt\r\n").unwrap();
    let p = json!({
        "list": list.to_string_lossy(),
        "root": lib.join("from").to_string_lossy(),
        "to": lib.join("port").to_string_lossy(),
    });
    let r = s.run("clip.port", &p).unwrap();
    assert_eq!(r["sources"], 2, "{r}");
    assert_eq!(r["refused"], 1, "{r}");
    assert_eq!(r["unreachable"], json!(["P/A/w.fbx"]));
    std::fs::copy(data().join("PackA/Walks/walk.fbx"), from.join("w.fbx")).unwrap();
    let r = s.run("clip.port", &p).unwrap();
    assert_eq!(r["new"], 1, "{r}");
    assert_eq!(r["imported"], 1, "{r}");
    assert_eq!(r["pending"], 0, "{r}");
    std::fs::remove_dir_all(lib).unwrap();
}

#[test]
fn a_copy_cut_differently_is_not_a_duplicate() {
    let (mut s, lib) = common::session("fbx-port-meta");
    let (a, b) = (lib.join("from/P/A"), lib.join("from/Q/A"));
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    for d in [&a, &b] {
        std::fs::write(d.join("t.fbx"), takes_file()).unwrap();
    }
    std::fs::write(a.join("t.fbx.meta"), takes_meta()).unwrap();
    std::fs::write(
        b.join("t.fbx.meta"),
        takes_meta().replace("name: First", "name: Opening"),
    )
    .unwrap();
    let list = lib.join("from/list.txt");
    std::fs::write(&list, "P/A/t.fbx\nQ/A/t.fbx\n").unwrap();
    let r = s
        .run(
            "clip.port",
            &json!({
                "list": list.to_string_lossy(),
                "root": lib.join("from").to_string_lossy(),
                "to": lib.join("port").to_string_lossy(),
            }),
        )
        .unwrap();
    assert_eq!(r["imported"], 2, "{r}");
    assert_eq!(r["duplicate"], 0, "{r}");
    std::fs::remove_dir_all(lib).unwrap();
}

#[test]
fn a_port_refuses_to_write_inside_a_repository() {
    let (mut s, lib) = common::session("fbx-port-repo");
    let inside = repo().join("target/never-written");
    let e = s.run("clip.port", &params(&inside)).unwrap_err().to_string();
    assert!(e.contains("inside the repository"), "{e}");
    assert!(!inside.exists());
    // a root the paths don't lie under stops the run, and no file is given an outcome for it
    let to = lib.join("port");
    let mut p = params(&to);
    p["root"] = json!(lib.to_string_lossy());
    let e = s.run("clip.port", &p).unwrap_err().to_string();
    assert!(e.contains("under the root"), "{e}");
    assert!(!to.join(MANIFEST).exists());
    std::fs::remove_dir_all(lib).ok();
}
