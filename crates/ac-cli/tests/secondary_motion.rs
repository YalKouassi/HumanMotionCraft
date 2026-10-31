//! Secondary motion's acceptance on a hand-keyed walk (roadmap 119800): one undo step, muting gives
//! the clip back exactly, no key of the person's changes.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use ac_core::quat;
use ac_core::session::Session;
use serde_json::json;

fn session(tag: &str) -> (Session, std::path::PathBuf) {
    let repo = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lib = std::env::temp_dir().join(format!(
        "{}-secondary-{tag}-{}",
        ac_core::product::ID,
        std::process::id()
    ));
    (ac_cli::session(&repo.join("assets"), &lib).unwrap(), lib)
}

/// Stand, three metres of footstep keys with no AI, then a stop held for a second.
fn walk(s: &mut Session) {
    s.run("physics.set", &json!({ "on": false })).unwrap();
    s.run("pose.set_root", &json!({ "root": [0, 0, 0.945] })).unwrap();
    s.run("key.set", &json!({ "interp": "smooth" })).unwrap();
    s.run("frame.set", &json!({ "frame": 80 })).unwrap();
    s.run("pose.set_root", &json!({ "root": [0, -3.0, 0.945] }))
        .unwrap();
    s.run("key.set", &json!({ "interp": "smooth" })).unwrap();
    s.run(
        "anim.steps",
        &json!({ "from": 0, "to": 80, "steps": 6, "fill": "none" }),
    )
    .unwrap();
    s.run("frame.set", &json!({ "frame": 110 })).unwrap();
    s.run("key.set", &json!({ "interp": "smooth" })).unwrap();
}

fn frames() -> Vec<f64> {
    (0..=110).map(f64::from).collect()
}

#[test]
fn one_undo_step_gives_back_the_clip() {
    let (mut s, lib) = session("undo");
    walk(&mut s);
    let before = s.document.clip.clone();
    s.run("layer.secondary", &json!({})).unwrap();
    assert_ne!(s.document.clip, before);
    s.run("edit.undo", &json!({})).unwrap();
    assert_eq!(s.document.clip, before);
    std::fs::remove_dir_all(&lib).ok();
}

#[test]
fn muting_gives_back_the_clip_exactly_and_no_key_changes() {
    let (mut s, lib) = session("mute");
    walk(&mut s);
    let base = s.document.clip.base().keys.clone();
    let length = s.document.clip.length();
    let before = s.document.clip.sample_with(&frames(), &s.document.links).unwrap();
    let r = s.run("layer.secondary", &json!({})).unwrap();
    let i = r["i"].as_u64().unwrap();
    assert!(r["bones"].as_u64().unwrap() > 0, "it should move something: {r}");
    assert_eq!(s.document.clip.edit, 0, "the key tools stay on the base");
    assert_eq!(s.document.clip.base().keys, base, "a key of the person's changed");
    assert_eq!(s.document.clip.length(), length, "the layer changed the length");
    let on = s.document.clip.sample_with(&frames(), &s.document.links).unwrap();
    assert_ne!(on.0, before.0, "the layer should show");
    assert_eq!(on.1, before.1, "the root's path must stay");
    s.run("layer.set", &json!({ "i": i, "muted": true })).unwrap();
    let muted = s.document.clip.sample_with(&frames(), &s.document.links).unwrap();
    assert_eq!(muted, before, "muted is not the clip");
    std::fs::remove_dir_all(&lib).ok();
}

#[test]
fn the_same_clip_gives_the_same_layer() {
    let (mut a, lib_a) = session("same-a");
    let (mut b, lib_b) = session("same-b");
    walk(&mut a);
    walk(&mut b);
    a.run("layer.secondary", &json!({})).unwrap();
    b.run("layer.secondary", &json!({})).unwrap();
    assert_eq!(a.document.clip, b.document.clip);
    std::fs::remove_dir_all(&lib_a).ok();
    std::fs::remove_dir_all(&lib_b).ok();
}

/// The opposite arm goes forward with a foot, and the ankles stay: a hip moves at most its half
/// width (0.1766 m, the ankles' x in `anim-steps`) x sin 6 degrees = 1.85 cm, so 5 cm is a safe bound
/// on what IK can't win back.
#[test]
fn the_opposite_arm_swings_and_the_feet_stay() {
    let (mut s, lib) = session("swing");
    walk(&mut s);
    let sk = s.document.skeleton.clone();
    let world = |s: &Session| {
        let (r, o) = s.document.clip.sample_with(&frames(), &s.document.links).unwrap();
        r.iter()
            .zip(&o)
            .map(|(r, o)| sk.fk(r, *o).unwrap().1)
            .collect::<Vec<_>>()
    };
    let before = world(&s);
    let r = s.run("layer.secondary", &json!({})).unwrap();
    let after = world(&s);
    let bone = |n: &str| sk.bone(n).unwrap();
    let (fl, fr, wl, wr) = (bone("foot.L"), bone("foot.R"), bone("wrist.L"), bone("wrist.R"));
    // the walk goes along -Y, so forward is less y; the middle of the walk, away from its ends
    let f = (20..60)
        .max_by(|&a, &b| {
            let ahead = |f: usize| before[f][fr][1] - before[f][fl][1];
            ahead(a).total_cmp(&ahead(b))
        })
        .unwrap();
    assert!(
        after[f][wr][1] < before[f][wr][1],
        "right hand forward on frame {f}"
    );
    assert!(after[f][wl][1] > before[f][wl][1], "left hand back on frame {f}");
    let mut drift: f64 = 0.0;
    for (a, b) in after.iter().zip(&before) {
        for foot in [fl, fr] {
            drift = drift.max(quat::len3(quat::sub3(a[foot], b[foot])));
        }
    }
    assert!((drift - r["foot_drift"].as_f64().unwrap()).abs() < 1e-6);
    assert!(drift < 0.05, "an ankle moved {drift} m");
    std::fs::remove_dir_all(&lib).ok();
}

/// After the walk stops, follow-through moves the upper body, and it dies away by the end.
#[test]
fn the_body_follows_through_a_stop() {
    let (mut s, lib) = session("follow");
    walk(&mut s);
    let r = s.run("layer.secondary", &json!({ "parts": ["follow"] })).unwrap();
    let lay = s.document.clip.layers[r["i"].as_u64().unwrap() as usize].clone();
    let spine = s.document.skeleton.bone("spine01").unwrap();
    let turn = |f: usize| quat::len3(quat::log(lay.keys[f].rotations[spine]));
    let after_stop = (80..95).map(turn).fold(0.0, f64::max);
    assert!(after_stop > 0.0, "no lean after the stop");
    assert!(after_stop > turn(110), "the lean should die away");
    assert!(lay.only.as_ref().unwrap().iter().all(|&b| {
        let n = &s.document.skeleton.names[b];
        n.starts_with("spine") || n.starts_with("upperarm") || n.starts_with("lowerarm")
    }));
    std::fs::remove_dir_all(&lib).ok();
}

#[test]
fn hands_alone_turn_only_the_wrists() {
    let (mut s, lib) = session("hands");
    walk(&mut s);
    let r = s.run("layer.secondary", &json!({ "parts": ["hands"] })).unwrap();
    let lay = &s.document.clip.layers[r["i"].as_u64().unwrap() as usize];
    let names: Vec<_> = lay
        .only
        .as_ref()
        .unwrap()
        .iter()
        .map(|&b| s.document.skeleton.names[b].clone())
        .collect();
    assert_eq!(names, ["wrist.L", "wrist.R"]);
    std::fs::remove_dir_all(&lib).ok();
}

#[test]
fn a_single_key_is_refused_and_changes_nothing() {
    let (mut s, lib) = session("one");
    s.run("key.set", &json!({})).unwrap();
    let before = s.document.clip.clone();
    assert!(s.run("layer.secondary", &json!({})).is_err());
    assert_eq!(s.document.clip, before);
    std::fs::remove_dir_all(&lib).ok();
}

/// A loop keeps its length and keys.
#[test]
fn a_loop_keeps_its_length() {
    let (mut s, lib) = session("loop");
    s.run("clip.open", &json!({ "id": "Locomotion/Walk" })).unwrap();
    let length = s.document.clip.length();
    let before = s.document.clip.base().keys.clone();
    s.run("layer.secondary", &json!({})).unwrap();
    assert_eq!(s.document.clip.length(), length);
    assert_eq!(s.document.clip.base().keys, before);
    std::fs::remove_dir_all(&lib).ok();
}
