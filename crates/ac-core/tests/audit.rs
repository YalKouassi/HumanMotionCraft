//! Editing behaviour that must stay dependable, swept in small steps (roadmap 116800): a limb's IK
//! never flips or snaps as its end moves, and moving the body leaves pinned limbs where they are.
//! `crates/ac-cli/tests/part_layer.rs` checks the part layer, whose commands are in `ac-io`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use ac_core::Session;
use ac_core::document::standing;
use ac_core::ik;
use ac_core::quat::Vec3;
use serde_json::json;

fn dist(a: Vec3, b: Vec3) -> f64 {
    (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f64>().sqrt()
}

fn world(s: &mut Session, bone: &str) -> Vec3 {
    common::arr(&s.run("bone.world", &json!({ "bone": bone })).unwrap()["position"])
}

/// A leg's foot moves round a circle in front of the hips, 1 cm a step: the knee moves smoothly.
#[test]
fn leg_ik_does_not_flip_as_the_foot_moves() {
    let sk = common::skeleton();
    let rest = standing(&sk);
    let foot = sk.bone("foot.L").unwrap();
    let knee = sk.bone("lowerleg01.L").unwrap();
    let start = sk.fk(&rest.rotations, rest.root).unwrap().1[foot];
    let mut rots = rest.rotations.clone();
    let mut last = sk.fk(&rots, rest.root).unwrap().1[knee];
    let radius = 0.25;
    let steps = (2.0 * std::f64::consts::PI * radius / 0.01) as usize;
    for i in 0..=steps {
        let a = i as f64 / steps as f64 * 2.0 * std::f64::consts::PI;
        // from the rest place, round a circle in the leg's forward (-Y) and up (+Z) plane
        let target = [
            start[0],
            start[1] - radius * a.sin(),
            start[2] + radius * (1.0 - a.cos()),
        ];
        let pole = ik::own_pole(&sk, &rots, rest.root, "leg.L").unwrap();
        rots = ik::two_bone(&sk, &rots, rest.root, "leg.L", target, pole, None).unwrap();
        let p = sk.fk(&rots, rest.root).unwrap().1;
        assert!(dist(p[foot], target) < 1e-3, "step {i}: the foot is off target");
        let moved = dist(p[knee], last);
        assert!(
            moved < 0.05,
            "step {i}: the knee jumped {moved} m for a 1 cm move"
        );
        last = p[knee];
    }
}

/// A hand drags forward past its reach and back, 2 cm a step: the elbow doesn't snap, even as the
/// arm goes straight.
#[test]
fn dragging_a_hand_through_full_reach_does_not_snap_the_elbow() {
    let mut s = common::session();
    let mut last = world(&mut s, "lowerarm01.L");
    for i in 0..80 {
        let by = if i < 40 { -0.02 } else { 0.02 };
        s.run("pose.drag", &json!({ "bone": "wrist.L", "by": [0.0, by, 0.0] }))
            .unwrap();
        let e = world(&mut s, "lowerarm01.L");
        let moved = dist(e, last);
        assert!(
            moved < 0.08,
            "step {i}: the elbow jumped {moved} m for a 2 cm move"
        );
        last = e;
    }
}

/// With both feet pinned the body moves across and down: the feet stay within 1 mm, and the knees
/// move smoothly.
#[test]
fn moving_the_body_leaves_pinned_feet_where_they_are() {
    let mut s = common::session();
    let (l, r) = (world(&mut s, "foot.L"), world(&mut s, "foot.R"));
    s.run("pin.set", &json!({ "bone": "foot.L", "on": true }))
        .unwrap();
    s.run("pin.set", &json!({ "bone": "foot.R", "on": true }))
        .unwrap();
    let mut last = world(&mut s, "lowerleg01.L");
    for i in 0..30 {
        s.run(
            "pose.drag",
            &json!({ "bone": "root", "by": [0.003, -0.003, -0.005] }),
        )
        .unwrap();
        assert!(
            dist(world(&mut s, "foot.L"), l) < 1e-3,
            "step {i}: the left foot moved"
        );
        assert!(
            dist(world(&mut s, "foot.R"), r) < 1e-3,
            "step {i}: the right foot moved"
        );
        let k = world(&mut s, "lowerleg01.L");
        let moved = dist(k, last);
        assert!(moved < 0.04, "step {i}: the knee jumped {moved} m");
        last = k;
    }
}
