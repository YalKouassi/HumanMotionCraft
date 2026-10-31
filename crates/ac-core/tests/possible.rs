//! Impossible poses (`ac_core::possible`): setting a joint's angles, bending one back, and the
//! check an AI fill runs on its frames.

use std::sync::Arc;

use ac_core::anatomy::{self, Joint};
use ac_core::clip::{Interp, Key, KeySource};
use ac_core::possible::{MARGIN_DEG, Possible};
use ac_core::quat::IDENTITY;
use ac_core::ranges::Ranges;
use ac_core::{Document, Skeleton};

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

fn every_joint(sk: &Skeleton) -> Vec<(String, Joint)> {
    let mut out = Vec::new();
    for &j in anatomy::JOINTS {
        for side in if anatomy::sided(j) {
            &["L", "R"][..]
        } else {
            &["L"][..]
        } {
            out.push((format!("{j}.{side}"), Joint::new(sk, j, side).expect("joint")));
        }
    }
    out
}

/// On every check pose in ranges.json (recorded mocap), each joint set to its own angles plus a
/// change reads exactly that, and no other joint moves.
#[test]
fn set_angles_reads_back_and_leaves_the_other_joints() {
    let sk = rig();
    let r = tables(&sk);
    let joints = every_joint(&sk);
    for (n, check) in r.checks.iter().enumerate() {
        for (name, joint) in &joints {
            let mut pose = check.rotations.clone();
            let before: Vec<[f64; 3]> = joints
                .iter()
                .map(|(_, j)| j.angles(&sk, &pose).expect("angles"))
                .collect();
            let a = joint.angles(&sk, &pose).expect("angles");
            let want = [a[0] + 7.0, a[1] - 5.0, a[2] + 11.0];
            joint.set_angles(&sk, &mut pose, want).expect("set");
            let got = joint.angles(&sk, &pose).expect("angles");
            for k in 0..3 {
                assert!(
                    (got[k] - want[k]).abs() < 1e-6,
                    "check {n} {name}: wanted {want:?}, read {got:?}"
                );
            }
            for ((other, j), was) in joints.iter().zip(&before) {
                if other == name {
                    continue;
                }
                let now = j.angles(&sk, &pose).expect("angles");
                for k in 0..3 {
                    assert!(
                        (now[k] - was[k]).abs() < 1e-6,
                        "check {n}: setting {name} moved {other}: {was:?} -> {now:?}"
                    );
                }
            }
        }
    }
}

/// A left foot turned 110 degrees out (30 past the toe-out limit of 80) is bent back to
/// 80 - MARGIN_DEG, its flexion kept.
#[test]
fn fix_bends_a_turned_foot_back_to_the_limit() {
    let sk = rig();
    let possible = Possible::new(&sk, &tables(&sk)).expect("limits");
    let ankle = Joint::new(&sk, "ankle", "L").expect("joint");
    let mut pose = vec![IDENTITY; sk.len()];
    ankle.set_angles(&sk, &mut pose, [5.0, 110.0, 0.0]).expect("set");
    let b = possible.breaches(&sk, &pose).expect("breaches");
    assert_eq!(b.len(), 1, "{b:?}");
    assert!((b[0].past() - 30.0).abs() < 1e-6 && b[0].joint == "ankle.L");
    let (fixed, left) = possible.fix(&sk, &mut pose).expect("fix");
    assert_eq!((fixed.len(), left.len()), (1, 0));
    let a = ankle.angles(&sk, &pose).expect("angles");
    assert!(
        (a[0] - 5.0).abs() < 1e-6 && (a[1] - (80.0 - MARGIN_DEG)).abs() < 1e-6,
        "{a:?}"
    );
    assert!(possible.breaches(&sk, &pose).expect("breaches").is_empty());
}

/// An AI key with a foot turned round is bent back by the check every fill runs, and the note
/// says which frame; the keys of your own are left alone.
#[test]
fn keep_possible_bends_the_fill_back_and_says_so() {
    let sk = rig();
    let mut doc = Document::new(sk.clone());
    doc.ranges = Some(Arc::new(tables(&sk)));
    let rest = vec![IDENTITY; sk.len()];
    let mut bad = rest.clone();
    Joint::new(&sk, "ankle", "R")
        .expect("joint")
        .set_angles(&sk, &mut bad, [0.0, -150.0, 0.0])
        .expect("set");
    for f in [0, 4] {
        doc.clip
            .set_key(Key::new(f, rest.clone(), [0.0; 3], Interp::Linear))
            .expect("key");
    }
    for f in 1..4 {
        let mut k = Key::new(
            f,
            if f == 2 { bad.clone() } else { rest.clone() },
            [0.0; 3],
            Interp::Linear,
        );
        k.source = KeySource::Ai;
        k.model = "test".into();
        doc.clip.set_key(k).expect("key");
    }
    let kept = doc.keep_possible_report(0, 4).expect("checked");
    let notes = &kept.notes;
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(
        notes[0].contains("frames 2;") && notes[0].contains("ankle.R abduction"),
        "{notes:?}"
    );
    // the same, as data: one joint, bent on frame 2 only, by as much as the note says
    assert!(kept.checked);
    assert_eq!(kept.bent.len(), 1, "{:?}", kept.bent);
    let b = &kept.bent[0];
    assert_eq!(
        (b.joint.as_str(), b.component, b.worst_frame, &b.frames[..]),
        ("ankle.R", "abduction", 2, &[2][..])
    );
    assert!(
        notes[0].contains(&format!("{:.0}° past", b.past_deg)) && b.past_deg > 0.0,
        "{b:?} vs {notes:?}"
    );
    let possible = doc.possible().expect("limits").expect("loaded");
    let (rots, _) = doc.clip.sample_with(&[2.0], &doc.links).expect("sample");
    assert!(possible.breaches(&sk, &rots[0]).expect("breaches").is_empty());
    assert!(
        doc.keep_possible(0, 4).expect("checked").is_empty(),
        "nothing left to bend"
    );
}
