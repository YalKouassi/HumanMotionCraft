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

/// A folder of its own for a test about files, gone first if a last run left it.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "{}-index-{name}-{}",
        ac_core::product::ID,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Ten frames at 30 fps: `frames` 10, `seconds` 1/3, by hand.
const TEN: &str = r#"{"keys":[{"frame":0},{"frame":9}],"fps":30,"loop":true}"#;

#[test]
fn the_index_lists_a_file_without_reading_it_until_it_changes() {
    use ac_io::library::{Kind, Library};
    let dir = scratch("reads");
    let file = dir.join("animations/Pack/A.clip.json");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, TEN).unwrap();
    std::fs::write(dir.join("animations/Pack/Bad.clip.json"), "not json").unwrap();
    let lib = Library::new(&dir, Kind::Animations);
    let index = dir.join(".index/animations.json");

    // items() writes nothing; listed() is the same list, and the same again from the index
    let plain = lib.items();
    assert!(!index.exists());
    assert_eq!(lib.listed(), plain);
    assert_eq!(lib.listed(), plain);
    assert_eq!(plain[0]["frames"], 10);
    assert_eq!(plain[0]["seconds"], json!(10.0 / 30.0));

    // a mark put in the index comes back: the file wasn't opened
    let mut saved: Value = serde_json::from_str(&std::fs::read_to_string(&index).unwrap()).unwrap();
    assert!(saved["items"]["Pack/A"].is_object());
    // an unreadable file isn't kept: it is tried again
    assert!(saved["items"].get("Pack/Bad").is_none());
    saved["items"]["Pack/A"]["meta"]["frames"] = json!(999);
    std::fs::write(&index, saved.to_string()).unwrap();
    assert_eq!(lib.listed()[0]["frames"], 999);

    // a changed size is read again: 21 frames
    std::fs::write(
        &file,
        r#"{"keys":[{"frame":20}],"fps":30,"loop":false,"x":"longer than it was"}"#,
    )
    .unwrap();
    assert_eq!(lib.listed()[0]["frames"], 21);

    // a removed file leaves the list and the index
    std::fs::remove_file(&file).unwrap();
    assert_eq!(lib.listed().len(), 1);
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&index).unwrap()).unwrap();
    assert!(saved["items"].get("Pack/A").is_none());

    // an index that makes no sense is only read again
    std::fs::write(&index, "not json").unwrap();
    assert_eq!(lib.listed().len(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn many_files_read_on_threads_come_back_in_order() {
    use ac_io::library::{Kind, Library};
    // above THREADED_FROM (32), and natural order: Clip 2 before Clip 10
    let dir = scratch("threads");
    let group = dir.join("animations/Pack");
    std::fs::create_dir_all(&group).unwrap();
    for i in 0..100 {
        let frames = format!(r#"{{"keys":[{{"frame":{i}}}],"fps":30}}"#);
        std::fs::write(group.join(format!("Clip {i}.clip.json")), frames).unwrap();
    }
    let got = Library::new(&dir, Kind::Animations).listed();
    assert_eq!(got.len(), 100);
    for (i, item) in got.iter().enumerate() {
        assert_eq!(item["name"], format!("Clip {i}"));
        assert_eq!(item["frames"], i + 1);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

/// A session on `dir` as the app makes one with `--library <dir>`.
fn opened(dir: &std::path::Path) -> ac_io::setup::Built {
    ac_io::setup::SessionBuilder::new(&common::repo().join("assets"))
        .library(dir)
        .limits(false)
        .build()
        .unwrap()
}

#[test]
fn a_folder_with_animations_opens_as_a_library_and_built_ins_stay_marked() {
    let dir = scratch("opens");
    let group = dir.join("animations/Pack");
    std::fs::create_dir_all(&group).unwrap();
    std::fs::write(group.join("A.clip.json"), TEN).unwrap();
    let mut built = opened(&dir);
    assert!(built.notes.iter().all(|n| !n.contains("can't list")));
    let list = built.session.run("library.list", &json!({})).unwrap();
    let items = list["animations"].as_array().unwrap();
    let mine = items.iter().find(|a| a["id"] == "Pack/A").unwrap();
    assert_eq!(mine["builtin"], false);
    assert_eq!(mine["frames"], 10);
    let walk = items.iter().find(|a| a["id"] == "Locomotion/Walk").unwrap();
    assert_eq!(walk["builtin"], true);
    // deleted through the command the panel uses: the file goes
    built
        .session
        .run("clip.delete", &json!({ "id": "Pack/A" }))
        .unwrap();
    assert!(!group.exists(), "the empty group goes with its last clip");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_folder_of_clips_without_animations_says_so_and_gets_nothing_put_in_it() {
    let dir = scratch("stray");
    std::fs::create_dir_all(dir.join("Pack")).unwrap();
    std::fs::write(dir.join("Pack/A.clip.json"), TEN).unwrap();
    std::fs::write(dir.join("B.clip.json"), TEN).unwrap();
    assert_eq!(ac_io::library::clips_outside_layout(&dir), 2);
    let built = opened(&dir);
    assert!(built.installed.is_none());
    assert!(
        built.notes.iter().any(|n| n.contains("holds 2 clips")),
        "{:?}",
        built.notes
    );
    assert!(!dir.join("animations").exists());
    std::fs::remove_dir_all(dir).unwrap();
}
