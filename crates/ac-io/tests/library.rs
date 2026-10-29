//! The library commands against a scratch library made from the built-ins.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use ac_core::{Document, Session};
use common::session;
use serde_json::{Value, json};

#[test]
fn open_save_reopen_is_the_same_clip() {
    let (mut s, lib) = session("roundtrip");
    let list = s.run("library.list", &json!({})).unwrap();
    assert!(
        list["animations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == "Locomotion/Walk")
    );
    s.run("clip.open", &json!({ "id": "Walk" })).unwrap(); // by name works too
    assert_eq!(s.document.clip_id.as_deref(), Some("Locomotion/Walk"));
    let walk = s.document.clip.clone();
    assert!(walk.length() > 10 && walk.loop_);
    s.run("clip.save_as", &json!({ "name": "My walk", "group": "Mine" }))
        .unwrap();
    s.run("clip.open", &json!({ "id": "Locomotion/Walk" })).unwrap();
    s.run("clip.open", &json!({ "id": "Mine/My walk" })).unwrap();
    let mut mine = s.document.clip.clone();
    mine.name = walk.name.clone();
    mine.group = walk.group.clone();
    mine.origin = walk.origin.clone();
    assert_eq!(mine, walk);
    std::fs::remove_dir_all(lib).unwrap();
}

#[test]
fn poses_apply_and_parts_persist() {
    let (mut s, lib) = session("poses");
    s.run("clip.new", &json!({ "name": "Test", "group": "Mine" }))
        .unwrap();
    s.run("frame.set", &json!({ "frame": 12 })).unwrap();
    s.run(
        "pose.apply",
        &json!({ "id": "Reference/T-pose", "parts": ["Left arm"] }),
    )
    .unwrap();
    let st = s.run("editor.state", &json!({})).unwrap();
    assert_eq!(st["keys"].as_array().unwrap().len(), 2);
    assert_eq!(st["keys"][1]["part"], "Left arm");
    s.run(
        "parts.save",
        &json!({ "name": "Both arms", "parts": ["Left arm", "Right arm"] }),
    )
    .unwrap();
    s.run(
        "pose.save",
        &json!({ "name": "Arms", "group": "Mine", "parts": ["Both arms"] }),
    )
    .unwrap();
    let list = s.run("library.list", &json!({})).unwrap();
    let arms = list["poses"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "Mine/Arms")
        .cloned()
        .unwrap();
    assert_eq!(arms["part"], "Both arms");
    // a fresh session reads the part sets back
    let mut doc = Document::new(s.document.skeleton.as_ref().clone());
    doc.library = Some(lib.clone());
    ac_io::commands::load_parts(&mut doc).unwrap();
    assert!(doc.custom_parts.contains_key("Both arms"));
    assert!(
        s.run("parts.save", &json!({ "name": "Whole body", "parts": ["Head"] }))
            .is_err()
    );
    std::fs::remove_dir_all(lib).unwrap();
}

#[test]
fn a_linked_animation_plays_under_my_keys() {
    let (mut s, lib) = session("link");
    s.run(
        "clip.new",
        &json!({ "name": "Walk and wave", "link": "Locomotion/Walk", "parts": ["Right arm"] }),
    )
    .unwrap();
    assert_eq!(s.document.clip.layers.len(), 3);
    assert!(s.document.links.contains_key("Locomotion/Walk"));
    // the legs move with the linked walk
    let foot = |s: &mut Session, f: f64| -> Value {
        s.run("frame.set", &json!({ "frame": f })).unwrap();
        s.run("bone.world", &json!({ "bone": "foot.L" })).unwrap()["position"].clone()
    };
    let a = foot(&mut s, 0.0);
    let b = foot(&mut s, 8.0);
    assert_ne!(a, b);
    s.run(
        "clip.delete",
        &json!({ "id": s.document.clip_id.clone().unwrap() }),
    )
    .unwrap();
    assert!(s.document.clip_id.is_none());
    assert!(s.run("clip.open", &json!({ "id": "No/Such" })).is_err());
    std::fs::remove_dir_all(lib).unwrap();
}

#[test]
fn a_folder_imports_with_subfolders_as_groups_and_skips_what_wont_read() {
    let (mut s, lib) = session("import");
    let src = lib.with_extension("in").join("My moves");
    let _ = std::fs::remove_dir_all(&src);
    std::fs::create_dir_all(src.join("Moves")).unwrap();
    std::fs::create_dir_all(src.join(".thumbs")).unwrap();
    let walk = std::fs::read_to_string(lib.join("animations/Locomotion/Walk.clip.json")).unwrap();
    let tpose = std::fs::read_to_string(lib.join("poses/Reference/T-pose.pose.json")).unwrap();
    let mut other: Value = serde_json::from_str(&walk).unwrap();
    other["bones"] = json!(["root"]);
    for (file, text) in [
        ("Walk.clip.json", walk.clone()),
        ("Moves/Walk.clip.json", walk.clone()),
        ("T.pose.json", tpose),
        ("Bad.clip.json", "not json".to_string()),
        ("Other rig.clip.json", other.to_string()),
        (".thumbs/Hidden.clip.json", walk.clone()),
        ("notes.txt", "not an item".to_string()),
    ] {
        std::fs::write(src.join(file), text).unwrap();
    }
    let from = json!({ "from": src.to_string_lossy() });
    let r = s.run("library.import", &from).unwrap();
    assert_eq!(r["group"], "My moves");
    assert_eq!(r["animations"]["copied"], json!(["My moves/Walk", "Moves/Walk"]));
    assert_eq!(r["poses"]["copied"], json!(["My moves/T"]));
    let skipped: Vec<&str> = r["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["file"].as_str().unwrap())
        .collect();
    assert_eq!(skipped, ["Bad.clip.json", "Other rig.clip.json"]);
    assert!(r.get("cut").is_none());

    // again: everything is already there and kept, unless overwrite
    let r = s.run("library.import", &from).unwrap();
    assert_eq!(r["animations"]["copied"], json!([]));
    assert_eq!(r["animations"]["kept"], json!(["My moves/Walk", "Moves/Walk"]));
    std::fs::write(lib.join("animations/Moves/Walk.clip.json"), "edited").unwrap();
    let r = s
        .run(
            "library.import",
            &json!({ "from": src.to_string_lossy(), "overwrite": true }),
        )
        .unwrap();
    assert_eq!(r["animations"]["copied"], json!(["My moves/Walk", "Moves/Walk"]));
    assert_eq!(
        std::fs::read_to_string(lib.join("animations/Moves/Walk.clip.json")).unwrap(),
        walk
    );
    s.run("clip.open", &json!({ "id": "Moves/Walk" })).unwrap();

    // a folder with only files that won't read is refused, naming the first
    let bad = lib.with_extension("bad");
    std::fs::create_dir_all(&bad).unwrap();
    std::fs::write(bad.join("Broken.clip.json"), "{").unwrap();
    let e = s
        .run("library.import", &json!({ "from": bad.to_string_lossy() }))
        .unwrap_err()
        .to_string();
    assert!(e.contains("Broken.clip.json"), "{e}");

    for d in [lib, src.parent().unwrap().to_path_buf(), bad] {
        std::fs::remove_dir_all(d).unwrap();
    }
}
