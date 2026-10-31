//! The physics pass with held contacts and tall boxes (roadmap 161000).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use ac_core::Skeleton;
use ac_core::clip::{Contact, Surface};
use ac_core::document::standing;
use ac_core::physics::{self, Body, G};
use ac_core::quat::{Quat, Vec3};

/// 40 frames of the rest pose: on the floor to frame 4, 0.5 m up from 5 to 32, on the floor again.
fn hop(sk: &Skeleton) -> (Vec<Vec<Quat>>, Vec<Vec3>) {
    let rest = standing(sk);
    let roots = (0..40)
        .map(|f| {
            let mut r = rest.root;
            if (5..33).contains(&f) {
                r[2] += 0.5 / sk.hip_height;
            }
            r
        })
        .collect();
    (vec![rest.rotations.clone(); 40], roots)
}

fn at_frame(sk: &Skeleton, rots: &[Quat], root: Vec3, bone: &str) -> Vec3 {
    sk.fk(rots, root).unwrap().1[sk.bone(bone).unwrap()]
}

fn held(sk: &Skeleton, bone: &str, from: i64, to: i64) -> Contact {
    let (rots, roots) = hop(sk);
    Contact {
        bone: bone.into(),
        from,
        to,
        at: at_frame(sk, &rots[from as usize], roots[from as usize], bone),
    }
}

fn distance(a: Vec3, b: Vec3) -> f64 {
    (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f64>().sqrt()
}

#[test]
fn without_a_contact_the_hop_is_one_parabola() {
    let sk = common::skeleton();
    let (rots, roots) = hop(&sk);
    let r = physics::solve_held(&sk, &rots, &roots, 30.0, &[], &[], None).unwrap();
    assert_eq!(r.flights, [(4, 33)]);
    // half way, the hips are above the line between take-off and landing by g T^2 / 8
    let total = 29.0 / 30.0;
    let (t, line) = (14.0 / 30.0, roots[4][2]);
    let up = (r.roots[18][2] - line) * sk.hip_height;
    let want = 0.5 * G * t * (total - t);
    assert!((up - want).abs() < 1e-9, "{up} against {want}");
}

#[test]
fn a_held_foot_splits_the_flight_and_does_not_move() {
    let sk = common::skeleton();
    let (rots, roots) = hop(&sk);
    let c = held(&sk, "foot.L", 15, 20);
    let r = physics::solve_held(&sk, &rots, &roots, 30.0, &[], std::slice::from_ref(&c), None).unwrap();
    // the last frame of the contact is the take-off
    assert_eq!(r.flights, [(4, 15), (20, 33)]);
    for f in 15..=20 {
        let d = distance(at_frame(&sk, &r.rotations[f], r.roots[f], "foot.L"), c.at);
        assert!(d < 1e-6, "frame {f}: the foot moved {d} m");
    }
    // the pose in the air is kept outside the contact
    assert_eq!(r.rotations[10], rots[10]);
    assert_eq!(r.rotations[25], rots[25]);
}

#[test]
fn a_held_hand_does_too() {
    let sk = common::skeleton();
    let (rots, roots) = hop(&sk);
    let c = held(&sk, "wrist.R", 15, 20);
    let r = physics::solve_held(&sk, &rots, &roots, 30.0, &[], std::slice::from_ref(&c), None).unwrap();
    assert_eq!(r.flights, [(4, 15), (20, 33)]);
    for f in 15..=20 {
        let d = distance(at_frame(&sk, &r.rotations[f], r.roots[f], "wrist.R"), c.at);
        assert!(d < 1e-6, "frame {f}: the hand moved {d} m");
    }
}

/// Held out of the limb's reach, as a key moved later can leave it: the body goes to the point.
#[test]
fn a_point_out_of_reach_still_holds_and_two_can_be_held() {
    let sk = common::skeleton();
    let (rots, roots) = hop(&sk);
    let mut far = held(&sk, "foot.L", 15, 20);
    far.at[0] += 3.0;
    let r = physics::solve_held(&sk, &rots, &roots, 30.0, &[], std::slice::from_ref(&far), None).unwrap();
    let d = distance(at_frame(&sk, &r.rotations[17], r.roots[17], "foot.L"), far.at);
    assert!(d < 1e-6, "{d}");

    let both = [held(&sk, "foot.L", 15, 20), held(&sk, "wrist.R", 15, 20)];
    let r = physics::solve_held(&sk, &rots, &roots, 30.0, &[], &both, None).unwrap();
    for c in &both {
        let d = distance(at_frame(&sk, &r.rotations[17], r.roots[17], &c.bone), c.at);
        assert!(d < 1e-6, "{}: {d}", c.bone);
    }
}

#[test]
fn a_contact_past_the_end_of_the_clip_is_harmless_and_a_bad_bone_refused() {
    let sk = common::skeleton();
    let (rots, roots) = hop(&sk);
    let off_the_end = held(&sk, "foot.L", 35, 39);
    let mut late = off_the_end.clone();
    (late.from, late.to) = (500, 600);
    let r = physics::solve_held(&sk, &rots, &roots, 30.0, &[], &[late], None).unwrap();
    assert_eq!(r.flights, [(4, 33)]);
    let mut bad = off_the_end;
    bad.bone = "head".into();
    assert!(physics::solve_held(&sk, &rots, &roots, 30.0, &[], &[bad], None).is_err());
}

fn tall(top: f64, min_x: f64) -> Surface {
    Surface {
        name: "Wall".into(),
        min: [min_x, -2.0],
        max: [min_x + 0.5, 2.0],
        top,
        yaw: 0.0,
    }
}

/// The rest pose walking along x at floor level, through a box.
fn across(sk: &Skeleton, frames: usize) -> (Vec<Vec<Quat>>, Vec<Vec3>) {
    let rest = standing(sk);
    let roots = (0..frames)
        .map(|f| {
            let mut r = rest.root;
            r[0] += f as f64 * 0.03 / sk.hip_height;
            r
        })
        .collect();
    (vec![rest.rotations.clone(); frames], roots)
}

#[test]
fn a_toe_crossing_a_tall_box_side_causes_no_lift() {
    let sk = common::skeleton();
    let (rots, roots) = across(&sk, 40);
    let wall = tall(2.0, 0.6);
    let r = physics::solve(&sk, &rots, &roots, 30.0, std::slice::from_ref(&wall), None).unwrap();
    assert_eq!(r.lifted, 0, "{:?}", r.lifted_frames);
    for (a, b) in r.roots.iter().zip(&roots) {
        assert!((a[2] - b[2]).abs() < 1e-9);
    }
}

#[test]
fn coming_down_on_a_box_top_lifts_as_before() {
    let sk = common::skeleton();
    let rest = standing(&sk);
    // 0.5 m up while it moves over the box, then keyed back down to floor level inside it
    let roots: Vec<Vec3> = (0..30)
        .map(|f| {
            let mut r = rest.root;
            r[0] += (f.min(10) as f64) * 0.1 / sk.hip_height;
            if f < 15 {
                r[2] += 0.5 / sk.hip_height;
            }
            r
        })
        .collect();
    let rots = vec![rest.rotations.clone(); 30];
    let step = tall(0.3, 0.8);
    let r = physics::solve(&sk, &rots, &roots, 30.0, std::slice::from_ref(&step), None).unwrap();
    for f in 15..30 {
        assert!(r.lifted_frames.contains(&f), "frame {f}: {:?}", r.lifted_frames);
    }
    let dz = (r.roots[25][2] - roots[25][2]) * sk.hip_height;
    assert!((dz - 0.28).abs() < 1e-3, "lifted {dz}, not 0.28");
}

#[test]
fn a_joint_deep_inside_a_box_is_not_resting_on_it() {
    let sk = common::skeleton();
    let rest = standing(&sk);
    let body = Body::new(&sk).unwrap();
    let pos = sk.fk(&rest.rotations, rest.root).unwrap().1;
    let wrist = pos[sk.bone("wrist.L").unwrap()][2];
    let around = |top| Surface {
        name: "Box".into(),
        min: [-2.0, -2.0],
        max: [2.0, 2.0],
        top,
        yaw: 0.0,
    };
    assert!(!body.rests(&pos, &[around(3.0)]));
    assert!(body.rests(&pos, &[around(wrist + 0.05)]));
}
