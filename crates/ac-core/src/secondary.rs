//! Secondary motion worked out from the motion below (roadmap 119800): arm swing, shoulders, pelvis,
//! spine counter-rotation, head stabilisation, hands and follow-through. It returns one additive
//! turn per frame and bone for a layer of its own, so nothing here touches a key of the person's;
//! `layer.secondary` is the command. No model and no randomness: hand-keyed clips work the same as
//! any other, and AI Fill never calls this.
//!
//! Everything is driven by what the feet and hips already do. `swing` is how far the left foot is
//! ahead of the right (+1: left ahead), with any lasting stance taken off, so a stand gives nothing.
//! The legs follow a turned pelvis by IK, each ankle back where the keys had it.

use crate::gait;
use crate::ik;
use crate::quat::{self, Quat, Vec3};
use crate::rig::Skeleton;

type R<T> = Result<T, String>;

/// The parts `layer.secondary` can make.
pub const PARTS: &[&str] = &["arms", "shoulders", "pelvis", "spine", "head", "hands", "follow"];

const DEG: f64 = std::f64::consts::PI / 180.0;
// How big each part is at `amount` 1, in radians at a full stride. Judged for an ordinary walk; the
// owner reviews them by eye (roadmap 119800).
const ARM: f64 = 20.0 * DEG;
const ELBOW_SHARE: f64 = 0.4;
const CLAVICLE: f64 = 4.0 * DEG;
const PELVIS_YAW: f64 = 6.0 * DEG;
const PELVIS_ROLL: f64 = 3.0 * DEG;
const SPINE_YAW: f64 = 6.0 * DEG;
const WRIST_SHARE: f64 = 0.6;
/// The foot separation that counts as a full stride, in hip heights.
const FULL_STRIDE: f64 = 0.35;
/// Seconds either side over which a lasting stance, a facing or a trunk's lean is averaged.
const SLOW: f64 = 0.5;
/// Seconds the hands and forearms trail the upper arm by.
const TRAIL: f64 = 0.067;
/// The follow-through spring: its frequency (Hz) and damping ratio, the lean a metre of lag gives
/// (radians), the most it leans, and the body's acceleration it listens to (m/s²; past this is a
/// jump in the keys, not a stop).
const SPRING_HZ: f64 = 1.2;
const SPRING_DAMPING: f64 = 0.35;
const FOLLOW_PER_METRE: f64 = 1.0;
const FOLLOW_MAX: f64 = 12.0 * DEG;
const MAX_ACCEL: f64 = 20.0;
/// Seconds the acceleration is smoothed over either side, so steps don't shake the spring.
const ACCEL_SMOOTH: f64 = 0.125;
/// The most the head corrects for the trunk's sway.
const SWAY_MAX: f64 = 25.0 * DEG;
/// A turn smaller than this (radians) is no turn.
const NOTHING: f64 = 1e-9;

const SPINE: [&str; 5] = ["spine05", "spine04", "spine03", "spine02", "spine01"];
/// The spine bones the follow-through leans.
const UPPER_SPINE: [&str; 3] = ["spine03", "spine02", "spine01"];
const NECK: [&str; 4] = ["neck01", "neck02", "neck03", "head"];
/// Each side, and which way its arm goes with `swing`.
const SIDES: [(&str, f64); 2] = [("L", -1.0), ("R", 1.0)];

/// What to make.
#[derive(Debug, Clone)]
pub struct Options {
    pub amount: f64,
    /// Names from [`PARTS`].
    pub parts: Vec<String>,
}

impl Options {
    fn has(&self, part: &str) -> bool {
        self.parts.iter().any(|p| p == part)
    }
}

/// The motion below the layer, frame by frame.
pub struct Below<'a> {
    pub rotations: &'a [Vec<Quat>],
    pub roots: &'a [Vec3],
    pub fps: f64,
    pub looping: bool,
}

/// The layer's content.
#[derive(Debug, Clone)]
pub struct Motion {
    /// `[frame][bone]`: the turn on top of the pose below.
    pub turns: Vec<Vec<Quat>>,
    /// The bones it turns on some frame, ascending.
    pub bones: Vec<usize>,
    /// The furthest an ankle ends up from where the motion below has it (m).
    pub foot_drift: f64,
    /// The largest turn of any bone on any frame (radians).
    pub largest: f64,
}

fn bone(sk: &Skeleton, name: &str) -> R<usize> {
    sk.bone(name).ok_or_else(|| format!("the rig has no {name}"))
}

fn axis_turn(axis: Vec3, angle: f64) -> Quat {
    quat::exp(quat::scale3(axis, angle / 2.0))
}

fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// A bone's world rotation.
fn world(sk: &Skeleton, rots: &[Quat], i: usize) -> Quat {
    let mut g = rots.get(i).copied().unwrap_or(quat::IDENTITY);
    let mut at = sk.parents.get(i).copied().flatten();
    while let Some(p) = at {
        g = quat::mul(rots.get(p).copied().unwrap_or(quat::IDENTITY), g);
        at = sk.parents.get(p).copied().flatten();
    }
    quat::normalize(g)
}

/// Turn a bone (and so what hangs from it) by `by`, given in world axes.
fn turn(sk: &Skeleton, rots: &mut [Quat], i: usize, by: Quat) {
    let parent = sk
        .parents
        .get(i)
        .copied()
        .flatten()
        .map_or(quat::IDENTITY, |p| world(sk, rots, p));
    if let Some(q) = rots.get_mut(i) {
        let local = quat::mul(quat::conj(parent), quat::mul(by, parent));
        *q = quat::normalize(quat::mul(local, *q));
    }
}

/// How many frames either side a window at frame `i` of `n` takes. A loop wraps; otherwise the
/// window shrinks evenly towards an end, so a clip's first and last frames are their own mean.
fn reach(half: usize, i: usize, n: usize, looping: bool) -> usize {
    if looping {
        half.min(n.saturating_sub(1) / 2)
    } else {
        half.min(i).min(n.saturating_sub(1).saturating_sub(i))
    }
}

/// The mean of a signal over `half` frames either side.
fn mean(signal: &[f64], half: usize, looping: bool) -> Vec<f64> {
    let n = signal.len();
    (0..n)
        .map(|i| {
            let h = reach(half, i, n, looping);
            let sum: f64 = (0..=2 * h)
                .filter_map(|k| signal.get((i + n + k - h) % n))
                .sum();
            sum / (2 * h + 1) as f64
        })
        .collect()
}

/// A rotation averaged over `half` frames either side of each frame.
fn mean_rotation(qs: &[Quat], half: usize, looping: bool) -> Vec<Quat> {
    let n = qs.len();
    (0..n)
        .map(|i| {
            let me = qs.get(i).copied().unwrap_or(quat::IDENTITY);
            let h = reach(half, i, n, looping);
            let mut sum = [0.0; 4];
            for k in 0..=2 * h {
                if let Some(&q) = qs.get((i + n + k - h) % n) {
                    for (s, x) in sum.iter_mut().zip(quat::align(q, me)) {
                        *s += x;
                    }
                }
            }
            if quat::norm(sum) > 1e-9 {
                quat::normalize(sum)
            } else {
                me
            }
        })
        .collect()
}

/// A frame's neighbour `by` frames earlier: wraps on a loop, stops at the start otherwise.
fn earlier(signal: &[f64], i: usize, by: usize, looping: bool) -> f64 {
    let n = signal.len().max(1);
    let at = if looping {
        (i + n - by % n) % n
    } else {
        i.saturating_sub(by)
    };
    signal.get(at).copied().unwrap_or(0.0)
}

/// How far the body leans on (forward positive, radians): a damped spring pushed by its
/// acceleration along the way it faces. A loop runs twice, so its end leads into its start.
fn follow_through(along: &[f64], fps: f64, looping: bool) -> Vec<f64> {
    let n = along.len();
    if n < 3 {
        return vec![0.0; n];
    }
    // speed into each frame; the first frame has none of its own
    let mut v: Vec<f64> = along.windows(2).map(|w| (w[1] - w[0]) * fps).collect();
    let first = v.first().copied().unwrap_or(0.0);
    let last = v.last().copied().unwrap_or(0.0);
    v.insert(0, if looping { last } else { first });
    let mut accel = vec![0.0; n];
    for i in 1..n {
        accel[i] = ((v[i] - v[i - 1]) * fps).clamp(-MAX_ACCEL, MAX_ACCEL);
    }
    let accel = mean(&accel, (ACCEL_SMOOTH * fps).round() as usize, looping);
    let w = 2.0 * std::f64::consts::PI * SPRING_HZ;
    // enough steps a frame to stay stable at any frame rate
    let sub = ((w / fps * 4.0).ceil() as usize).max(1);
    let dt = 1.0 / fps / sub as f64;
    let (mut x, mut speed) = (0.0_f64, 0.0_f64);
    let mut out = vec![0.0; n];
    for _ in 0..if looping { 2 } else { 1 } {
        for (slot, a) in out.iter_mut().zip(&accel) {
            for _ in 0..sub {
                speed += (-w * w * x - 2.0 * SPRING_DAMPING * w * speed - a) * dt;
                x += speed * dt;
            }
            *slot = (x * FOLLOW_PER_METRE).clamp(-FOLLOW_MAX, FOLLOW_MAX);
        }
    }
    out
}

/// The way the body generally faces on each frame, as a unit vector on the floor.
fn forward_of(faces: &[f64], half: usize, looping: bool) -> Vec<Vec3> {
    let cos = mean(&faces.iter().map(|a| a.cos()).collect::<Vec<_>>(), half, looping);
    let sin = mean(&faces.iter().map(|a| a.sin()).collect::<Vec<_>>(), half, looping);
    cos.iter()
        .zip(&sin)
        .zip(faces)
        .map(|((&c, &s), &raw)| {
            let n = (c * c + s * s).sqrt();
            if n > 1e-6 {
                [c / n, s / n, 0.0]
            } else {
                [raw.cos(), raw.sin(), 0.0]
            }
        })
        .collect()
}

/// The axis that swings a part lying along `along` towards `forward`; `None` when it already
/// points that way.
fn swing_axis(along: Vec3, forward: Vec3) -> Option<Vec3> {
    let axis = quat::cross(along, forward);
    let n = quat::len3(axis);
    (n > 1e-3 * quat::len3(along).max(1e-9)).then(|| quat::scale3(axis, 1.0 / n))
}

/// The layer's turns for the motion below.
pub fn motion(sk: &Skeleton, below: &Below, options: &Options) -> R<Motion> {
    let n = below.rotations.len();
    if n != below.roots.len() {
        return Err("the motion below has a different number of poses and roots".into());
    }
    let fps = below.fps.max(1.0);
    let looping = below.looping;
    let slow = (SLOW * fps).round() as usize;
    let trail = ((TRAIL * fps).round() as usize).max(1);
    let amount = options.amount;

    let (foot_l, foot_r, spine1, head) = (
        bone(sk, "foot.L")?,
        bone(sk, "foot.R")?,
        bone(sk, "spine01")?,
        bone(sk, "head")?,
    );

    // what the motion below does
    let mut joints: Vec<Vec<Vec3>> = Vec::with_capacity(n);
    let mut faces = Vec::with_capacity(n);
    let mut trunk = Vec::with_capacity(n);
    for (r, o) in below.rotations.iter().zip(below.roots) {
        let (_, j) = sk.fk(r, *o).map_err(|e| e.to_string())?;
        faces.push(gait::facing(sk, &j)?);
        trunk.push(world(sk, r, spine1));
        joints.push(j);
    }
    let forward = forward_of(&faces, slow, looping);
    let at = |f: usize, b: usize| -> Vec3 {
        joints
            .get(f)
            .and_then(|j| j.get(b).copied())
            .unwrap_or_default()
    };
    let way = |f: usize| forward.get(f).copied().unwrap_or([0.0, -1.0, 0.0]);
    let full = FULL_STRIDE * sk.hip_height;
    let settled = |raw: Vec<f64>| -> Vec<f64> {
        let lasting = mean(&raw, slow, looping);
        let off: Vec<f64> = raw
            .iter()
            .zip(&lasting)
            .map(|(x, m)| ((x - m) / full).clamp(-1.0, 1.0))
            .collect();
        mean(&off, 1, looping)
    };
    let swing = settled(
        (0..n)
            .map(|f| dot(quat::sub3(at(f, foot_l), at(f, foot_r)), way(f)))
            .collect(),
    );
    let lift = settled((0..n).map(|f| at(f, foot_l)[2] - at(f, foot_r)[2]).collect());
    let lean = if options.has("follow") {
        let along: Vec<f64> = (0..n).map(|f| dot(at(f, 0), way(f))).collect();
        follow_through(&along, fps, looping)
    } else {
        vec![0.0; n]
    };
    let trunk_slow = mean_rotation(&trunk, slow, looping);

    // each arm's forward swing as it would be, which the hand trails whether or not the arm is made
    let pendulum: Vec<Vec<f64>> = SIDES
        .iter()
        .map(|(_, sign)| {
            (0..n)
                .map(|f| (sign * ARM * swing[f] + lean[f]) * amount)
                .collect()
        })
        .collect();
    // what the arm itself is turned by
    let arm_angle = |s: usize, f: usize| -> f64 {
        if options.has("arms") {
            pendulum[s][f]
        } else {
            lean[f] * amount
        }
    };

    let arm_bones = SIDES
        .iter()
        .map(|(side, _)| {
            Ok([
                bone(sk, &format!("clavicle.{side}"))?,
                bone(sk, &format!("shoulder01.{side}"))?,
                bone(sk, &format!("upperarm01.{side}"))?,
                bone(sk, &format!("lowerarm01.{side}"))?,
                bone(sk, &format!("wrist.{side}"))?,
            ])
        })
        .collect::<R<Vec<_>>>()?;
    let spine: Vec<usize> = SPINE.iter().map(|b| bone(sk, b)).collect::<R<_>>()?;
    let upper: Vec<usize> = UPPER_SPINE.iter().map(|b| bone(sk, b)).collect::<R<_>>()?;
    let neck: Vec<usize> = NECK.iter().map(|b| bone(sk, b)).collect::<R<_>>()?;

    let up = [0.0, 0.0, 1.0];
    let mut turns = Vec::with_capacity(n);
    let (mut foot_drift, mut largest) = (0.0_f64, 0.0_f64);
    let mut used = vec![false; sk.len()];
    for f in 0..n {
        let (base, root) = (&below.rotations[f], below.roots[f]);
        let mut rots = base.clone();
        let fwd = way(f);
        let left = quat::cross(up, fwd);

        // the pelvis: the left hip forward with the left foot, and down when that foot is lifted
        let (yaw, roll) = if options.has("pelvis") {
            (-PELVIS_YAW * swing[f] * amount, -PELVIS_ROLL * lift[f] * amount)
        } else {
            (0.0, 0.0)
        };
        let pelvis_moves = yaw.abs() > NOTHING || roll.abs() > NOTHING;
        if pelvis_moves {
            turn(sk, &mut rots, 0, quat::mul(axis_turn(fwd, roll), axis_turn(up, yaw)));
        }

        // the spine undoes the pelvis and turns the shoulders the other way
        if options.has("spine") {
            let share = 1.0 / spine.len() as f64;
            let y = (SPINE_YAW * swing[f] * amount - yaw) * share;
            let r = -roll * share;
            if y.abs() > NOTHING || r.abs() > NOTHING {
                for &b in &spine {
                    turn(sk, &mut rots, b, quat::mul(axis_turn(fwd, r), axis_turn(up, y)));
                }
            }
        }
        if lean[f].abs() > NOTHING {
            let share = lean[f] * amount / upper.len() as f64;
            for &b in &upper {
                turn(sk, &mut rots, b, axis_turn(left, share));
            }
        }

        let (_, placed) = sk.fk(&rots, root).map_err(|e| e.to_string())?;
        let pos = |b: usize| placed.get(b).copied().unwrap_or_default();
        for (s, (_, sign)) in SIDES.iter().enumerate() {
            let [clavicle, shoulder, upper_arm, lower_arm, wrist] = arm_bones[s];
            if options.has("shoulders")
                && let Some(axis) = swing_axis(quat::sub3(pos(shoulder), pos(clavicle)), fwd)
            {
                let by = sign * CLAVICLE * swing[f] * amount;
                if by.abs() > NOTHING {
                    turn(sk, &mut rots, clavicle, axis_turn(axis, by));
                }
            }
            let now = arm_angle(s, f);
            let back = (f + n - trail % n) % n;
            let then = arm_angle(s, if looping { back } else { f.saturating_sub(trail) });
            if let Some(axis) = swing_axis(quat::sub3(pos(lower_arm), pos(upper_arm)), fwd)
                && now.abs() > NOTHING
            {
                turn(sk, &mut rots, upper_arm, axis_turn(axis, now));
            }
            if let Some(axis) = swing_axis(quat::sub3(pos(wrist), pos(lower_arm)), fwd) {
                // the elbow bends on the way forward, late; it never bends backwards
                let bend = ELBOW_SHARE * then.max(0.0);
                if bend > NOTHING {
                    turn(sk, &mut rots, lower_arm, axis_turn(axis, bend));
                }
                if options.has("hands") {
                    // the hand trails: it lags behind whichever way the arm is going
                    let drag =
                        WRIST_SHARE * (earlier(&pendulum[s], f, trail, looping) - pendulum[s][f]);
                    if drag.abs() > NOTHING {
                        turn(sk, &mut rots, wrist, axis_turn(axis, drag));
                    }
                }
            }
        }

        // the head keeps the way the keys point it, without the trunk's sway or what this layer added
        if options.has("head") {
            let sway = quat::align(quat::mul(trunk[f], quat::conj(trunk_slow[f])), quat::IDENTITY);
            let angle = 2.0 * quat::len3(quat::log(sway));
            let keep = if angle > SWAY_MAX { SWAY_MAX / angle } else { 1.0 };
            let sway = quat::slerp(quat::IDENTITY, sway, keep * amount.min(1.0));
            let wanted = quat::mul(quat::conj(sway), world(sk, base, head));
            for (k, &b) in neck.iter().enumerate() {
                let now = world(sk, &rots, head);
                let to_do = quat::align(quat::mul(wanted, quat::conj(now)), quat::IDENTITY);
                let share = 1.0 / (neck.len() - k) as f64;
                let by = quat::exp(quat::scale3(quat::log(to_do), share));
                if 2.0 * quat::len3(quat::log(by)) > NOTHING {
                    turn(sk, &mut rots, b, by);
                }
            }
        }

        // the legs take the pelvis's turn: each ankle back where the motion below has it, the foot
        // as it was
        if pelvis_moves {
            for (limb, foot) in [("leg.L", foot_l), ("leg.R", foot_r)] {
                let pole = ik::own_pole(sk, &rots, root, limb).map_err(|e| e.to_string())?;
                rots = ik::two_bone(sk, &rots, root, limb, at(f, foot), pole, None)
                    .map_err(|e| e.to_string())?;
            }
            let (_, moved) = sk.fk(&rots, root).map_err(|e| e.to_string())?;
            for foot in [foot_l, foot_r] {
                let was = at(f, foot);
                let now = moved.get(foot).copied().unwrap_or(was);
                foot_drift = foot_drift.max(quat::len3(quat::sub3(now, was)));
            }
        }

        let frame: Vec<Quat> = base
            .iter()
            .zip(&rots)
            .enumerate()
            .map(|(b, (was, now))| {
                let d = quat::align(quat::normalize(quat::mul(quat::conj(*was), *now)), quat::IDENTITY);
                let angle = 2.0 * quat::len3(quat::log(d));
                if angle <= NOTHING {
                    return quat::IDENTITY;
                }
                if let Some(u) = used.get_mut(b) {
                    *u = true;
                }
                largest = largest.max(angle);
                d
            })
            .collect();
        turns.push(frame);
    }
    let bones = used
        .iter()
        .enumerate()
        .filter_map(|(b, u)| u.then_some(b))
        .collect();
    Ok(Motion {
        turns,
        bones,
        foot_drift,
        largest,
    })
}
