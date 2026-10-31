//! A long fill keeps its feet planted (roadmap 167000), on the scene of `ai-fill-long-gap`: the worst
//! slide is under the 2 cm line, and the keys and the anchor bands are still there.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use ac_core::clip::KeySource;
use serde_json::json;

#[test]
fn a_long_fill_slides_under_the_line_and_meets_its_keys() {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let library = std::env::temp_dir().join(format!("ac-long-fill-feet-{}", std::process::id()));
    std::fs::create_dir_all(&library).unwrap();
    let mut s = ac_io::setup::SessionBuilder::new(&assets)
        .library(&library)
        .ai(ac_ai::shared(&assets))
        .strict()
        .build()
        .unwrap()
        .session;
    s.run("physics.set", &json!({ "on": false })).unwrap();
    s.run("ai.settings", &json!({ "ai_force": 1, "style": "any" })).unwrap();
    s.run("clip.open", &json!({ "id": "Locomotion/Walk" })).unwrap();
    s.run("keys.copy", &json!({ "frames": [8] })).unwrap();
    s.run("pose.reset", &json!({})).unwrap();
    s.run("clip.new", &json!({ "name": "Long gap", "group": "Mine" })).unwrap();
    s.run("keys.paste", &json!({ "at": 90 })).unwrap();
    s.run("pose.set_root", &json!({ "root": [0, -2.2993044, 0.9027198] })).unwrap();
    s.run("key.set", &json!({})).unwrap();
    let mine = |s: &ac_core::Session| -> Vec<_> {
        s.document
            .clip
            .keys()
            .iter()
            .filter(|k| k.source == KeySource::Manual)
            .cloned()
            .collect()
    };
    let before = mine(&s);

    let out = s.run("ai.fill", &json!({ "from": 0, "to": 90 })).unwrap();

    let slide = out["diagnostics"]["sliding_max_cm"].as_f64().unwrap();
    assert!(slide < 2.0, "the feet slide {slide} cm");
    assert_eq!(mine(&s), before, "the fill changed a key of yours");
    let anchors: Vec<i64> = s
        .document
        .clip
        .keys()
        .iter()
        .filter(|k| ac_core::anchors::is_anchor(&k.model))
        .map(|k| k.frame)
        .collect();
    assert_eq!(anchors, [30, 60]);
    std::fs::remove_dir_all(&library).unwrap();
}
