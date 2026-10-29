//! Analytic two-bone IK for arms and legs (a port of cf-animate's `ik.py`).
//!
//! Each limb is upper -> lower -> end. Only the upper and the lower bone turn, as rigid rotations
//! about their joints, so twist bones keep their local rotations and the solve is exact in one pass:
//!
//! 1. bend the lower joint so `|end - upper|` matches the distance to the target;
//! 2. swing the upper bone so the end points at the target;
//! 3. turn the limb about the upper->target axis so the middle joint faces the pole.
//!
//! The default pole keeps the current bend plane, so a drag never flips a knee.

use std::f64::consts::PI;

use crate::error::CoreError;
use crate::quat::{self, Mat3, Quat, Vec3, add3, len3, scale3, sub3};
use crate::rig::Skeleton;

/// The middle joint this far (x the upper bone) off the limb axis turns fully to the pole.
const POLE_SURE: f64 = 0.15;
/// A limb bent less than this takes its bend axis from the pole (or the body), not itself.
const STRAIGHT_DEG: f64 = 5.0;
/// A limb's own middle joint as the pole is trusted fully from this bend on.
const POLE_BENT_DEG: f64 = 20.0;

/// Limb name -> (upper, lower, end) bones.
pub const LIMBS: &[(&str, [&str; 3])] = &[
    ("arm.L", ["upperarm01.L", "lowerarm01.L", "wrist.L"]),
    ("leg.L", ["upperleg01.L", "lowerleg01.L", "foot.L"]),
    ("arm.R", ["upperarm01.R", "lowerarm01.R", "wrist.R"]),
    ("leg.R", ["upperleg01.R", "lowerleg01.R", "foot.R"]),
];

pub fn limb_bones(limb: &str) -> Option<[&'static str; 3]> {
    LIMBS.iter().find(|(n, _)| *n == limb).map(|(_, b)| *b)
}

/// The limb an effector (a hand or a foot) belongs to.
pub fn limb_of_effector(bone: &str) -> Option<&'static str> {
    LIMBS.iter().find(|(_, b)| b[2] == bone).map(|(n, _)| *n)
}

fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn unit(v: Vec3) -> Vec3 {
    let n = len3(v);
    if n > 1e-12 {
        scale3(v, 1.0 / n)
    } else {
        scale3(v, 0.0)
    }
}

fn transpose(m: &Mat3) -> Mat3 {
    [
        [m[0][0], m[1][0], m[2][0]],
        [m[0][1], m[1][1], m[2][1]],
        [m[0][2], m[1][2], m[2][2]],
    ]
}

pub fn axis_angle(axis: Vec3, angle: f64) -> Mat3 {
    let [x, y, z] = unit(axis);
    let (c, s) = (angle.cos(), angle.sin());
    let cc = 1.0 - c;
    [
        [c + x * x * cc, x * y * cc - z * s, x * z * cc + y * s],
        [y * x * cc + z * s, c + y * y * cc, y * z * cc - x * s],
        [z * x * cc - y * s, z * y * cc + x * s, c + z * z * cc],
    ]
}

fn rotation_between(a: Vec3, b: Vec3) -> Mat3 {
    let (a, b) = (unit(a), unit(b));
    let axis = cross(a, b);
    let s = len3(axis);
    let c = dot(a, b).clamp(-1.0, 1.0);
    if s < 1e-12 {
        if c > 0.0 {
            return quat::MAT3_IDENTITY;
        }
        let mut perp = cross(a, [1.0, 0.0, 0.0]);
        if len3(perp) < 1e-6 {
            perp = cross(a, [0.0, 1.0, 0.0]);
        }
        return axis_angle(perp, PI);
    }
    axis_angle(axis, s.atan2(c))
}

fn get<T: Copy>(v: &[T], i: usize, what: &str) -> Result<T, CoreError> {
    v.get(i)
        .copied()
        .ok_or_else(|| CoreError::Pose(format!("{what} {i} out of range")))
}

/// Pre-multiply the world rotation of `bone` by `delta` (its children follow).
fn apply_world(
    sk: &Skeleton,
    rots: &mut [Quat],
    root: Vec3,
    bone: usize,
    delta: &Mat3,
) -> Result<(), CoreError> {
    let (r, _) = sk.fk(rots, root)?;
    let rp = match sk.parents.get(bone).copied().flatten() {
        Some(p) => get(&r, p, "bone")?,
        None => quat::MAT3_IDENTITY,
    };
    let rb = get(&r, bone, "bone")?;
    let local = quat::mat3_mul(&quat::mat3_mul(&transpose(&rp), delta), &rb);
    if let Some(slot) = rots.get_mut(bone) {
        *slot = quat::from_mat3(&local);
    }
    Ok(())
}

fn index(sk: &Skeleton, name: &str) -> Result<usize, CoreError> {
    sk.bone(name)
        .ok_or_else(|| CoreError::Pose(format!("the rig has no bone {name:?}")))
}

/// The way `limb`'s middle joint points in this pose, as a pole for [`two_bone`]; `None` when the
/// limb is (nearly) straight and has no bend plane of its own. A solve that holds a foot on a
/// frame of an animation passes this, so a knee the pose turns out (a crouch) stays out instead of
/// turning to the default in front of the hips, and frames next to an untouched key don't jump.
pub fn own_pole(
    sk: &Skeleton,
    rotations: &[Quat],
    root: Vec3,
    limb: &str,
) -> Result<Option<Vec3>, CoreError> {
    let bones = limb_bones(limb).ok_or_else(|| CoreError::Pose(format!("unknown limb {limb:?}")))?;
    let (_, p) = sk.fk(rotations, root)?;
    let a = get(&p, index(sk, bones[0])?, "bone")?;
    let b = get(&p, index(sk, bones[1])?, "bone")?;
    let c = get(&p, index(sk, bones[2])?, "bone")?;
    let (la, lb) = (len3(sub3(b, a)), len3(sub3(c, b)));
    if len3(cross(sub3(a, b), sub3(c, b))) < STRAIGHT_DEG.to_radians().sin() * la * lb {
        return Ok(None);
    }
    let ac = unit(sub3(c, a));
    let mid = add3(a, scale3(ac, dot(sub3(b, a), ac)));
    Ok(Some(add3(b, sub3(b, mid))))
}

/// Rotations with the end of `limb` at `target` (clamped to reach).
///
/// `pole` is a world point the elbow/knee turns toward; `None` keeps the current bend plane (legs:
/// a pole in front of the hips). `keep_end_rotation` keeps the end's world orientation; by default
/// feet do (a planted foot stays flat) and hands do not.
pub fn two_bone(
    sk: &Skeleton,
    rotations: &[Quat],
    root: Vec3,
    limb: &str,
    target: Vec3,
    pole: Option<Vec3>,
    keep_end_rotation: Option<bool>,
) -> Result<Vec<Quat>, CoreError> {
    let bones = limb_bones(limb).ok_or_else(|| CoreError::Pose(format!("unknown limb {limb:?}")))?;
    let is_leg = limb.starts_with("leg");
    let keep_end = keep_end_rotation.unwrap_or(is_leg);
    let (iu, il, ie) = (index(sk, bones[0])?, index(sk, bones[1])?, index(sk, bones[2])?);
    let mut rots = rotations.to_vec();
    let (r0, p) = sk.fk(&rots, root)?;
    let end_world = get(&r0, ie, "bone")?;
    let (a, b, c) = (get(&p, iu, "bone")?, get(&p, il, "bone")?, get(&p, ie, "bone")?);
    let (la, lb) = (len3(sub3(b, a)), len3(sub3(c, b)));

    let pole = match pole {
        Some(p) => p,
        None if is_leg => {
            // knees forward of the hips: the pelvis's facing, whatever the leg's current twist
            add3(
                scale3(add3(a, target), 0.5),
                scale3(fallback_bend(sk, limb, &r0)?, la + lb),
            )
        }
        None => {
            let ac = unit(sub3(c, a));
            let mid = add3(a, scale3(ac, dot(sub3(b, a), ac)));
            let mut bend = sub3(b, mid);
            if len3(bend) < 1e-4 * la {
                bend = fallback_bend(sk, limb, &r0)?;
            }
            add3(b, bend)
        }
    };

    // 1. bend at the lower joint
    let d = len3(sub3(target, a)).clamp((la - lb).abs() + 1e-6, la + lb - 1e-6);
    let want = ((la * la + lb * lb - d * d) / (2.0 * la * lb))
        .clamp(-1.0, 1.0)
        .acos();
    let have = dot(unit(sub3(a, b)), unit(sub3(c, b))).clamp(-1.0, 1.0).acos();
    let mut n = cross(sub3(a, b), sub3(c, b));
    // a (near-)straight limb's own bend axis is noise: take it from the pole (or the body)
    if len3(n) < STRAIGHT_DEG.to_radians().sin() * la * lb {
        n = cross(sub3(a, b), sub3(b, pole));
        if len3(n) < STRAIGHT_DEG.to_radians().sin() * la * len3(sub3(pole, b)).max(1e-9) {
            n = cross(sub3(a, b), scale3(fallback_bend(sk, limb, &r0)?, -1.0));
        }
    }
    apply_world(sk, &mut rots, root, il, &axis_angle(n, want - have))?;

    // 2. swing the upper bone so the end points at the target
    let (_, p) = sk.fk(&rots, root)?;
    let swing = rotation_between(sub3(get(&p, ie, "bone")?, a), sub3(target, a));
    apply_world(sk, &mut rots, root, iu, &swing)?;

    // 3. twist about A->target so the middle joint faces the pole
    let (_, p) = sk.fk(&rots, root)?;
    let axis = unit(sub3(target, a));
    let mut bv = sub3(get(&p, il, "bone")?, a);
    bv = sub3(bv, scale3(axis, dot(bv, axis)));
    let mut pv = sub3(pole, a);
    pv = sub3(pv, scale3(axis, dot(pv, axis)));
    if len3(bv) > 1e-9 && len3(pv) > 1e-9 {
        let (ub, up) = (unit(bv), unit(pv));
        let ang = dot(cross(ub, up), axis).atan2(dot(ub, up));
        // turn only as far as the joint clearly sits off the axis (and the pole does too)
        let mut sure = len3(bv).min(len3(pv)) / (POLE_SURE * la);
        if len3(sub3(pole, b)) < 0.01 {
            // the pole is the limb's own middle joint: a straight limb's joint faces nowhere
            sure *= have.sin() / POLE_BENT_DEG.to_radians().sin();
        }
        apply_world(sk, &mut rots, root, iu, &axis_angle(axis, ang * sure.min(1.0)))?;
    }

    if keep_end {
        let (r, _) = sk.fk(&rots, root)?;
        let parent = sk
            .parents
            .get(ie)
            .copied()
            .flatten()
            .ok_or_else(|| CoreError::Pose("end bone has no parent".into()))?;
        let rp = get(&r, parent, "bone")?;
        if let Some(slot) = rots.get_mut(ie) {
            *slot = quat::from_mat3(&quat::mat3_mul(&transpose(&rp), &end_world));
        }
    }
    Ok(rots)
}

/// Bend direction for a straight limb: knees forward (over their own toes), elbows back.
fn fallback_bend(sk: &Skeleton, limb: &str, r: &[Mat3]) -> Result<Vec3, CoreError> {
    let side = limb.rsplit('.').next().unwrap_or("L");
    let (it, ifoot) = (
        index(sk, &format!("toe3-1.{side}"))?,
        index(sk, &format!("foot.{side}"))?,
    );
    let (toe, foot) = (get(&sk.heads, it, "bone")?, get(&sk.heads, ifoot, "bone")?);
    let is_leg = limb.starts_with("leg");
    if is_leg {
        let mut own = quat::mat3_vec(&get(r, ifoot, "bone")?, sub3(toe, foot));
        own[2] = 0.0;
        if len3(own) > 1e-6 {
            return Ok(unit(own));
        }
    }
    let mut fwd = sub3(toe, foot);
    fwd[2] = 0.0;
    // turn the rest direction by the body's yaw only, taken from the hip line
    let r0 = get(r, 0, "bone")?;
    let left = [r0[0][0], r0[1][0], r0[2][0]];
    let yaw = left[1].atan2(left[0]);
    let (c, s) = (yaw.cos(), yaw.sin());
    let fwd = unit([c * fwd[0] - s * fwd[1], s * fwd[0] + c * fwd[1], 0.0]);
    Ok(if is_leg { fwd } else { scale3(fwd, -1.0) })
}

/// Move the root joint to `position` with the ends of `pinned` limbs staying put.
pub fn move_root(
    sk: &Skeleton,
    rotations: &[Quat],
    root: Vec3,
    position: Vec3,
    pinned: &[&str],
) -> Result<(Vec<Quat>, Vec3), CoreError> {
    let (_, p) = sk.fk(rotations, root)?;
    let mut keep = Vec::new();
    for limb in pinned {
        let bones = limb_bones(limb).ok_or_else(|| CoreError::Pose(format!("unknown limb {limb:?}")))?;
        keep.push((*limb, get(&p, index(sk, bones[2])?, "bone")?));
    }
    let new_root = add3(
        root,
        scale3(sub3(position, get(&p, 0, "bone")?), 1.0 / sk.hip_height),
    );
    let mut rots = rotations.to_vec();
    for (limb, at) in keep {
        rots = two_bone(sk, &rots, new_root, limb, at, None, None)?;
    }
    Ok((rots, new_root))
}

/// Turn `bone`'s parent so that the bone's head follows `target`.
pub fn turn_parent(
    sk: &Skeleton,
    rotations: &[Quat],
    root: Vec3,
    bone: usize,
    target: Vec3,
) -> Result<Vec<Quat>, CoreError> {
    let p = sk
        .parents
        .get(bone)
        .copied()
        .flatten()
        .ok_or_else(|| CoreError::Pose("the root has no parent to turn".into()))?;
    let (r, pos) = sk.fk(rotations, root)?;
    let pp = get(&pos, p, "bone")?;
    let delta = rotation_between(sub3(get(&pos, bone, "bone")?, pp), sub3(target, pp));
    let rg = match sk.parents.get(p).copied().flatten() {
        Some(g) => get(&r, g, "bone")?,
        None => quat::MAT3_IDENTITY,
    };
    let mut rots = rotations.to_vec();
    if let Some(slot) = rots.get_mut(p) {
        *slot = quat::from_mat3(&quat::mat3_mul(
            &quat::mat3_mul(&transpose(&rg), &delta),
            &get(&r, p, "bone")?,
        ));
    }
    Ok(rots)
}
