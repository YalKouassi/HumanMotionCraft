//! Project files: save, reset, open and save again gives the same bytes; a version-0 file
//! migrates; a newer version, a corrupt file and another rig's file are refused with the document
//! unchanged.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{repo, session};
use serde_json::json;

fn data(name: &str) -> String {
    repo()
        .join("fixtures/data")
        .join(name)
        .to_string_lossy()
        .replace('\\', "/")
}

#[test]
fn save_open_save_is_byte_identical() {
    let (mut s, lib) = session("project-roundtrip");
    s.run("clip.new", &json!({ "name": "Hop", "group": "Mine" }))
        .unwrap();
    s.run("frame.set", &json!({ "frame": 10 })).unwrap();
    s.run("pose.set_root", &json!({ "root": [0.2, 0.0, 1.1] }))
        .unwrap();
    s.run("key.set", &json!({})).unwrap();
    s.run(
        "layer.add",
        &json!({ "source": "link", "link": "Locomotion/Walk", "parts": ["Upper body"] }),
    )
    .unwrap();
    s.run("surface.add", &json!({ "name": "Step", "top": 0.3 }))
        .unwrap();
    s.run("ai.settings", &json!({ "trajectory": 0.5 })).unwrap();
    s.run("frame.set", &json!({ "frame": 4 })).unwrap();
    let path = lib.join("scratch").join("hop.project.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let p = path.to_string_lossy().to_string();
    let first = s.run("project.save", &json!({ "path": p })).unwrap();
    let bytes1 = std::fs::read(&path).unwrap();
    assert_eq!(first["bytes"].as_u64().unwrap() as usize, bytes1.len());
    assert!(
        s.document.clip_id.is_none(),
        "the document belongs to the project now"
    );
    assert!(!s.document.unsaved());

    s.run("session.reset", &json!({})).unwrap();
    assert!(s.document.project.is_none());
    let opened = s.run("project.open", &json!({ "path": p })).unwrap();
    assert_eq!(opened["frame"], json!(4.0));
    assert_eq!(opened["links"], json!(["Locomotion/Walk"]));
    assert_eq!(opened["migrated"], json!(false));
    assert!(!s.document.unsaved());
    assert_eq!(s.document.ai_settings.trajectory, 0.5);
    // the open document is the file: an edit makes it unsaved, undo makes it clean again
    s.run("pose.set_root", &json!({ "root": [0.0, 0.0, 1.2] }))
        .unwrap();
    s.run("key.set", &json!({})).unwrap();
    assert!(s.document.unsaved());
    s.run("edit.undo", &json!({})).unwrap();
    assert!(!s.document.unsaved());

    // saved again without a path: the same file, the same bytes
    let second = s.run("project.save", &json!({})).unwrap();
    let bytes2 = std::fs::read(&path).unwrap();
    assert_eq!(bytes1, bytes2);
    assert_eq!(first["hash"], second["hash"]);
    let info = s.run("project.info", &json!({ "path": p })).unwrap();
    assert_eq!(info["hash"], first["hash"]);
    assert_eq!(info["clips"], json!(2));
    std::fs::remove_dir_all(lib).unwrap();
}

#[test]
fn a_version_0_file_migrates() {
    let (mut s, lib) = session("project-v0");
    let text = std::fs::read_to_string(data("project-v0.project.json")).unwrap();
    let loaded = ac_io::project::load_text(&text).unwrap();
    assert_eq!(loaded.version, 0);
    assert!(loaded.migrated());
    assert_eq!(loaded.frame, 7.0);
    assert_eq!(loaded.links.keys().collect::<Vec<_>>(), ["Mine/Base"]);
    let r = s
        .run(
            "project.open",
            &json!({ "path": data("project-v0.project.json") }),
        )
        .unwrap();
    assert_eq!(r["migrated"], json!(true));
    assert_eq!(r["version"], json!(0));
    // saved again, it is a version-1 file
    let path = lib.join("scratch").join("old.project.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    s.run("project.save", &json!({ "path": path.to_string_lossy() }))
        .unwrap();
    let again = ac_io::project::load_text(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(again.version, ac_io::project::VERSION);
    assert_eq!(again.clip, loaded.clip);
    assert_eq!(again.links, loaded.links);
    assert_eq!(again.frame, 7.0);
    std::fs::remove_dir_all(lib).unwrap();
}

#[test]
fn bad_files_are_refused_and_the_document_kept() {
    let (mut s, lib) = session("project-bad");
    s.run("clip.new", &json!({ "name": "Keep", "group": "Mine" }))
        .unwrap();
    let before = s.document.clone();
    let newer = s
        .run(
            "project.open",
            &json!({ "path": data("project-future.project.json") }),
        )
        .unwrap_err()
        .to_string();
    assert!(newer.contains("newer version"), "{newer}");
    assert!(newer.contains("project version 99"), "{newer}");
    let corrupt = s
        .run(
            "project.open",
            &json!({ "path": data("project-corrupt.project.json") }),
        )
        .unwrap_err()
        .to_string();
    assert!(corrupt.contains("clip:"), "{corrupt}");
    let rig = s
        .run(
            "project.open",
            &json!({ "path": repo().join("assets/rig/default_rig.json").to_string_lossy() }),
        )
        .unwrap_err()
        .to_string();
    assert!(rig.contains("not a project file"), "{rig}");
    // an animation file is named for what it is
    let clip = s.run("library.list", &json!({})).unwrap()["animations"][0]["path"].clone();
    if let Some(p) = clip.as_str() {
        let e = s
            .run("project.open", &json!({ "path": p }))
            .unwrap_err()
            .to_string();
        assert!(e.contains("animation file"), "{e}");
    }
    assert!(s.document == before && s.document.project.is_none());
    assert!(
        s.run("project.save", &json!({})).is_err(),
        "no path and no project yet"
    );
    std::fs::remove_dir_all(lib).unwrap();
}

#[test]
fn migrate_rejects_a_bad_version_0_links_list() {
    let v = json!({ "version": 0, "links": [{ "id": "A/a", "clip": {} }, { "id": "A/a", "clip": {} }] });
    let e = ac_io::project::migrate(v).unwrap_err();
    assert!(e.contains("twice"), "{e}");
    let v = json!({ "version": 1, "links": [1] });
    assert_eq!(
        ac_io::project::migrate(v.clone()).unwrap(),
        v,
        "only version 0 is touched"
    );
}
