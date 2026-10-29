//! The range tables and the anatomical angles behind them (`ac_core::anatomy`, `ac_core::ranges`).

use ac_core::Skeleton;
use ac_core::anatomy::Joint;
use ac_core::quat::{self, IDENTITY};
use ac_core::ranges::{self, Ranges};

fn rig() -> Skeleton {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/rig/default_rig.json"
    ))
    .expect("the rig reads");
    Skeleton::from_json(&text).expect("the rig parses")
}

fn tables(sk: &Skeleton) -> Ranges {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/rig/ranges.json"
    ))
    .expect("ranges.json reads");
    Ranges::from_json(&text, sk).expect("ranges.json parses")
}

#[test]
fn python_and_rust_measure_the_same_angles() {
    let sk = rig();
    let r = tables(&sk);
    assert!(r.checks.len() >= 2, "ranges.json carries check poses");
    let (worst, at) = ranges::check_drift(&r, &sk).expect("the checks measure");
    assert!(worst < 1e-6, "builder and app disagree by {worst} degrees ({at})");
}

#[test]
fn every_pair_id_is_in_the_tables() {
    let sk = rig();
    let r = tables(&sk);
    for id in ranges::PAIR_IDS {
        r.pair(id).expect("pair present");
    }
}

/// The rest pose's shoulder, worked from the rig's heads: the upper arm `d` (upperarm01.L ->
/// lowerarm01.L) turned from straight down by `acos(-d.z)` about `down x d`; abduction is that
/// angle along the outward axis, flexion along the forward one.
#[test]
fn rest_shoulder_reads_the_a_pose() {
    let sk = rig();
    let head = |n: &str| sk.heads[sk.bone(n).expect("bone")];
    let d = quat::sub3(head("lowerarm01.L"), head("upperarm01.L"));
    let n = quat::len3(d);
    let d = [d[0] / n, d[1] / n, d[2] / n];
    let angle = (-d[2]).acos().to_degrees();
    let h = d[0].hypot(d[1]);
    let (abd, flex) = (angle * d[0] / h, angle * -d[1] / h);
    let rest = vec![IDENTITY; sk.len()];
    let a = Joint::new(&sk, "shoulder", "L")
        .expect("joint")
        .angles(&sk, &rest)
        .expect("angles");
    assert!(
        (a[0] - flex).abs() < 1e-9 && (a[1] - abd).abs() < 1e-9,
        "{a:?} vs flex {flex}, abd {abd}"
    );
    assert!(
        (abd - 41.7).abs() < 0.1,
        "the A-pose holds the arm about 41.7 degrees out, got {abd}"
    );
    let r = Joint::new(&sk, "shoulder", "R")
        .expect("joint")
        .angles(&sk, &rest)
        .expect("angles");
    assert!(
        (r[0] - a[0]).abs() < 1e-9 && (r[1] - a[1]).abs() < 1e-9,
        "left and right mirror"
    );
}

/// The chest turned 20 degrees about +X (forward, since +X x up = forward): the trunk bends 20
/// forward and nothing else, and the head, left where it was, reads 20 back against the chest.
#[test]
fn chest_bend_is_trunk_flexion() {
    let sk = rig();
    let mut pose = vec![IDENTITY; sk.len()];
    let half = 10f64.to_radians();
    pose[sk.bone("spine01").expect("bone")] = [half.sin(), 0.0, 0.0, half.cos()];
    pose[sk.bone("neck01").expect("bone")] = [-half.sin(), 0.0, 0.0, half.cos()];
    let t = Joint::new(&sk, "trunk", "L")
        .expect("joint")
        .angles(&sk, &pose)
        .expect("angles");
    assert!(
        (t[0] - 20.0).abs() < 1e-9 && t[1].abs() < 1e-9 && t[2].abs() < 1e-9,
        "{t:?}"
    );
    let n = Joint::new(&sk, "neck", "L")
        .expect("joint")
        .angles(&sk, &pose)
        .expect("angles");
    assert!(
        (n[0] + 20.0).abs() < 1e-9 && n[1].abs() < 1e-9 && n[2].abs() < 1e-9,
        "{n:?}"
    );
}

/// Turning the thigh 30 degrees about its own rest direction `d` keeps the swing (still the turn
/// from straight down to `d`) and adds exactly 30 degrees of twist: on the left, +30 about a
/// downward axis turns the front of the thigh inward, internal rotation (positive).
#[test]
fn hip_twist_is_rotation() {
    let sk = rig();
    let head = |n: &str| sk.heads[sk.bone(n).expect("bone")];
    let d = quat::sub3(head("lowerleg01.L"), head("upperleg01.L"));
    let s = 15f64.to_radians().sin() / quat::len3(d);
    let mut pose = vec![IDENTITY; sk.len()];
    pose[sk.bone("upperleg01.L").expect("bone")] = [d[0] * s, d[1] * s, d[2] * s, 15f64.to_radians().cos()];
    let hip = Joint::new(&sk, "hip", "L").expect("joint");
    let rest = hip.angles(&sk, &vec![IDENTITY; sk.len()]).expect("angles");
    let a = hip.angles(&sk, &pose).expect("angles");
    assert!(
        (a[0] - rest[0]).abs() < 1e-9 && (a[1] - rest[1]).abs() < 1e-9,
        "swing kept: {a:?} vs {rest:?}"
    );
    assert!(
        (a[2] - 30.0).abs() < 1e-9,
        "30 degrees of internal rotation: {a:?}"
    );
}

/// Past the narrow tip of the hip's ellipse (abduction 38.6, flexion 132.1) the distance to the
/// region counts, not the band's vertical gap: 45 out and level is 45 - 38.6 = 6.4 past (the tip),
/// straight forward at 140 is 140 - 132.1 = 7.9 past (the top).
#[test]
fn past_is_the_distance_to_the_region() {
    let sk = rig();
    let r = tables(&sk);
    let hip = r.pair("hip_swing").expect("pair");
    assert!((ranges::judge(hip, 45.0, 0.0).past_hard - 6.4).abs() < 1e-9);
    assert!((ranges::judge(hip, 0.0, 140.0).past_hard - 7.9).abs() < 1e-9);
    assert_eq!(ranges::judge(hip, 10.0, 60.0).past_hard, 0.0);
    let far = ranges::judge(hip, 45.0, 120.0).past_hard;
    assert!(
        far > 6.4 && far < 60.0,
        "45 out and 120 forward is some way past, not 120: {far}"
    );
}
