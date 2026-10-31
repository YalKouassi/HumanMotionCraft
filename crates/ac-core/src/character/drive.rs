//! One frame: the rig's pose -> the character's world transforms. The hips go where the rig's
//! hip joints are (scaled, see [`super::plan::Plan::map_point`]); every driven node takes its
//! bone's world turn through its alignment; the rest follow their parents at rest. Then, with a
//! weight per foot, each leg is solved by two-bone IK so the ankle lands where the rig's ankle is
//! (scaled), the knee bending the way the rig's knee does, the foot keeping its orientation. A
//! target out of reach is reached as far as the leg goes and the gap is reported. With
//! `Plan::hands`, the arms are solved the same way onto the wrists.

use crate::quat::{self, Mat3, Vec3};
use crate::rig::Skeleton;

use super::World;
use super::plan::{Limb, Plan};

/// How much each limb's IK counts this frame (0: forward kinematics only, 1: on the target).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weights {
    pub feet: [f64; 2],
    pub hands: [f64; 2],
}

impl Weights {
    pub const NONE: Weights = Weights {
        feet: [0.0; 2],
        hands: [0.0; 2],
    };
    pub const FEET: Weights = Weights {
        feet: [1.0; 2],
        hands: [0.0; 2],
    };
}

/// The character on one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Posed {
    pub worlds: Vec<World>,
    /// Per foot: how far short of its target the ankle stayed (m; 0 when reached).
    pub unreached: [f64; 2],
    /// The same for the hands (0 when hands are not solved).
    pub hands_unreached: [f64; 2],
}

fn unit(v: Vec3) -> Vec3 {
    let l = quat::len3(v);
    if l > 1e-12 {
        quat::scale3(v, 1.0 / l)
    } else {
        [0.0; 3]
    }
}

fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The pass over every node: driven nodes take `R_b · A · W_rest.r`, the hips are placed, the
/// rest follow their parents; `over` replaces the world rotation of a node (the IK's result).
fn pass(
    plan: &Plan,
    c: &super::Character,
    rots: &[Mat3],
    hips_p: Vec3,
    over: &[(usize, Mat3)],
) -> Vec<World> {
    let mut out = plan.rest.clone();
    for &i in c.order() {
        let node = &c.nodes[i];
        let parent = node.parent.and_then(|p| out.get(p).copied());
        let mut w = match parent {
            Some(p) => p.child(node.translation, node.rotation, node.scale),
            None => World::IDENTITY.child(node.translation, node.rotation, node.scale),
        };
        if let Some(d) = plan.drive_of(i) {
            let rb = rots.get(d.bone).copied().unwrap_or(quat::MAT3_IDENTITY);
            w.r = quat::mat3_mul(&quat::mat3_mul(&rb, &d.align), &plan.rest[i].r);
        }
        if i == plan.hips {
            w.p = hips_p;
        }
        if let Some((_, r)) = over.iter().find(|(n, _)| *n == i) {
            w.r = *r;
        }
        if !c.under(i, plan.hips) {
            w = plan.rest[i];
        }
        out[i] = w;
    }
    out
}

/// Two-bone IK in world space: new world rotations for the upper and lower nodes so the end
/// lands on `target` (as far as the limb reaches), the middle joint bending towards `pole`
/// (a direction); returns the rotations and how far short the end stayed.
fn two_bone(worlds: &[World], limb: &Limb, target: Vec3, pole: Vec3) -> Option<([(usize, Mat3); 2], f64)> {
    let a = worlds.get(limb.upper)?.p;
    let b = worlds.get(limb.lower)?.p;
    let c = worlds.get(limb.end)?.p;
    let (la, lb) = (quat::len3(quat::sub3(b, a)), quat::len3(quat::sub3(c, b)));
    if la < 1e-9 || lb < 1e-9 {
        return None;
    }
    let to = quat::sub3(target, a);
    let dist = quat::len3(to);
    if dist < 1e-9 {
        return None;
    }
    let reach = la + lb;
    let d = dist.clamp((la - lb).abs() + 1e-6, reach - 1e-6);
    let short = (dist - reach).max(0.0);
    let u = unit(to);
    // the bend direction: the pole across u; falling back on the current bend, then any side
    let mut n = quat::sub3(pole, quat::scale3(u, dot(pole, u)));
    if quat::len3(n) < 1e-6 {
        let cur = quat::sub3(b, a);
        n = quat::sub3(cur, quat::scale3(u, dot(cur, u)));
    }
    if quat::len3(n) < 1e-6 {
        let pick = if u[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        n = quat::sub3(pick, quat::scale3(u, dot(pick, u)));
    }
    let n = unit(n);
    let cos_a = ((la * la + d * d - lb * lb) / (2.0 * la * d)).clamp(-1.0, 1.0);
    let sin_a = (1.0 - cos_a * cos_a).max(0.0).sqrt();
    let b_new = quat::add3(
        a,
        quat::add3(quat::scale3(u, la * cos_a), quat::scale3(n, la * sin_a)),
    );
    let c_new = quat::add3(a, quat::scale3(u, d));
    let upper = worlds.get(limb.upper)?.r;
    let lower = worlds.get(limb.lower)?.r;
    let d_upper = super::plan::align(quat::sub3(b, a), quat::sub3(b_new, a))?;
    let upper_new = quat::mat3_mul(&d_upper, &upper);
    // the lower bone moved with the upper: aim it from there
    let c_moved = quat::add3(b_new, quat::mat3_vec(&d_upper, quat::sub3(c, b)));
    let d_lower = super::plan::align(quat::sub3(c_moved, b_new), quat::sub3(c_new, b_new))?;
    let lower_new = quat::mat3_mul(&d_lower, &quat::mat3_mul(&d_upper, &lower));
    Some(([(limb.upper, upper_new), (limb.lower, lower_new)], short))
}

/// Drive the character from one rig pose.
pub fn drive(
    plan: &Plan,
    c: &super::Character,
    sk: &Skeleton,
    rotations: &[quat::Quat],
    root: Vec3,
    weights: Weights,
) -> Result<Posed, String> {
    if c.len() != plan.rest.len() {
        return Err("the plan was made for another character".into());
    }
    let (rots, pos) = sk.fk(rotations, root).map_err(|e| e.to_string())?;
    let at = |b: usize| pos.get(b).copied().unwrap_or_default();
    let mid = quat::scale3(quat::add3(at(plan.hip_bones[0]), at(plan.hip_bones[1])), 0.5);
    let r_root = rots.get(plan.root_bone).copied().unwrap_or(quat::MAT3_IDENTITY);
    let hips_p = quat::add3(plan.map_point(mid), quat::mat3_vec(&r_root, plan.hips_offset));
    let fk = pass(plan, c, &rots, hips_p, &[]);
    let mut over: Vec<(usize, Mat3)> = Vec::new();
    let mut unreached = [0.0; 2];
    let mut hands_unreached = [0.0; 2];
    let limbs: Vec<(Limb, f64, &mut f64)> = plan
        .legs
        .iter()
        .zip(weights.feet)
        .zip(unreached.iter_mut())
        .map(|((l, w), u)| (*l, w, u))
        .chain(
            plan.arms
                .iter()
                .zip(weights.hands)
                .zip(hands_unreached.iter_mut())
                .map(|((l, w), u)| (*l, if plan.hands { w } else { 0.0 }, u)),
        )
        .collect();
    for (limb, w, short) in limbs {
        if w.is_nan() || w <= 0.0 {
            continue;
        }
        let w = w.min(1.0);
        let wanted = plan.map_point(at(limb.end_bone));
        let fk_end = fk.get(limb.end).map_or(wanted, |x| x.p);
        let target = quat::add3(quat::scale3(fk_end, 1.0 - w), quat::scale3(wanted, w));
        // the knee (elbow) bends the way the rig's does
        let (ha, hb, hc) = (at(limb.upper_bone), at(limb.lower_bone), at(limb.end_bone));
        let axis = unit(quat::sub3(hc, ha));
        let bend = quat::sub3(hb, ha);
        let pole = quat::sub3(bend, quat::scale3(axis, dot(bend, axis)));
        if let Some((rs, gap)) = two_bone(&fk, &limb, target, pole) {
            over.extend(rs);
            *short = gap * w;
        }
    }
    let worlds = if over.is_empty() {
        fk
    } else {
        pass(plan, c, &rots, hips_p, &over)
    };
    Ok(Posed {
        worlds,
        unreached,
        hands_unreached,
    })
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )]
    use super::*;
    use crate::character::{Character, Node};

    fn node(name: &str, parent: Option<usize>, t: Vec3) -> Node {
        Node {
            name: name.into(),
            parent,
            translation: t,
            rotation: quat::IDENTITY,
            scale: 1.0,
        }
    }

    #[test]
    fn two_bone_reaches_a_target_in_the_pole_plane_and_reports_the_gap() {
        // a straight leg down Z: hip at 1, knee at 0.5, ankle at 0
        let nodes = vec![
            node("hip", None, [0.0, 0.0, 1.0]),
            node("knee", Some(0), [0.0, 0.0, -0.5]),
            node("ankle", Some(1), [0.0, 0.0, -0.5]),
        ];
        let c = Character::new(nodes, vec![], vec![], false).unwrap();
        let worlds = c.rest_worlds();
        let limb = Limb {
            upper: 0,
            lower: 1,
            end: 2,
            upper_bone: 0,
            lower_bone: 0,
            end_bone: 0,
        };
        let (rs, gap) = two_bone(&worlds, &limb, [0.0, 0.0, 0.2], [0.0, -1.0, 0.0]).unwrap();
        assert!(gap.abs() < 1e-12);
        // apply: knee = hip + R_upper * (0, 0, -0.5); ankle = knee + R_lower * (0, 0, -0.5)
        let knee = quat::add3([0.0, 0.0, 1.0], quat::mat3_vec(&rs[0].1, [0.0, 0.0, -0.5]));
        let ankle = quat::add3(knee, quat::mat3_vec(&rs[1].1, [0.0, 0.0, -0.5]));
        assert!(
            (ankle[2] - 0.2).abs() < 1e-9 && ankle[0].abs() < 1e-9 && ankle[1].abs() < 1e-9,
            "{ankle:?}"
        );
        assert!(knee[1] < -0.1, "the knee bends towards the pole (-Y): {knee:?}");
        let (_, gap) = two_bone(&worlds, &limb, [0.0, 0.0, -0.5], [0.0, -1.0, 0.0]).unwrap();
        assert!((gap - 0.5).abs() < 1e-9, "{gap}");
    }
}
