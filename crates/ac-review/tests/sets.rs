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
    assert_eq!(m.rotations[10][0], [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(m.roots[10], [0.0, 0.0, 1.0]);
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
    assert_eq!((v["name"].as_str(), v["set_version"].as_u64()), (Some("a"), Some(1)));
    assert!(r.run("review.set_new", &p(json!({"name": "a"}))).is_err(), "no overwriting");
    assert!(r.run("review.set_new", &p(json!({"name": "../a"}))).is_err());

    let saved = |id: &str, labels: Value, r: &mut Review| {
        r.run("review.set_clip", &p(json!({"id": id, "labels": labels}))).unwrap()
    };
    saved("library:G/walk_01", json!(["walk"]), &mut r);
    saved("library:G/hop", json!(["hop"]), &mut r);
    let on_disk = TrainingSet::load(&folder, "a").unwrap();
    // 1 at the start, one for each change
    assert_eq!(on_disk.set_version, 3);
    assert_eq!(on_disk.clips.len(), 2);

    let v = r.run("review.set_duplicate", &p(json!({"as": "b"}))).unwrap();
    assert_eq!((v["name"].as_str(), v["clips"].as_u64(), v["set_version"].as_u64()), (Some("b"), Some(2), Some(1)));
    saved("library:G/walk_01", json!(["changed"]), &mut r);
    saved("library:H/run", json!(["run"]), &mut r);
    assert_eq!(TrainingSet::load(&folder, "a").unwrap(), on_disk, "the copy is its own");

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
    let q = r.queue(&Filter { show: Show::InSet, ..Filter::default() });
    assert_eq!(q.len(), 3);
    r.run("review.set_open", &p(json!({"name": null}))).unwrap();
    assert!(r.queue(&Filter { show: Show::InSet, ..Filter::default() }).is_empty());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn clips_are_kept_rejected_cut_and_removed_and_nonsense_refused() {
    let dir = scratch("clips");
    let mut r = review(&dir, &dir.join("sets"));
    assert!(
        r.run("review.set_clip", &p(json!({"id": "library:G/hop", "status": "keep"}))).is_err(),
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
    r.run("review.set_clip", &p(json!({"id": id, "cuts": [[5, 9], [8, 12]]}))).unwrap();
    let (s, t) = r.find(id).unwrap();
    assert_eq!(r.status_of(s, t).map(|x| x.word()), Some("cut"));
    let c = &r.training.active.as_ref().unwrap().clips[id];
    assert_eq!(c.cuts, [[5, 12]]);
    r.run("review.set_clip", &p(json!({"id": id, "status": "reject"}))).unwrap();
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
        .run("review.decide", &p(json!({"id": "library:G/hop", "status": "ok"})))
        .unwrap_err();
    assert!(e.contains("training set"), "{e}");
    assert!(!dir.join("decisions.json").exists());

    // a set with no private clips may live there; adding one is refused and leaves it as it was
    r.run("review.set_new", &p(json!({"name": "a"}))).unwrap();
    let before = std::fs::read_to_string(folder.join("a.set.json")).unwrap();
    let e = r
        .run("review.set_clip", &p(json!({"id": "library:G/hop", "status": "keep"})))
        .unwrap_err();
    assert!(e.contains("private"), "{e}");
    let a = r.training.active.as_ref().unwrap();
    assert!(a.clips.is_empty());
    assert_eq!(a.set_version, 1);
    assert_eq!(std::fs::read_to_string(folder.join("a.set.json")).unwrap(), before);

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
    let e = r.run("review.set_duplicate", &p(json!({"as": "copy"}))).unwrap_err();
    assert!(e.contains("private"), "{e}");
    assert!(!folder.join("copy.set.json").exists());
    std::fs::remove_dir_all(&dir).ok();
}
