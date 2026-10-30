//! A part layer changes its part and leaves the other bones alone (roadmap 116800).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;

/// A Left arm layer changes the left arm, and nothing else.
#[test]
fn a_part_layer_leaves_other_bones_alone() {
    let repo = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lib = std::env::temp_dir().join(format!(
        "{}-part-layer-{}",
        ac_core::product::ID,
        std::process::id()
    ));
    let mut s = ac_cli::session(&repo.join("assets"), &lib).unwrap();
    s.run("physics.set", &json!({ "on": false })).unwrap();
    s.run("pose.set_root", &json!({ "root": [0.1, 0.0, 0.0] }))
        .unwrap();
    s.run("key.set", &json!({})).unwrap();
    s.run("frame.set", &json!({ "frame": 20 })).unwrap();
    s.run("pose.set_root", &json!({ "root": [0.2, 0.0, 0.0] }))
        .unwrap();
    s.run("key.set", &json!({})).unwrap();
    let names = s.document.skeleton.names.clone();
    let frames: Vec<f64> = (0..=20).map(f64::from).collect();
    let (before, roots) = s.document.clip.sample_with(&frames, &s.document.links).unwrap();
    s.run("layer.add", &json!({ "name": "Arm", "parts": ["Left arm"] }))
        .unwrap();
    s.run("frame.set", &json!({ "frame": 10 })).unwrap();
    let arm = "upperarm01.L";
    s.run(
        "pose.set_bone",
        &json!({ "bone": arm, "rotation": [0.0, 0.0, 0.38268343236508984, 0.9238795325112867] }),
    )
    .unwrap();
    s.run("key.set", &json!({})).unwrap();
    let (after, roots2) = s.document.clip.sample_with(&frames, &s.document.links).unwrap();
    assert_eq!(roots, roots2);
    let i = names.iter().position(|n| n == arm).unwrap();
    assert_ne!(before[10][i], after[10][i], "the layer should move the arm");
    for (f, (a, b)) in before.iter().zip(&after).enumerate() {
        for (j, name) in names.iter().enumerate() {
            let in_arm = name.ends_with(".L")
                && [
                    "clavicle",
                    "shoulder",
                    "upperarm",
                    "lowerarm",
                    "wrist",
                    "finger",
                    "metacarpal",
                ]
                .iter()
                .any(|p| name.starts_with(p));
            if !in_arm {
                assert_eq!(a[j], b[j], "frame {f}: {name} changed");
            }
        }
    }
    std::fs::remove_dir_all(&lib).ok();
}
