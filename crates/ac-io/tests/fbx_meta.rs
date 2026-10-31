//! A Unity `.meta` beside an FBX: `fixtures/data/walk-cut.fbx` is the generated walk of `fbx.rs`
//! with joints named `Bone00`, ... so that the name guess finds nothing, and its `.meta` (both from
//! `python -m ac_train.fbxwrite`) names the bones and cuts the 40 frames into two clips. The cuts
//! are compared with the same walk imported whole under its readable names.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use ac_core::Skeleton;
use ac_core::clipfile;
use ac_io::fbx::unity_meta::{self, UnityMeta};
use ac_io::fbx::{import_bytes, open_bytes};
use serde_json::json;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn skeleton() -> Skeleton {
    Skeleton::from_json(&std::fs::read_to_string(repo().join("assets/rig/default_rig.json")).unwrap())
        .unwrap()
}

fn data(name: &str) -> Vec<u8> {
    std::fs::read(repo().join("fixtures/data").join(name)).unwrap()
}

fn fbx() -> Vec<u8> {
    data("walk-cut.fbx")
}

fn meta_text() -> String {
    String::from_utf8(data("walk-cut.fbx.meta")).unwrap()
}

fn meta() -> UnityMeta {
    unity_meta::parse(&meta_text()).unwrap()
}

/// The fixture's `.meta` with one piece of text changed.
fn meta_with(from: &str, to: &str) -> UnityMeta {
    let text = meta_text();
    assert!(text.contains(from), "{from}");
    unity_meta::parse(&text.replace(from, to)).unwrap()
}

fn none() -> BTreeMap<String, String> {
    BTreeMap::new()
}

#[test]
fn the_cut_file_can_only_be_read_through_its_bone_map() {
    let e = import_bytes(&fbx(), &skeleton(), "w", &none()).unwrap_err();
    assert!(e.contains("not a humanoid") && e.contains("Bone00"), "{e}");
}

#[test]
fn a_bone_map_and_clip_ranges_import_as_named_clips() {
    let sk = skeleton();
    let m = meta();
    let source = open_bytes(&fbx(), "walk-cut", &none(), Some(&m)).unwrap();
    assert_eq!(source.clips().len(), 2);

    let start = source.clip(&sk, &source.clips()[0]).unwrap();
    assert_eq!(start.clip.name, "Walk start");
    assert_eq!(start.clip.length(), 20);
    assert!(start.clip.loop_);
    assert_eq!(start.report["bone_map"], "unity_meta");
    assert_eq!(start.report["clip"]["first"], 0.0);
    assert_eq!(start.report["clip"]["last"], 19.0);

    let end = source.clip(&sk, &source.clips()[1]).unwrap();
    assert_eq!(end.clip.name, "Walk end");
    assert_eq!(end.clip.length(), 20);
    assert!(!end.clip.loop_);

    // the same places as the walk imported whole under readable names: sideways and along the walk
    // exactly the source's, and height up to each clip's own floor (a few millimetres)
    let whole = import_bytes(&data("walk-yup-75.fbx"), &sk, "w", &none()).unwrap();
    assert_eq!(whole.report["bone_map"], "names");
    assert!(!whole.clip.loop_);
    for (cut, at) in [(&start, 0), (&end, 20)] {
        for (f, key) in cut.clip.keys().iter().enumerate() {
            let want = whole.clip.keys()[f + at].root;
            for axis in 0..3 {
                let off = (key.root[axis] - want[axis]).abs() * sk.hip_height;
                assert!(off < 0.02, "{} frame {f} axis {axis}: {off} m", cut.clip.name);
            }
        }
    }
}

#[test]
fn a_correction_outranks_the_bone_map() {
    let sk = skeleton();
    let m = meta();
    // Bone39 is the first end site, after the writer's 39 named joints: the left toe's end
    assert_ne!(m.bones["l_toe"], "Bone39");
    let mut j = none();
    j.insert("l_toe".to_string(), "Bone39".to_string());
    let s = open_bytes(&fbx(), "w", &j, Some(&m)).unwrap();
    let got = s.clip(&sk, &s.clips()[0]).unwrap();
    assert_eq!(got.report["joints"]["l_toe"], "Bone39");
    assert_eq!(got.report["given"], json!(["l_toe"]));
}

#[test]
fn a_meta_without_a_bone_map_falls_back_to_the_guess_and_says_so() {
    let sk = skeleton();
    let m = unity_meta::parse("ModelImporter:\n  humanDescription:\n    human: []\n").unwrap();
    let s = open_bytes(&data("walk-yup-75.fbx"), "w", &none(), Some(&m)).unwrap();
    let got = s.pick(&sk, None).unwrap();
    assert_eq!(got.report["bone_map"], "names");
    assert_eq!(got.clip.length(), 40);
    let notes = got.report["notes"].to_string();
    assert!(notes.contains("no humanoid bone map"), "{notes}");
    // no meta at all says nothing about one
    let got = import_bytes(&data("walk-yup-75.fbx"), &sk, "w", &none()).unwrap();
    assert!(!got.report["notes"].to_string().contains(".meta"));
}

#[test]
fn a_map_that_leaves_out_a_landmark_says_what_was_guessed() {
    // the readable file: the map names only the hips, the rest is guessed and listed
    let sk = skeleton();
    let m = unity_meta::parse(
        "ModelImporter:\n  humanDescription:\n    human:\n    - boneName: Hips\n      humanName: Hips\n",
    )
    .unwrap();
    let s = open_bytes(&data("walk-yup-75.fbx"), "w", &none(), Some(&m)).unwrap();
    let got = s.whole(&sk).unwrap();
    assert_eq!(got.report["bone_map"], "unity_meta");
    let notes = got.report["notes"].to_string();
    assert!(
        notes.contains("leaves out") && notes.contains("l_knee (LeftLeg)"),
        "{notes}"
    );
    // and on the unreadable file nothing can be guessed: refused, naming what is missing
    let e = open_bytes(&fbx(), "w", &none(), Some(&m)).unwrap_err();
    assert!(e.contains("Hips") && e.contains("no such joint"), "{e}");
}

#[test]
fn a_meta_of_another_file_or_a_missing_bone_is_refused() {
    let m = meta_with("boneName: Bone00", "boneName: NoSuchBone");
    let e = open_bytes(&fbx(), "w", &none(), Some(&m)).unwrap_err();
    assert!(e.contains("NoSuchBone") && e.contains("no such joint"), "{e}");
    // the file's own joints follow, so the fix can be written
    assert!(e.contains("Bone01"), "{e}");
    // with the bone gone from the map, the guess can't stand in for it
    let m = meta_with("humanName: Neck", "humanName: Left Index Proximal");
    let e = open_bytes(&fbx(), "w", &none(), Some(&m)).unwrap_err();
    assert!(e.contains("not a humanoid") && e.contains("neck"), "{e}");
}

#[test]
fn clips_outside_the_take_or_of_another_take_are_refused_by_name() {
    let sk = skeleton();
    for (m, want) in [
        (meta_with("lastFrame: 39", "lastFrame: 40"), "outside the take"),
        (meta_with("firstFrame: 20", "firstFrame: 39"), "fewer than 2"),
        (meta_with("firstFrame: 20", "firstFrame: 45"), "fewer than 2"),
    ] {
        let s = open_bytes(&fbx(), "w", &none(), Some(&m)).unwrap();
        // the first clip is untouched, and still imports
        assert_eq!(s.clip(&sk, &s.clips()[0]).unwrap().clip.length(), 20);
        let e = s.clip(&sk, &s.clips()[1]).unwrap_err();
        assert!(e.contains("Walk end") && e.contains(want), "{e}");
    }
    let m = meta_with("takeName: Take 001", "takeName: Another");
    let s = open_bytes(&fbx(), "w", &none(), Some(&m)).unwrap();
    let e = s.clip(&sk, &s.clips()[0]).unwrap_err();
    assert!(e.contains("Another") && e.contains("Take 001"), "{e}");
}

#[test]
fn pick_opens_the_named_clip_and_refuses_one_that_is_not_there() {
    let sk = skeleton();
    let m = meta();
    let s = open_bytes(&fbx(), "w", &none(), Some(&m)).unwrap();
    let got = s.pick(&sk, Some("Walk end")).unwrap();
    assert_eq!(got.clip.name, "Walk end");
    assert_eq!(got.report["clips"].as_array().unwrap().len(), 2);
    let got = s.pick(&sk, None).unwrap();
    assert_eq!(got.clip.name, "Walk start");
    assert!(got.report["notes"].to_string().contains("2 clips"));
    let e = s.pick(&sk, Some("Nope")).unwrap_err();
    assert!(e.contains("Nope") && e.contains("Walk start, Walk end"), "{e}");
    // a file with no clips has none to name
    let plain = open_bytes(&data("walk-yup-75.fbx"), "w", &none(), None).unwrap();
    assert!(
        plain
            .pick(&sk, Some("Walk"))
            .unwrap_err()
            .contains("no named clips")
    );
}

#[test]
fn a_batch_writes_a_file_per_clip_lists_a_refused_one_and_stays_in_its_folder() {
    let (mut s, lib) = common::session("fbx-meta-batch");
    let from = lib.join("from");
    let to = lib.join("to");
    std::fs::create_dir_all(&from).unwrap();
    std::fs::write(from.join("walk.fbx"), fbx()).unwrap();
    let text = meta_text()
        .replace("name: Walk start", "name: ../../up/and out")
        .replace("lastFrame: 39", "lastFrame: 400");
    std::fs::write(from.join("walk.fbx.meta"), text).unwrap();
    // an unreadable .meta refuses its file, and the others still go through
    std::fs::write(from.join("bad.fbx"), fbx()).unwrap();
    std::fs::write(from.join("bad.fbx.meta"), "DefaultImporter:\n  a: 1\n").unwrap();
    let r = s
        .run(
            "clip.import_batch",
            &json!({ "from": from.to_string_lossy(), "to": to.to_string_lossy() }),
        )
        .unwrap();
    let files = r["files"].as_array().unwrap();
    assert_eq!(files.len(), 1, "{r}");
    assert_eq!(files[0]["clip"], "../../up/and out");
    assert_eq!(files[0]["loop"], true);
    assert_eq!(files[0]["frames"], 20);
    let failed = r["failed"].as_array().unwrap();
    assert_eq!(failed.len(), 2, "{r}");
    assert_eq!(failed[0]["file"], "bad.fbx");
    assert!(
        failed[0]["error"].as_str().unwrap().contains("ModelImporter"),
        "{r}"
    );
    assert_eq!(failed[1]["clip"], "Walk end");
    assert!(
        failed[1]["error"].as_str().unwrap().contains("outside the take"),
        "{r}"
    );
    // the slashes became part of the name (its leading dots trimmed): one file, in the folder asked for
    let written: Vec<String> = std::fs::read_dir(&to)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(written, ["walk - _.._up_and out.clip.json"]);
    let saved = clipfile::load_clip(&std::fs::read_to_string(to.join(&written[0])).unwrap()).unwrap();
    assert!(saved.loop_, "the loop flag is kept in the clip file");
    assert_eq!(saved.name, "../../up/and out");
    std::fs::remove_dir_all(lib).unwrap();
}

#[test]
fn a_binary_meta_is_refused_beside_the_file() {
    let (_, lib) = common::session("fbx-meta-beside");
    std::fs::create_dir_all(&lib).unwrap();
    let path = lib.join("x.fbx");
    std::fs::write(&path, fbx()).unwrap();
    assert_eq!(
        unity_meta::beside(&path).unwrap(),
        None,
        "no .meta: nothing to say"
    );
    std::fs::write(lib.join("x.fbx.meta"), [0xFFu8, 0xFE, 0x00]).unwrap();
    assert!(unity_meta::beside(&path).unwrap_err().contains("not UTF-8"));
    std::fs::remove_dir_all(lib).unwrap();
}
