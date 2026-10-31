//! The evaluation-only mark on the optional sample clips (roadmap 165000), on a made-up clip: the
//! mark follows the motion, and everything that sends motion out of the app refuses. No sample clip
//! is read here, and none may ship.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use ac_core::clip::{Clip, Interp, Key};
use ac_core::clipfile;
use ac_core::{Document, Session, Skeleton};
use ac_io::library::{Kind, Library};
use serde_json::{Value, json};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ac-samples-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn session(library: &Path) -> Session {
    let rig = std::fs::read_to_string(repo().join("assets/rig/default_rig.json")).unwrap();
    let mut doc = Document::new(Skeleton::from_json(&rig).unwrap());
    doc.library = Some(library.to_path_buf());
    doc.assets = Some(repo().join("assets"));
    let mut s = Session::new(doc);
    let mut clashes = Vec::new();
    for specs in ac_io::ALL {
        clashes.extend(s.register(specs));
    }
    clashes.extend(s.register(ac_posefill::SPECS));
    assert!(clashes.is_empty(), "{clashes:?}");
    s
}

fn clip(doc: &Document, mark: bool) -> Clip {
    let mut c = Clip::new(doc.skeleton.names.clone(), 30.0);
    c.evaluation_only = mark;
    c.group = "Mine".into();
    for f in [0, 30] {
        let rest = vec![[0.0, 0.0, 0.0, 1.0]; doc.skeleton.names.len()];
        c.set_key(Key::new(f, rest, [0.0, 0.0, f as f64 / 100.0], Interp::Linear))
            .unwrap();
    }
    c
}

/// A library with a marked clip and a clean one.
fn with_clips(name: &str) -> (Session, PathBuf) {
    let lib = scratch(name);
    let s = session(&lib);
    let anims = Library::new(&lib, Kind::Animations);
    anims
        .write("Mine/marked", &clipfile::save_clip(&clip(&s.document, true)))
        .unwrap();
    anims
        .write("Mine/clean", &clipfile::save_clip(&clip(&s.document, false)))
        .unwrap();
    (s, lib)
}

fn marked(s: &mut Session) -> bool {
    s.run("editor.state", &json!({})).unwrap()["evaluation_only"] == true
}

#[test]
fn the_file_keeps_the_mark_and_a_clean_file_does_not_mention_it() {
    let (s, _) = with_clips("file");
    let text = clipfile::save_clip(&clip(&s.document, true));
    assert!(clipfile::load_clip(&text).unwrap().evaluation_only);
    let clean = clipfile::save_clip(&clip(&s.document, false));
    assert!(!clean.contains("evaluation_only"));
    assert!(!clipfile::load_clip(&clean).unwrap().evaluation_only);
}

#[test]
fn a_copied_pose_marks_where_it_lands() {
    let (mut s, _) = with_clips("pose");
    s.run("clip.open", &json!({ "id": "Mine/marked" })).unwrap();
    s.run("pose.copy", &json!({})).unwrap();
    s.run("clip.open", &json!({ "id": "Mine/clean" })).unwrap();
    assert!(!marked(&mut s));
    s.run("pose.paste", &json!({})).unwrap();
    assert!(marked(&mut s));
    s.run("edit.undo", &json!({})).unwrap();
    assert!(!marked(&mut s), "undoing the paste takes the mark back off");
}

#[test]
fn copied_keys_mark_where_they_land() {
    let (mut s, _) = with_clips("keys");
    s.run("clip.open", &json!({ "id": "Mine/marked" })).unwrap();
    s.run("keys.copy", &json!({ "frames": [0] })).unwrap();
    s.run("clip.open", &json!({ "id": "Mine/clean" })).unwrap();
    s.run("keys.paste", &json!({ "at": 10 })).unwrap();
    assert!(marked(&mut s));
}

#[test]
fn a_saved_pose_carries_the_mark() {
    let (mut s, _) = with_clips("saved");
    s.run("clip.open", &json!({ "id": "Mine/marked" })).unwrap();
    let id = s.run("pose.save", &json!({ "name": "from sample" })).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    s.run("clip.open", &json!({ "id": "Mine/clean" })).unwrap();
    s.run("pose.apply", &json!({ "id": id })).unwrap();
    assert!(marked(&mut s));
}

#[test]
fn a_new_animation_from_marked_motion_is_marked() {
    let (mut s, _) = with_clips("new");
    s.run("clip.open", &json!({ "id": "Mine/marked" })).unwrap();
    s.run("clip.new", &json!({ "name": "started there", "group": "Mine" }))
        .unwrap();
    assert!(marked(&mut s));
    s.run("clip.open", &json!({ "id": "Mine/clean" })).unwrap();
    s.run(
        "clip.new",
        &json!({ "name": "links it", "group": "Mine", "link": "Mine/marked" }),
    )
    .unwrap();
    assert!(marked(&mut s));
}

#[test]
fn saving_under_a_new_name_keeps_the_mark() {
    let (mut s, lib) = with_clips("saveas");
    s.run("clip.open", &json!({ "id": "Mine/marked" })).unwrap();
    s.run("clip.save_as", &json!({ "name": "renamed", "group": "Mine" }))
        .unwrap();
    let text = Library::new(&lib, Kind::Animations).read("Mine/renamed").unwrap();
    assert!(clipfile::load_clip(&text).unwrap().evaluation_only);
}

#[test]
fn exports_refuse_and_say_why() {
    let (mut s, lib) = with_clips("export");
    let out = scratch("export-out");
    s.run("clip.open", &json!({ "id": "Mine/marked" })).unwrap();
    let glb = out.join("a.glb");
    let mut refused: Vec<(&str, Value)> = vec![
        ("clip.export", json!({ "path": glb })),
        ("clip.export", json!({ "path": glb, "ids": ["Mine/marked"] })),
        ("clip.write", json!({ "path": out.join("a.clip.json") })),
    ];
    for (id, params) in refused.drain(..) {
        let e = s.run(id, &params).unwrap_err().to_string();
        assert!(e.contains("evaluation-only"), "{id}: {e}");
    }
    assert!(
        std::fs::read_dir(&out).unwrap().next().is_none(),
        "nothing was written"
    );
    let b = s
        .run(
            "clip.export_batch",
            &json!({ "ids": ["Mine/marked", "Mine/clean"], "folder": out }),
        )
        .unwrap();
    assert_eq!(b["files"].as_array().map(Vec::len), Some(1), "{b}");
    assert!(b["failed"].to_string().contains("evaluation-only"), "{b}");
    drop(lib);
}

#[test]
fn a_clip_linking_a_marked_clip_cannot_leave_by_id() {
    let (mut s, lib) = with_clips("link");
    let anims = Library::new(&lib, Kind::Animations);
    let mut c = clip(&s.document, false);
    let mut lay = ac_core::clip::Layer::keys("link", Vec::new());
    lay.source = ac_core::clip::LayerSource::Link;
    lay.link = "Mine/marked".into();
    c.layers.push(lay);
    anims.write("Mine/links", &clipfile::save_clip(&c)).unwrap();
    let out = scratch("link-out");
    let e = s
        .run(
            "clip.export",
            &json!({ "path": out.join("a.glb"), "ids": ["Mine/links"] }),
        )
        .unwrap_err()
        .to_string();
    assert!(e.contains("evaluation-only"), "{e}");
    s.run("clip.open", &json!({ "id": "Mine/links" })).unwrap();
    assert!(
        marked(&mut s),
        "opening it marks it: the link could be baked into keys"
    );
}

#[test]
fn the_shipped_library_holds_nothing_marked() {
    for kind in [Kind::Animations, Kind::Poses] {
        let lib = Library::new(&repo().join("assets/library"), kind);
        for item in lib.items() {
            assert_ne!(item["evaluation_only"], true, "{}", item["id"]);
        }
    }
}

#[test]
fn start_refuses_without_the_persons_acceptance() {
    let lib = scratch("accept");
    let mut s = session(&lib);
    let e = s
        .run("posefill.samples", &json!({ "action": "start" }))
        .unwrap_err()
        .to_string();
    assert!(e.contains("licence accepted first"), "{e}");
    let e = s
        .run("posefill.samples", &json!({ "action": "start", "accept": false }))
        .unwrap_err()
        .to_string();
    assert!(e.contains("licence accepted first"), "{e}");
}
