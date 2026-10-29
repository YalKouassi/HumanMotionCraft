//! Full-body IK meets its goals, holds its pins, stays inside the joint limits, leaves the frozen
//! bones alone and gives the same answer twice. Every check is a property, not a stored output.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use ac_core::Skeleton;
use ac_core::document::standing;
use ac_core::fullbody::{Goal, Limits, TOLERANCE, solve};
use ac_core::quat::{self, Vec3};

fn setup() -> (Skeleton, Limits) {
    let sk = common::skeleton();
    let text = std::fs::read_to_string(common::repo().join("assets/rig/limits.json")).unwrap();
    let limits = Limits::from_json(&text, &sk).unwrap();
    (sk, limits)
}

fn joints(sk: &Skeleton, rots: &[[f64; 4]], root: Vec3) -> Vec<Vec3> {
    sk.fk(rots, root).unwrap().1
}

fn dist(a: Vec3, b: Vec3) -> f64 {
    quat::len3(quat::sub3(a, b))
}

#[test]
fn reaches_a_reachable_target_with_feet_pinned() {
    let (sk, limits) = setup();
    let rest = standing(&sk);
    let p0 = joints(&sk, &rest.rotations, rest.root);
    let (w, fl, fr) = (
        sk.bone("wrist.L").unwrap(),
        sk.bone("foot.L").unwrap(),
        sk.bone("foot.R").unwrap(),
    );
    // the hand forward and up: well inside arm's reach of the shoulder
    let target = quat::add3(p0[w], [-0.1, -0.25, 0.3]);
    let goals = [
        Goal { bone: w, target },
        Goal {
            bone: fl,
            target: p0[fl],
        },
        Goal {
            bone: fr,
            target: p0[fr],
        },
    ];
    let s = solve(&sk, (&rest.rotations, rest.root), &goals, Some(&limits)).unwrap();
    let p = joints(&sk, &s.rotations, s.root);
    assert!(dist(p[w], target) < 1e-4, "wrist off by {}", dist(p[w], target));
    assert!(
        dist(p[fl], p0[fl]) < 1e-4 && dist(p[fr], p0[fr]) < 1e-4,
        "a pinned foot moved"
    );
    for (i, q) in s.rotations.iter().enumerate() {
        assert!(
            limits.contains(i, *q, 1e-9),
            "{} is outside its limits",
            sk.names[i]
        );
    }
}

#[test]
fn crouches_with_feet_pinned() {
    // the root goes 15 cm down and the pinned feet stay: the knees must bend
    let (sk, limits) = setup();
    let rest = standing(&sk);
    let p0 = joints(&sk, &rest.rotations, rest.root);
    let (fl, fr, knee) = (
        sk.bone("foot.L").unwrap(),
        sk.bone("foot.R").unwrap(),
        sk.bone("lowerleg01.L").unwrap(),
    );
    let target = quat::add3(p0[0], [0.0, 0.0, -0.15]);
    let goals = [
        Goal { bone: 0, target },
        Goal {
            bone: fl,
            target: p0[fl],
        },
        Goal {
            bone: fr,
            target: p0[fr],
        },
    ];
    let s = solve(&sk, (&rest.rotations, rest.root), &goals, Some(&limits)).unwrap();
    let p = joints(&sk, &s.rotations, s.root);
    assert!(s.error < 1e-4, "error {}", s.error);
    assert!(dist(p[0], target) < 1e-4);
    // a bent knee goes forward (-Y, the way the body faces), never backwards
    assert!(
        p[knee][1] < p0[knee][1],
        "knee went backwards: {} vs {}",
        p[knee][1],
        p0[knee][1]
    );
}

#[test]
fn an_unreachable_target_gets_as_close_as_it_can() {
    let (sk, limits) = setup();
    let rest = standing(&sk);
    let p0 = joints(&sk, &rest.rotations, rest.root);
    let (w, fl, fr) = (
        sk.bone("wrist.L").unwrap(),
        sk.bone("foot.L").unwrap(),
        sk.bone("foot.R").unwrap(),
    );
    let target = [3.0, 0.0, 1.0]; // 3 m to the side, feet pinned
    let goals = [
        Goal { bone: w, target },
        Goal {
            bone: fl,
            target: p0[fl],
        },
        Goal {
            bone: fr,
            target: p0[fr],
        },
    ];
    let s = solve(&sk, (&rest.rotations, rest.root), &goals, Some(&limits)).unwrap();
    let p = joints(&sk, &s.rotations, s.root);
    assert!(s.error > TOLERANCE);
    assert!(
        dist(p[w], target) < dist(p0[w], target) - 0.1,
        "did not get closer"
    );
    for (i, q) in s.rotations.iter().enumerate() {
        assert!(
            limits.contains(i, *q, 1e-9),
            "{} is outside its limits",
            sk.names[i]
        );
    }
}

#[test]
fn frozen_bones_stay_and_answers_repeat() {
    let (sk, limits) = setup();
    let rest = standing(&sk);
    let p0 = joints(&sk, &rest.rotations, rest.root);
    let w = sk.bone("wrist.R").unwrap();
    let goals = [Goal {
        bone: w,
        target: quat::add3(p0[w], [0.1, -0.2, 0.2]),
    }];
    let a = solve(&sk, (&rest.rotations, rest.root), &goals, Some(&limits)).unwrap();
    let b = solve(&sk, (&rest.rotations, rest.root), &goals, Some(&limits)).unwrap();
    assert_eq!(a, b);
    for (i, n) in sk.names.iter().enumerate() {
        if ["finger", "metacarpal", "toe", "eye"]
            .iter()
            .any(|p| n.starts_with(p))
        {
            assert_eq!(a.rotations[i], rest.rotations[i], "{n} moved");
        }
    }
}

#[test]
fn refuses_bad_input() {
    let (sk, _) = setup();
    let rest = standing(&sk);
    assert!(solve(&sk, (&rest.rotations, rest.root), &[], None).is_err());
    assert!(
        solve(
            &sk,
            (&rest.rotations, rest.root),
            &[Goal {
                bone: 999,
                target: [0.0; 3]
            }],
            None
        )
        .is_err()
    );
    assert!(
        solve(
            &sk,
            (&rest.rotations, rest.root),
            &[Goal {
                bone: 0,
                target: [f64::NAN, 0.0, 0.0]
            }],
            None
        )
        .is_err()
    );
}
