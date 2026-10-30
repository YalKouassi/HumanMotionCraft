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
