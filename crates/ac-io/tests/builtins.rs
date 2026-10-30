//! `library.sync`: the installed built-ins follow the shipped ones (fixed, new, retired) and the
//! person's own work and changes are left alone.
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
