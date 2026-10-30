//! The physics pass against hand-worked goals: hull geometry, balance (centre of mass inside the
//! support, feet held), fulcrum only for slow feet, determinism and refused input.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use ac_core::document::standing;
use ac_core::fullbody::Limits;
use ac_core::physics::{self, BALANCE_MARGIN, Body};
use ac_core::quat::{Quat, Vec3};

fn limits(sk: &ac_core::Skeleton) -> Limits {
    Limits::from_json(
        &std::fs::read_to_string(common::repo().join("assets/rig/limits.json")).unwrap(),
        sk,
    )
    .unwrap()
}

fn about_x(t: f64) -> Quat {
    [(t / 2.0).sin(), 0.0, 0.0, (t / 2.0).cos()]
}

#[test]
fn hull_of_a_square() {
    // the unit square with a point inside and one on an edge: the hull is the four corners, CCW
    let h = physics::hull(&[
        [0.0, 0.0],
        [1.0, 0.0],
        [0.5, 0.5],
        [1.0, 1.0],
        [0.0, 1.0],
        [0.5, 0.0],
    ]);
    assert_eq!(h, vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
    assert!(physics::contains(&h, [0.5, 0.5], 0.0));
    assert!(physics::contains(&h, [1.0, 0.5], 1e-12));
    assert!(!physics::contains(&h, [1.01, 0.5], 1e-3));
    // on a line: the two ends; a point holds only itself
    assert_eq!(
        physics::hull(&[[0.0, 0.0], [2.0, 0.0], [1.0, 0.0]]),
        vec![[0.0, 0.0], [2.0, 0.0]]
    );
    assert!(physics::contains(&[[1.0, 1.0]], [1.0, 1.0], 1e-9));
    assert!(!physics::contains(&[[1.0, 1.0]], [1.0, 1.1], 1e-3));
}

/// Distance from `q` inside a CCW hull to its nearest edge line.
fn inset(h: &[[f64; 2]], q: [f64; 2]) -> f64 {
    (0..h.len())
        .map(|i| {
            let (a, b) = (h[i], h[(i + 1) % h.len()]);
            ((b[0] - a[0]) * (q[1] - a[1]) - (b[1] - a[1]) * (q[0] - a[0])) / (b[0] - a[0]).hypot(b[1] - a[1])
        })
        .fold(f64::INFINITY, f64::min)
}

/// A slow lean past the toes: balance starts part way, and the hips ease into it rather than
/// jump. The keyed hips don't move at all.
#[test]
fn balance_fades_in_rather_than_jumping() {
    let sk = common::skeleton();
    let lim = limits(&sk);
    let rest = standing(&sk);
    let frames = 90;
    let rots: Vec<Vec<Quat>> = (0..frames)
        .map(|f| {
            let u = (f as f64 / 60.0).min(1.0);
            let mut r = rest.rotations.clone();
            for (b, t) in [
                ("spine05", 0.4),
                ("spine04", 0.34),
                ("spine03", 0.3),
                ("spine02", 0.3),
                ("upperarm01.L", -1.4),
                ("upperarm01.R", -1.4),
            ] {
                r[sk.bone(b).unwrap()] = about_x(t * u);
            }
            r
        })
        .collect();
    let roots = vec![rest.root; frames];
    let r = physics::solve(&sk, &rots, &roots, 30.0, &[], Some(&lim)).unwrap();
    assert!(r.balanced.len() > 5, "balanced {:?}", r.balanced);
    let first = r.balanced[0];
    assert!(first > 0, "the lean starts in balance");
    let h = sk.hip_height;
    let at = |f: usize| sk.fk(&r.rotations[f], r.roots[f]).unwrap().1;
    let foot = sk.bone("foot.L").unwrap();
    let rest_foot = sk.fk(&rest.rotations, rest.root).unwrap().1[foot];
    for f in 0..frames {
        let p = at(f)[foot];
        let d = (0..3).map(|k| (p[k] - rest_foot[k]).powi(2)).sum::<f64>().sqrt();
        assert!(d < 1e-3, "frame {f}: the foot moved {d} m");
        if f > 0 {
            let (a, b) = (r.roots[f - 1], r.roots[f]);
            let step = (a[0] - b[0]).hypot(a[1] - b[1]) * h;
            assert!(step < 0.01, "frame {f}: the hips jumped {step} m");
        }
    }
}

/// A body lying on its back, its feet lifted well off the floor: its feet don't hold it up, so it is
/// on the ground, not in flight, and the pass leaves it as keyed.
#[test]
fn a_lying_body_is_left_alone() {
    let sk = common::skeleton();
    let lim = limits(&sk);
    let rest = standing(&sk);
    let mut rots = rest.rotations.clone();
    rots[0] = about_x(std::f64::consts::FRAC_PI_2);
    // the hips 10 cm up, as root offsets in hip heights from the rest root
    let root = [0.0, 0.0, 0.1 / sk.hip_height - sk.heads[0][2] / sk.hip_height];
    let frames = 30;
    let r = physics::solve(
        &sk,
        &vec![rots.clone(); frames],
        &vec![root; frames],
        30.0,
        &[],
        Some(&lim),
    )
    .unwrap();
    assert!(r.flights.is_empty(), "flights {:?}", r.flights);
    assert!(r.balanced.is_empty(), "balanced {:?}", r.balanced);
    assert!(r.slid_frames.is_empty(), "slid {:?}", r.slid_frames);
    assert!(r.rotations.iter().all(|x| *x == rots));
    // lifted out of the floor if a heel is below it, but not moved across it
    assert!(r.roots.iter().all(|o| o[0] == root[0] && o[1] == root[1]));
}

/// A walk on the spot has its centre of mass nearly still, but its feet step: it doesn't stand.
#[test]
fn a_body_that_steps_does_not_stand() {
    let still = vec![vec![[0.1, 0.0, 0.0]]; 8];
    let coms = vec![[0.0; 3]; 8];
    assert_eq!(physics::standing(&still, &coms, 30.0), vec![true; 8]);
    // the foot moves 2 cm a frame (0.6 m/s) on frames 3 to 5: the pause either side is too short
    let y = [0.0, 0.0, 0.0, 0.02, 0.04, 0.06, 0.06, 0.06];
    let stepping: Vec<Vec<Vec3>> = y.iter().map(|y| vec![[0.1, *y, 0.0]]).collect();
    assert_eq!(physics::standing(&stepping, &coms, 30.0), vec![false; 8]);
    // a clip shorter than MIN_STAND can still stand
    let n = physics::MIN_STAND - 1;
    assert_eq!(
        physics::standing(&vec![vec![[0.1, 0.0, 0.0]]; n], &vec![[0.0; 3]; n], 30.0),
        vec![true; n]
    );
}

#[test]
fn a_still_lean_is_balanced_with_feet_held() {
    let sk = common::skeleton();
    let lim = limits(&sk);
    let body = Body::new(&sk).unwrap();
    let mut pose = standing(&sk);
    for (b, t) in [
        ("spine05", 0.4),
        ("spine04", 0.34),
        ("spine03", 0.3),
        ("spine02", 0.3),
        ("upperarm01.L", -1.4),
        ("upperarm01.R", -1.4),
    ] {
        pose.rotations[sk.bone(b).unwrap()] = about_x(t);
    }
    let before = physics::balance(&sk, &body, &pose.rotations, pose.root, &[])
        .unwrap()
        .unwrap();
    assert!(!before.inside, "the lean should start out of balance");
    let r = physics::solve(
        &sk,
        &[pose.rotations.clone()],
        &[pose.root],
        30.0,
        &[],
        Some(&lim),
    )
    .unwrap();
    assert_eq!(r.balanced, vec![0]);
    let after = physics::balance(&sk, &body, &r.rotations[0], r.roots[0], &[])
        .unwrap()
        .unwrap();
    // the aim is BALANCE_MARGIN inside; the re-aiming closes all but a sliver of it
    assert!(
        inset(&after.support, after.com) > BALANCE_MARGIN * 0.5,
        "inset {}",
        inset(&after.support, after.com)
    );
    let p0 = sk.fk(&pose.rotations, pose.root).unwrap().1;
    let p1 = sk.fk(&r.rotations[0], r.roots[0]).unwrap().1;
    for f in ["foot.L", "foot.R"] {
        let i = sk.bone(f).unwrap();
        let d =
            ((p0[i][0] - p1[i][0]).powi(2) + (p0[i][1] - p1[i][1]).powi(2) + (p0[i][2] - p1[i][2]).powi(2))
                .sqrt();
        assert!(d < 1e-4, "{f} moved {d}");
    }
    // the bent spine is kept (the hips do the work): within a degree of the key
    let s = sk.bone("spine02").unwrap();
    let dot: f64 = (0..4).map(|k| r.rotations[0][s][k] * about_x(0.3)[k]).sum();
    assert!(dot.abs() > (0.5f64.to_radians()).cos(), "spine02 changed");
    // and the same answer twice
    assert_eq!(
        r,
        physics::solve(
            &sk,
            &[pose.rotations.clone()],
            &[pose.root],
            30.0,
            &[],
            Some(&lim)
        )
        .unwrap()
    );
}

#[test]
fn fast_feet_skimming_the_floor_are_not_planted() {
    // standing, moving 0.1 m a frame along X (3 m/s at 30 fps): the feet are near the floor but
    // far too fast to be planted, so the fulcrum leaves every root where it was keyed
    let sk = common::skeleton();
    let pose = standing(&sk);
    let h = sk.hip_height;
    let roots: Vec<Vec3> = (0..10)
        .map(|f| [pose.root[0] + 0.1 * f as f64 / h, pose.root[1], pose.root[2]])
        .collect();
    let rots = vec![pose.rotations.clone(); 10];
    let r = physics::solve(&sk, &rots, &roots, 30.0, &[], None).unwrap();
    assert_eq!(r.slid, 0);
    assert_eq!(r.roots, roots);
    assert!(r.flights.is_empty() && r.lifted == 0);
}

#[test]
fn feet_below_a_box_top_are_lifted_onto_it() {
    // a 30 cm box under the standing figure: the lowest foot point, 2 cm up, ends on its top:
    // the body goes up 28 cm
    let sk = common::skeleton();
    let pose = standing(&sk);
    let box_ = ac_core::clip::Surface {
        name: "Box".into(),
        min: [-1.0, -1.0],
        max: [1.0, 1.0],
        top: 0.3,
        yaw: 0.0,
    };
    let r = physics::solve(
        &sk,
        std::slice::from_ref(&pose.rotations),
        &[pose.root],
        30.0,
        std::slice::from_ref(&box_),
        None,
    )
    .unwrap();
    assert_eq!(r.lifted, 1);
    let dz = (r.roots[0][2] - pose.root[2]) * sk.hip_height;
    assert!((dz - 0.28).abs() < 1e-9, "lifted {dz}, not 0.28");
}

#[test]
fn refuses_bad_input() {
    let sk = common::skeleton();
    let pose = standing(&sk);
    let rots = vec![pose.rotations.clone()];
    assert!(physics::solve(&sk, &rots, &[], 30.0, &[], None).is_err());
    assert!(physics::solve(&sk, &rots, &[pose.root], 0.0, &[], None).is_err());
    assert!(physics::solve(&sk, &rots, &[pose.root], f64::NAN, &[], None).is_err());
}

#[test]
fn a_turned_box_holds_what_is_under_it_turned() {
    // 2 m along X and 0.2 m along Y around (1, 0); turned a quarter it runs along Y instead
    let mut b = ac_core::clip::Surface {
        name: "Ledge".into(),
        min: [0.0, -0.1],
        max: [2.0, 0.1],
        top: 0.5,
        yaw: 0.0,
    };
    assert_eq!(physics::ground(std::slice::from_ref(&b), 1.9, 0.0), 0.5);
    assert_eq!(physics::ground(std::slice::from_ref(&b), 1.0, 0.9), 0.0);
    b.yaw = std::f64::consts::FRAC_PI_2;
    assert_eq!(physics::ground(std::slice::from_ref(&b), 1.9, 0.0), 0.0);
    assert_eq!(physics::ground(std::slice::from_ref(&b), 1.0, 0.9), 0.5);
    assert_eq!(physics::ground(std::slice::from_ref(&b), 1.05, -0.95), 0.5);
    // corners, turned counter-clockwise: (-1, -0.1) from the centre goes to (0.1, -1)
    let c = b.corners();
    assert!(
        (c[0][0] - 1.1).abs() < 1e-12 && (c[0][1] + 1.0).abs() < 1e-12,
        "{c:?}"
    );
}

#[test]
fn a_box_turn_survives_saving() {
    let mut clip = ac_core::clip::Clip::new(vec!["root".into()], 30.0);
    clip.surfaces.push(ac_core::clip::Surface {
        name: "Step".into(),
        min: [0.0, 0.0],
        max: [1.0, 0.5],
        top: 0.2,
        yaw: 0.7,
    });
    let text = ac_core::clipfile::save_clip(&clip);
    let back = ac_core::clipfile::load_clip(&text).unwrap();
    assert_eq!(back.surfaces, clip.surfaces);
    // a file from before boxes turned reads as unturned
    clip.surfaces[0].yaw = 0.0;
    let old = ac_core::clipfile::save_clip(&clip);
    assert!(!old.contains("yaw"), "{old}");
    assert_eq!(ac_core::clipfile::load_clip(&old).unwrap().surfaces[0].yaw, 0.0);
}
