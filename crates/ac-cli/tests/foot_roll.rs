//! `feet.roll` is for stiff feet: motion whose feet already roll, and a stand, stay as they are.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use serde_json::json;

#[test]
fn recorded_motion_and_standing_stay_as_they_are() {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let lib = std::env::temp_dir().join(format!(
        "{}-foot-roll-{}",
        ac_core::product::ID,
        std::process::id()
    ));
    let mut s = ac_cli::session(&assets, &lib).unwrap();
    for id in [
        "Locomotion/Walk",
        "Locomotion/Run",
        "Standing/Idle",
        "Locomotion/Side step 9",
    ] {
        s.run("clip.open", &json!({ "id": id })).unwrap();
        let out = s.run("feet.roll", &json!({})).unwrap();
        assert_eq!(out["frames"], 0, "{id}: {out}");
    }
    std::fs::remove_dir_all(&lib).ok();
}

/// `hips.transfer` is for a body falling to the side: the recorded walks stay exactly as they are.
#[test]
fn recorded_walks_keep_their_weight() {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let lib = std::env::temp_dir().join(format!(
        "{}-transfer-{}",
        ac_core::product::ID,
        std::process::id()
    ));
    let mut s = ac_cli::session(&assets, &lib).unwrap();
    for n in 12..=17 {
        let id = format!("Locomotion/Walk {n}");
        s.run("clip.open", &json!({ "id": id })).unwrap();
        let out = s.run("hips.transfer", &json!({})).unwrap();
        assert_eq!(out["frames"], 0, "{id}: {out}");
    }
    std::fs::remove_dir_all(&lib).ok();
}
