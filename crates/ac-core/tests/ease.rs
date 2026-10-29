//! Where two AI-filled spans meet at your key: the ease through it (`ease.rs`), and foot planting
//! that keeps a bent knee where the pose turns it (`ik::own_pole`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use ac_core::clip::{Clip, Interp, Key, KeySource};
use ac_core::ease::{EASE_FRAMES, ease_through};
use ac_core::quat::{self, Quat, Vec3};
use serde_json::json;

const ID: Quat = [0.0, 0.0, 0.0, 1.0];

/// Keys on frames 0..=20, yours on 0, 10 and 20, AI ones between. The root's x and one bone's
/// angle about X rise a step a frame into frame 10 and fall a step a frame out of it: a V, the
/// kink two spans filled on their own leave at the key they share.
fn v_clip(tag_right: &str) -> (Clip, usize) {
    let sk = common::skeleton();
    let bone = sk.bone("lowerarm01.L").unwrap();
    let mut clip = Clip::new(sk.names.clone(), 30.0);
    for f in 0..=20i64 {
        let x = 1.0 - (f - 10).abs() as f64 * 0.1;
        let mut rots = vec![ID; sk.names.len()];
        rots[bone] = quat::from_euler([x * 40.0, 0.0, 0.0]);
        let mut k = Key::new(f, rots, [x, 0.0, 1.0], Interp::Smooth);
        if f % 10 != 0 {
            k.source = KeySource::Ai;
            k.model = if f > 10 { tag_right.into() } else { "parts".into() };
        }
        clip.set_key(k).unwrap();
    }
    (clip, bone)
}

fn root_x(clip: &Clip) -> Vec<f64> {
    clip.keys().iter().map(|k| k.root[0]).collect()
}

/// The largest change of velocity (a frame to the next) over frames lo..=hi.
fn worst_jolt(x: &[f64], lo: usize, hi: usize) -> f64 {
    (lo..=hi)
        .map(|f| ((x[f + 1] - x[f]) - (x[f] - x[f - 1])).abs())
        .fold(0.0, f64::max)
}

#[test]
fn a_kink_at_a_key_between_two_fills_is_eased_through_it() {
    let (mut clip, bone) = v_clip("parts");
    let before = root_x(&clip);
    let n = EASE_FRAMES as usize;
    // a step a frame up, a step a frame down: the velocity turns by 0.2 on the key
    assert!((worst_jolt(&before, 10, 10) - 0.2).abs() < 1e-9);
    let changed = ease_through(&mut clip, 10, EASE_FRAMES).unwrap();
    assert_eq!(changed, 2 * (n - 1));
    let after = root_x(&clip);
    // the key and everything from the window's edge on are as they were
    for f in (0..=10 - n).chain([10]).chain(10 + n..=20) {
        assert_eq!(after[f], before[f], "frame {f}");
    }
    // the turn is spread over the window: no frame turns the velocity by half as much as the key did
    let jolt = worst_jolt(&after, 10 - n, 10 + n);
    assert!(jolt < 0.1, "worst jolt {jolt}");
    // the bone eases the same way (its angle follows the root's x)
    let ang: Vec<f64> = clip
        .keys()
        .iter()
        .map(|k| quat::to_euler(k.rotations[bone])[0] / 40.0)
        .collect();
    let ang_jolt = worst_jolt(&ang, 10 - n, 10 + n);
    assert!(ang_jolt < 0.1, "bone's worst jolt {ang_jolt}");
    // the eased keys stay AI keys of the span they belong to
    assert!(
        clip.keys()
            .iter()
            .all(|k| (k.frame % 10 == 0) == (k.source == KeySource::Manual))
    );
}

#[test]
fn no_ease_next_to_a_plain_or_text_made_span() {
    // the span after the key was made from text: it is never touched, so nothing eases
    let (mut clip, _) = v_clip("text A person waves.");
    let before = root_x(&clip);
    assert_eq!(ease_through(&mut clip, 10, EASE_FRAMES).unwrap(), 0);
    assert_eq!(root_x(&clip), before);
    // the span after the key is plain (no AI keys): nothing eases either
    let (mut clip, _) = v_clip("parts");
    for f in 11..20 {
        clip.remove_key(f).unwrap();
    }
    let before = root_x(&clip);
    assert_eq!(ease_through(&mut clip, 10, EASE_FRAMES).unwrap(), 0);
    assert_eq!(root_x(&clip), before);
    // an AI key, not yours, is no key to ease through
    let (mut clip, _) = v_clip("parts");
    assert_eq!(ease_through(&mut clip, 5, EASE_FRAMES).unwrap(), 0);
}

#[test]
fn planting_feet_keeps_a_knee_the_pose_turns_out() {
    let mut s = common::session();
    s.run("key.set", &json!({})).unwrap();
    let sk = s.document.skeleton.clone();
    let mut rest = s.document.clip.keys()[0].clone();
    // a crouch with the left knee turned out: the hip flexed and turned out, the knee bent
    let (hip, knee) = (sk.bone("upperleg01.L").unwrap(), sk.bone("lowerleg01.L").unwrap());
    rest.rotations[hip] = quat::from_euler([-60.0, 60.0, 0.0]);
    rest.rotations[knee] = quat::from_euler([100.0, 0.0, 0.0]);
    let at = |r: &[Quat], o: Vec3| sk.fk(r, o).unwrap().1;
    let pose = at(&rest.rotations, rest.root);
    let (foot, knee_at) = (pose[sk.bone("foot.L").unwrap()], pose[knee]);
    // the default pole (knees in front of the hips) would turn this knee well away
    let forward = ac_core::ik::two_bone(&sk, &rest.rotations, rest.root, "leg.L", foot, None, None).unwrap();
    let turned = at(&forward, rest.root)[knee];
    assert!(
        quat::len3(quat::sub3(turned, knee_at)) > 0.2,
        "the test pose doesn't exercise the pole"
    );
    for f in [0, 10] {
        let mut k = Key::new(f, rest.rotations.clone(), rest.root, Interp::Linear);
        k.source = KeySource::Manual;
        s.document.clip.set_key(k).unwrap();
    }
    assert!(s.document.plant_feet(0, 10, 0.25, 0.15).unwrap() > 0);
    let p = s.document.positions(&[5.0]).unwrap();
    let moved = quat::len3(quat::sub3(p[0][knee], knee_at));
    assert!(moved < 1e-3, "the planted frame's knee moved {moved} m");
}
