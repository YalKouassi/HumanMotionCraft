//! A step inside a filled span: `plant_steps` holds the foot the model says is down where it is,
//! so it doesn't slide (roadmap 135000).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use ac_core::clip::{Interp, Key, KeySource};
use ac_core::quat;
use serde_json::json;

/// Your keys on 0 and 20, knees bent (so a leg can reach a foot left behind), the body carried
/// 1 m along X between them (linear): both feet glide 5 cm a frame.
fn gliding() -> ac_core::Session {
    let mut s = common::session();
    s.run("key.set", &json!({})).unwrap();
    let sk = s.document.skeleton.clone();
    let mut rest = s.document.clip.keys()[0].clone();
    for side in ["L", "R"] {
        rest.rotations[sk.bone(&format!("upperleg01.{side}")).unwrap()] = quat::from_euler([-35.0, 0.0, 0.0]);
        rest.rotations[sk.bone(&format!("lowerleg01.{side}")).unwrap()] = quat::from_euler([70.0, 0.0, 0.0]);
    }
    // a fill keys every frame of its span: AI keys between yours
    let hip = s.document.skeleton.hip_height;
    for f in 0..=20 {
        let dx = f as f64 / 20.0 / hip;
        let mut k = Key::new(
            f,
            rest.rotations.clone(),
            [rest.root[0] + dx, rest.root[1], rest.root[2]],
            Interp::Linear,
        );
        k.source = if f % 20 == 0 {
            KeySource::Manual
        } else {
            KeySource::Ai
        };
        s.document.clip.set_key(k).unwrap();
    }
    s
}

#[test]
fn a_foot_the_model_says_is_down_stays_put() {
    let mut s = gliding();
    let sk = s.document.skeleton.clone();
    let (left, right) = (sk.bone("foot.L").unwrap(), sk.bone("foot.R").unwrap());
    // the left foot down on frames 5..=12 (the span's frames 1..=19 are contacts 0..=18)
    let contacts: Vec<[bool; 2]> = (1..20).map(|f| [(5..=12).contains(&f), false]).collect();
    let before: Vec<_> = (0..=20)
        .map(|f| s.document.positions(&[f as f64]).unwrap()[0].clone())
        .collect();
    let changed = s.document.plant_steps(0, 20, &contacts, 0.15).unwrap();
    assert_eq!(changed, 8);
    let at = |f: i64| s.document.positions(&[f as f64]).unwrap()[0].clone();
    // held where it is on the run's middle frame (8)
    let held = before[8][left];
    for f in 5..=12 {
        let moved = quat::len3(quat::sub3(at(f)[left], held));
        assert!(moved < 0.01, "frame {f}: the planted foot is {moved} m off");
    }
    // the other foot and the frames outside the run are as they were
    for f in (0..=20).filter(|f| !(5..=12).contains(f)) {
        assert!(
            quat::len3(quat::sub3(at(f)[left], before[f as usize][left])) < 1e-9,
            "frame {f}"
        );
    }
    for f in 0..=20 {
        assert!(
            quat::len3(quat::sub3(at(f)[right], before[f as usize][right])) < 1e-9,
            "frame {f}"
        );
    }
}

#[test]
fn a_single_frame_or_a_foot_in_the_air_is_not_planted() {
    let mut s = gliding();
    let one: Vec<[bool; 2]> = (1..20).map(|f| [f == 7, f == 9]).collect();
    assert_eq!(s.document.plant_steps(0, 20, &one, 0.15).unwrap(), 0);
    // a hop: the body 0.5 m up on frames 6..=14, where the model (wrongly) says the feet are down
    let hip = s.document.skeleton.hip_height;
    let rest = s.document.clip.keys()[0].clone();
    for f in 6..=14 {
        let t = f as f64 / 20.0;
        let mut k = Key::new(
            f,
            rest.rotations.clone(),
            [rest.root[0] + t / hip, rest.root[1], rest.root[2] + 0.5 / hip],
            Interp::Linear,
        );
        k.source = KeySource::Ai;
        s.document.clip.set_key(k).unwrap();
    }
    let air: Vec<[bool; 2]> = (1..20)
        .map(|f| [(8..=12).contains(&f), (8..=12).contains(&f)])
        .collect();
    assert_eq!(
        s.document.plant_steps(0, 20, &air, 0.15).unwrap(),
        0,
        "feet in the air stay in the air"
    );
    let wrong = vec![[true, true]; 3];
    assert!(
        s.document.plant_steps(0, 20, &wrong, 0.15).is_err(),
        "contacts for the wrong number of frames"
    );
}
