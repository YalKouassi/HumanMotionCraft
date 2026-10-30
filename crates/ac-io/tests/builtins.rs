//! `library.sync`: the installed built-ins follow the shipped ones (fixed, new, retired, moved to
//! the community) and the person's own work and changes are left alone.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

fn clip(name: &str, origin: &str, v: u32) -> String {
    json!({ "name": name, "origin": origin, "keys": [], "v": v }).to_string()
}

fn put(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn read(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join(rel)).unwrap()
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "{}-test-sync-{name}-{}",
        ac_core::product::ID,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn session(lib: &Path) -> ac_core::Session {
    ac_io::setup::SessionBuilder::new(&common::repo().join("assets"))
        .library(lib)
        .limits(false)
        .install(false)
        .strict()
        .build()
        .unwrap()
        .session
}

fn sync(s: &mut ac_core::Session, from: &Path) -> Value {
    s.run("library.sync", &json!({ "from": from.to_string_lossy() }))
        .unwrap()
}

#[test]
fn sync_fixes_adds_and_retires_built_ins_and_keeps_your_changes() {
    let (v1, v2, lib) = (scratch("v1"), scratch("v2"), scratch("lib"));
    for (rel, v) in [
        ("animations/G/Walk.clip.json", 1),
        ("animations/G/Old.clip.json", 1),
        ("animations/G/Edited.clip.json", 1),
        ("animations/G/Gone.clip.json", 1),
        ("poses/P/Stand.pose.json", 1),
    ] {
        put(&v1, rel, &clip(rel, "mocap", v));
    }
    let mut s = session(&lib);
    let r = s
        .run("library.install", &json!({ "from": v1.to_string_lossy() }))
        .unwrap();
    assert_eq!(r["copied"], 5);

    // the person: their own clip, an edited built-in, a deleted one
    put(&lib, "animations/Mine/Own.clip.json", &clip("Own", "", 1));
    put(
        &lib,
        "animations/G/Edited.clip.json",
        &clip("Edited", "mocap", 99),
    );
    std::fs::remove_file(lib.join("animations/G/Gone.clip.json")).unwrap();

    // the next release: Walk, Edited and Gone fixed, Old dropped, New added
    for (rel, v) in [
        ("animations/G/Walk.clip.json", 2),
        ("animations/G/Edited.clip.json", 2),
        ("animations/G/Gone.clip.json", 2),
        ("animations/H/New.clip.json", 1),
        ("poses/P/Stand.pose.json", 1),
    ] {
        put(&v2, rel, &clip(rel, "mocap", v));
    }
    let r = sync(&mut s, &v2);
    assert_eq!(
        r["animations"],
        json!({
            "added": ["H/New"],
            "updated": ["G/Walk"],
            "retired": ["G/Old"],
            "moved": [],
            "kept": [{ "id": "G/Edited", "why": "changed in your library: kept" }]
        })
    );
    assert_eq!(r["changed"], 3);
    assert_eq!(
        read(&lib, "animations/G/Walk.clip.json"),
        read(&v2, "animations/G/Walk.clip.json")
    );
    assert_eq!(
        read(&lib, "animations/G/Edited.clip.json"),
        clip("Edited", "mocap", 99)
    );
    assert!(
        !lib.join("animations/G/Gone.clip.json").exists(),
        "a deleted built-in stays deleted"
    );
    assert!(!lib.join("animations/G/Old.clip.json").exists());
    assert_eq!(
        read(&lib, ".retired/animations/G/Old.clip.json"),
        clip("animations/G/Old.clip.json", "mocap", 1)
    );
    assert_eq!(read(&lib, "animations/Mine/Own.clip.json"), clip("Own", "", 1));
    assert!(
        ac_io::builtins::summary(&r)
            .unwrap()
            .contains("1 new, 1 fixed, 1 retired")
    );

    // again: nothing to do
    let r = sync(&mut s, &v2);
    assert_eq!(r["changed"], 0);
    assert!(ac_io::builtins::summary(&r).is_none());

    for d in [v1, v2, lib] {
        std::fs::remove_dir_all(d).unwrap();
    }
}

/// A library installed before the record: items with an origin were built-ins; the rest are the
/// person's, even under a shipped name.
#[test]
fn a_library_without_the_record_goes_by_origin() {
    let (v2, lib) = (scratch("legacy-v2"), scratch("legacy-lib"));
    put(&v2, "animations/G/Walk.clip.json", &clip("Walk", "mocap", 2));
    put(&v2, "animations/G/Run.clip.json", &clip("Run", "mocap", 2));
    put(&lib, "animations/G/Walk.clip.json", &clip("Walk", "mocap", 1));
    put(&lib, "animations/G/Run.clip.json", &clip("Run", "", 7));
    put(&lib, "animations/Old/Dance.clip.json", &clip("Dance", "mocap", 1));
    put(
        &lib,
        "animations/Old/Dance copy.clip.json",
        &clip("Dance copy", "", 1),
    );
    let mut s = session(&lib);
    let r = sync(&mut s, &v2);
    assert_eq!(
        r["animations"],
        json!({
            "added": [],
            "updated": ["G/Walk"],
            "retired": ["Old/Dance"],
            "moved": [],
            "kept": [{ "id": "G/Run", "why": "yours, under a built-in's name: kept" }]
        })
    );
    assert_eq!(
        read(&lib, "animations/Old/Dance copy.clip.json"),
        clip("Dance copy", "", 1)
    );
    assert!(lib.join(ac_io::builtins::RECORD).is_file());
    assert_eq!(sync(&mut s, &v2)["changed"], 0);
    // not a built-in library: refused, nothing written
    assert!(
        s.run(
            "library.sync",
            &json!({ "from": lib.join("nowhere").to_string_lossy() })
        )
        .is_err()
    );
    for d in [v2, lib] {
        std::fs::remove_dir_all(d).unwrap();
    }
}

/// Built-ins that moved to the community's Official collection (roadmap 151000): a library from
/// before the move keeps every one, as a community install of its clip, and retires nothing.
#[test]
fn built_ins_that_moved_to_the_community_stay_as_community_installs() {
    let (v1, v2, lib, legacy) = (
        scratch("moved-v1"),
        scratch("moved-v2"),
        scratch("moved-lib"),
        scratch("moved-legacy"),
    );
    for (rel, v) in [
        ("animations/G/Walk.clip.json", 1),
        ("animations/S/Drunk.clip.json", 1),
        ("animations/S/Proud.clip.json", 1),
        ("animations/G/Old.clip.json", 1),
    ] {
        put(&v1, rel, &clip(rel, "mocap", v));
    }
    // the next release ships Walk only; Drunk and Proud moved, Old was dropped
    put(
        &v2,
        "animations/G/Walk.clip.json",
        &clip("animations/G/Walk.clip.json", "mocap", 1),
    );
    let moved = json!({ "animations": {
        "S/Drunk": { "clip": "c-drunk", "title": "Drunk walk", "author": "Official",
                     "licence": "CC-BY-4.0", "credit": "100STYLE" },
        "S/Proud": { "clip": "c-proud", "title": "Proud walk", "author": "Official",
                     "licence": "CC-BY-4.0", "credit": "100STYLE" },
    }});
    put(&v2, ac_io::builtins::MOVED, &moved.to_string());

    let mut s = session(&lib);
    s.run("library.install", &json!({ "from": v1.to_string_lossy() }))
        .unwrap();
    // the person changed Proud: it stays theirs, as a changed built-in does
    put(&lib, "animations/S/Proud.clip.json", &clip("Proud", "mocap", 9));
    let r = sync(&mut s, &v2);
    assert_eq!(
        r["animations"],
        json!({
            "added": [],
            "updated": [],
            "retired": ["G/Old"],
            "moved": ["S/Drunk"],
            "kept": [{ "id": "S/Proud", "why": "no longer shipped, changed in your library: kept" }]
        })
    );
    let drunk: Value = serde_json::from_str(&read(&lib, "animations/S/Drunk.clip.json")).unwrap();
    assert_eq!(drunk["origin"], "community:c-drunk");
    assert_eq!(drunk["v"], 1, "the clip itself is unchanged");
    let rec = ac_io::community_record::Record::load(&lib).unwrap();
    assert_eq!(rec.installs["S/Drunk"].clip, "c-drunk");
    assert_eq!(rec.installs["S/Drunk"].credit, "100STYLE");
    assert!(
        ac_io::builtins::summary(&r)
            .unwrap()
            .contains("1 now in the community (kept in your library)")
    );
    // again: nothing to do, and no longer a built-in
    assert_eq!(sync(&mut s, &v2)["changed"], 0);

    // a library from before the record: an item with an origin was a built-in, so it moves too
    put(
        &legacy,
        "animations/S/Drunk.clip.json",
        &clip("Drunk", "mocap", 1),
    );
    put(&legacy, "animations/S/Mine.clip.json", &clip("Mine", "", 1));
    let mut s = session(&legacy);
    let r = sync(&mut s, &v2);
    assert_eq!(r["animations"]["moved"], json!(["S/Drunk"]));
    assert_eq!(r["animations"]["retired"], json!([]));
    assert!(legacy.join("animations/S/Mine.clip.json").is_file());
    for d in [v1, v2, lib, legacy] {
        std::fs::remove_dir_all(d).unwrap();
    }
}
