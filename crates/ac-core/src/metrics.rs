//! Motion quality, measured (`clip.metrics`): what a reviewer looks for in an animation, as numbers
//! an agent can act on. Every rule is simple enough to check by hand:
//!
//! - **Foot sliding.** A foot point (a toe joint's head or the heel, as in [`crate::physics`]) is
//!   *planted* on a frame when its foot is `Planted` ([`crate::contact`]) and it is within [`physics::NEAR`] of the ground and moved slower than
//!   [`physics::PLANT_SPEED`] across it since the frame before (the physics pass's own rule). A
//!   contact span is a run of frames on which a foot has a planted point; its slide is the
//!   longest distance any of its points travelled across the ground while planted on consecutive
//!   frames, in centimetres. A planted foot should not move: anything over [`SLIDE_CM`] shows.
//! - **Floor penetration.** On each frame, the lowest joint head or heel point below the ground
//!   under it (by more than [`PENETRATION_SLACK`]): frames, depth (cm) and the joint.
//! - **Balance.** On a frame where the body is on the ground and stands ([`physics::standing`]; a
//!   walk is balanced by its motion, even on the spot), the centre of mass over the floor should lie inside the support (the hull of the foot points on the
//!   ground). Frames outside, and how far (cm). Moving frames outside are counted apart.
//! - **Joint limits.** Each bone's local rotation vector against `assets/rig/limits.json`: bones
//!   past their range by more than [`LIMIT_SLACK_DEG`], on which frames and by how much (degrees).
//! - **Pops at keys.** At each key with a frame on both sides, the largest change of any joint's
//!   velocity across the key (m/s): `|v_after - v_before|`. Over [`POP_SPEED`] reads as a pop.
//!
//! The motion measured is what plays (`shown`: the physics pass when it is on) or the keys alone
//! (`keys`). Deterministic; the document is not changed.

use serde_json::{Value, json};

use crate::document::{Document, Pose};
use crate::physics::{self, Body};
use crate::quat::{self, Quat, Vec3};
use crate::weight;

type R<T> = Result<T, String>;

/// Most frames one measurement covers.
pub const MAX_FRAMES: i64 = 20_000;
/// A planted foot sliding more than this (cm) shows.
pub const SLIDE_CM: f64 = 2.0;
/// Below the ground by less than this (m) is a touch, not a penetration.
pub const PENETRATION_SLACK: f64 = 0.001;
/// Past a joint limit by less than this (degrees) is not counted (the limits are the 0.5–99.5 %
/// range of real poses).
pub const LIMIT_SLACK_DEG: f64 = 1.0;
/// A velocity change at a key over this (m/s) reads as a pop.
pub const POP_SPEED: f64 = 1.0;

/// A run of frames a foot is planted on, and how far it slid.
#[derive(Debug, Clone, PartialEq)]
pub struct Slide {
    pub foot: &'static str,
    pub from: i64,
    pub to: i64,
    pub slide_cm: f64,
}

/// Frames with a problem, the worst of them and how bad (cm or degrees), and what it was on.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Spans {
    pub frames: Vec<i64>,
    pub worst: f64,
    pub worst_frame: Option<i64>,
    /// The joint (penetration) or bone (limits) at the worst frame.
    pub at: String,
    /// Balance: the floor offset `[dx, dy]` (m) that would bring the centre of mass inside at the
    /// worst frame.
    pub fix: Option<[f64; 2]>,
    /// Limits: the bone's rotation clamped into its range at the worst frame.
    pub clamped: Option<Quat>,
    /// Penetration: every bone below the ground at the worst frame and how deep (m), deepest
    /// first (a toe or heel counts as its foot).
    pub below: Vec<(String, f64)>,
}

impl Spans {
    fn add(&mut self, frame: i64, amount: f64, at: &str) -> bool {
        self.frames.push(frame);
        if self.worst_frame.is_none() || amount > self.worst {
            self.worst = amount;
            self.worst_frame = Some(frame);
            self.at = at.to_string();
            return true;
        }
        false
    }

    /// The frames as `[first, last]` runs.
    pub fn runs(&self) -> Vec<[i64; 2]> {
        let mut out: Vec<[i64; 2]> = Vec::new();
        for &f in &self.frames {
            match out.last_mut() {
                Some(r) if r[1] + 1 == f => r[1] = f,
                _ => out.push([f, f]),
            }
        }
        out
    }
}

/// The largest velocity change at a key.
#[derive(Debug, Clone, PartialEq)]
pub struct Pop {
    pub frame: i64,
    pub dv: f64,
    pub joint: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Metrics {
    pub from: i64,
    pub to: i64,
    pub shown: bool,
    pub sliding: Vec<Slide>,
    pub penetration: Spans,
    pub balance: Spans,
    /// Frames a moving body's capture point is outside the feet it has or is about to have.
    pub capture: Spans,
    /// Keys where a body joint's change of speed stands out from the frames near it.
    pub spikes: Vec<Pop>,
    /// Frames out of balance while moving (not counted as problems).
    pub moving_outside: usize,
    /// Per bone past its limits; `None`: no limits loaded.
    pub limits: Option<Vec<Spans>>,
    pub keys: Vec<Pop>,
    pub notes: Vec<String>,
}

/// One posed frame: joint heads, foot points (and their feet), centre of mass.
struct Frame {
    pos: Vec<Vec3>,
    feet: Vec<(usize, Vec3)>,
    com: Vec3,
}

fn ground_height(doc: &Document, p: Vec3) -> f64 {
    p[2] - physics::ground(&doc.clip.surfaces, p[0], p[1])
}

/// Measure frames `from..=to` (`None`: the whole clip) of the motion shown, or of the keys alone.
pub fn measure(doc: &Document, from: Option<i64>, to: Option<i64>, shown: bool) -> R<Metrics> {
    if doc.clip.base().keys.is_empty() {
        return Err(crate::error::hinted(
            "the animation has no keys to measure",
            "pose the body and run key.set, or open one with clip.open",
        ));
    }
    let last = (doc.clip.length_with(&doc.links) - 1).max(0);
    let (a, b) = (from.unwrap_or(0), to.unwrap_or(last));
    if a < 0 || b < a {
        return Err(format!("the range {a}..{b} is empty"));
    }
    if b > last {
        return Err(crate::error::hinted(
            format!("frame {b} is past the end of the animation (its last frame is {last})"),
            format!("measure up to frame {last}"),
        ));
    }
    if b - a + 1 > MAX_FRAMES {
        return Err(format!("{} frames is more than {MAX_FRAMES} at once", b - a + 1));
    }
    // one frame either side, for speeds and velocity changes at the ends
    let (lo, hi) = ((a - 1).max(0), (b + 1).min(last));
    // and more, for a pop against the frames near it
    let (wide_lo, wide_hi) = ((a - 3).max(0), (b + 3).min(last));
    let fr: Vec<f64> = (wide_lo..=wide_hi).map(|f| f as f64).collect();
    let wide_poses: Vec<Pose> = if shown {
        doc.shown_poses(&fr)?
    } else {
        let (r, o) = doc.clip.sample_with(&fr, &doc.links)?;
        r.into_iter()
            .zip(o)
            .map(|(rotations, root)| Pose { rotations, root })
            .collect()
    };
    let sk = &doc.skeleton;
    let body = Body::new(sk)?;
    let wide = wide_poses
        .iter()
        .map(|p| {
            let (world, pos) = sk.fk(&p.rotations, p.root).map_err(|e| e.to_string())?;
            let pts = body.points(&world, &pos);
            let feet = pts
                .iter()
                .enumerate()
                .map(|(k, q)| (body.foot_of(k), *q))
                .collect();
            let com = body.com(&pos);
            Ok(Frame { pos, feet, com })
        })
        .collect::<R<Vec<Frame>>>()?;
    let (skip, take) = ((lo - wide_lo) as usize, (hi - lo + 1) as usize);
    let frames = wide.get(skip..skip + take).unwrap_or(&wide);
    let poses = wide_poses.get(skip..skip + take).unwrap_or(&wide_poses);
    let fps = doc.clip.fps;
    let at = |f: i64| frames.get((f - lo) as usize);
    let mut m = Metrics {
        from: a,
        to: b,
        shown,
        sliding: sliding(doc, frames, lo, a, b, fps),
        penetration: Spans::default(),
        balance: Spans::default(),
        capture: Spans::default(),
        moving_outside: 0,
        limits: None,
        keys: Vec::new(),
        spikes: Vec::new(),
        notes: Vec::new(),
    };
    // the physics pass's own test of a body standing
    let feet: Vec<Vec<Vec3>> = frames
        .iter()
        .map(|fr| fr.feet.iter().map(|(_, p)| *p).collect())
        .collect();
    let coms: Vec<Vec3> = frames.iter().map(|fr| fr.com).collect();
    let stands = physics::standing(&feet, &coms, fps);
    let samples: Vec<weight::Sample> = frames
        .iter()
        .map(|fr| weight::Sample {
            feet: fr.feet.clone(),
            com: fr.com,
            rests: body.rests(&fr.pos, &doc.clip.surfaces),
        })
        .collect();
    let gaps = weight::gaps(&samples, fps, &doc.clip.surfaces);
    for f in a..=b {
        if let Some(Some(g)) = gaps.get((f - lo) as usize) {
            m.capture.add(f, g.outside * 100.0, "");
        }

        let (Some(fr), Some(pose)) = (at(f), poses.get((f - lo) as usize)) else {
            continue;
        };
        penetration(doc, fr, f, &mut m.penetration);
        if let Some(bal) = physics::balance(sk, &body, &pose.rotations, pose.root, &doc.clip.surfaces)?
            && !bal.inside
        {
            if stands.get((f - lo) as usize).copied().unwrap_or(false)
                && !body.rests(&fr.pos, &doc.clip.surfaces)
            {
                let (d, fix) = outside(&bal.support, bal.com);
                if m.balance.add(f, d * 100.0, "") {
                    m.balance.fix = Some(fix);
                }
            } else {
                m.moving_outside += 1;
            }
        }
    }
    match doc.limits.as_deref() {
        Some(l) => m.limits = Some(limits(doc, l, poses, lo, a, b)),
        None => m
            .notes
            .push("joint limits are not loaded (assets/rig/limits.json): limits were not checked".into()),
    }
    m.keys = pops(doc, frames, lo, a, b, fps);
    m.spikes = spikes(doc, &wide, wide_lo, a, b, fps);
    if shown && !doc.physics_shown() {
        m.notes
            .push("the physics pass is off: the motion shown is the keys".into());
    }
    Ok(m)
}

/// Each foot's phase on every frame of what plays (`shown`) or of the keys.
pub fn foot_phases(doc: &Document, shown: bool) -> R<[Vec<crate::contact::Phase>; 2]> {
    let n = doc.clip.length_with(&doc.links).max(0);
    if n > MAX_FRAMES {
        return Err(format!("{n} frames is more than {MAX_FRAMES} at once"));
    }
    let fr: Vec<f64> = (0..n).map(|f| f as f64).collect();
    let poses: Vec<Pose> = if shown {
        doc.shown_poses(&fr)?
    } else {
        let (r, o) = doc.clip.sample_with(&fr, &doc.links)?;
        r.into_iter()
            .zip(o)
            .map(|(rotations, root)| Pose { rotations, root })
            .collect()
    };
    let sk = &doc.skeleton;
    let body = Body::new(sk)?;
    let mut feet = Vec::with_capacity(poses.len());
    for p in &poses {
        let (world, pos) = sk.fk(&p.rotations, p.root).map_err(|e| e.to_string())?;
        let pts = body.points(&world, &pos);
        feet.push(
            pts.into_iter()
                .enumerate()
                .map(|(k, q)| (body.foot_of(k), q))
                .collect(),
        );
    }
    Ok(crate::contact::phases(
        &feet,
        doc.clip.fps,
        &doc.clip.surfaces,
        0.0,
    ))
}

fn sliding(doc: &Document, frames: &[Frame], lo: i64, a: i64, b: i64, fps: f64) -> Vec<Slide> {
    let mut out = Vec::new();
    let tagged: Vec<Vec<(usize, Vec3)>> = frames.iter().map(|fr| fr.feet.clone()).collect();
    let phases = crate::contact::phases(&tagged, fps, &doc.clip.surfaces, 0.0);
    for (foot, name) in [(0usize, "L"), (1, "R")] {
        // per frame: which of this foot's points are planted
        let planted = |f: i64| -> Vec<(usize, Vec3)> {
            let (Some(fr), Some(true)) = (
                frames.get((f - lo) as usize),
                phases[foot]
                    .get((f - lo) as usize)
                    .map(|p| *p == crate::contact::Phase::Planted),
            ) else {
                return Vec::new();
            };
            let prev = if f > 0 {
                frames.get((f - 1 - lo) as usize)
            } else {
                None
            };
            fr.feet
                .iter()
                .enumerate()
                .filter(|(_, (ft, _))| *ft == foot)
                .filter(|(k, (_, p))| {
                    let near = ground_height(doc, *p) < physics::NEAR;
                    let slow = prev
                        .and_then(|q| q.feet.get(*k))
                        .is_none_or(|(_, q)| (p[0] - q[0]).hypot(p[1] - q[1]) * fps < physics::PLANT_SPEED);
                    near && slow
                })
                .map(|(k, (_, p))| (k, *p))
                .collect()
        };
        let mut span: Option<(i64, Vec<f64>)> = None;
        let mut prev: Vec<(usize, Vec3)> = Vec::new();
        for f in a..=b + 1 {
            let now = if f <= b { planted(f) } else { Vec::new() };
            if now.is_empty() {
                if let Some((start, travel)) = span.take() {
                    out.push(Slide {
                        foot: name,
                        from: start,
                        to: f - 1,
                        slide_cm: travel.iter().copied().fold(0.0, f64::max) * 100.0,
                    });
                }
                prev = now;
                continue;
            }
            let (_, travel) = span.get_or_insert_with(|| (f, Vec::new()));
            for (k, p) in &now {
                if let Some((_, q)) = prev.iter().find(|(j, _)| j == k) {
                    let n = travel.len().max(*k + 1);
                    travel.resize(n, 0.0);
                    if let Some(t) = travel.get_mut(*k) {
                        *t += (p[0] - q[0]).hypot(p[1] - q[1]);
                    }
                }
            }
            prev = now;
        }
    }
    out
}

fn penetration(doc: &Document, fr: &Frame, f: i64, out: &mut Spans) {
    let names = &doc.skeleton.names;
    let mut below: Vec<(String, f64)> = Vec::new();
    let joints = fr
        .pos
        .iter()
        .enumerate()
        .map(|(i, p)| (*p, names.get(i).cloned().unwrap_or_default()));
    // heel points carry their foot's ankle name: that is the bone to move
    let heels = fr.feet.iter().map(|(ft, p)| {
        (
            *p,
            if *ft == 0 {
                "foot.L".to_string()
            } else {
                "foot.R".to_string()
            },
        )
    });
    for (p, name) in joints.chain(heels) {
        let depth = -ground_height(doc, p);
        if depth <= PENETRATION_SLACK {
            continue;
        }
        // a toe is moved by its foot
        let bone = match name.rsplit_once('.') {
            Some((head, side)) if head.starts_with("toe") => format!("foot.{side}"),
            _ => name,
        };
        match below.iter_mut().find(|(b, _)| *b == bone) {
            Some((_, d)) => *d = d.max(depth),
            None => below.push((bone, depth)),
        }
    }
    below.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    if let Some((bone, d)) = below.first().cloned()
        && out.add(f, d * 100.0, &bone)
    {
        out.below = below;
    }
}

/// How far (m) `q` is outside a counter-clockwise support hull, and the offset that brings it
/// [`physics::BALANCE_MARGIN`] inside the nearest edge.
fn outside(hull: &[[f64; 2]], q: [f64; 2]) -> (f64, [f64; 2]) {
    let near = |a: [f64; 2], b: [f64; 2]| {
        let d = [b[0] - a[0], b[1] - a[1]];
        let l = d[0] * d[0] + d[1] * d[1];
        let t = if l == 0.0 {
            0.0
        } else {
            (((q[0] - a[0]) * d[0] + (q[1] - a[1]) * d[1]) / l).clamp(0.0, 1.0)
        };
        [a[0] + d[0] * t, a[1] + d[1] * t]
    };
    let n = hull.len();
    let best = (0..n.max(1))
        .filter_map(|i| {
            let a = *hull.get(i)?;
            let b = *hull.get((i + 1) % n.max(1)).unwrap_or(&a);
            Some(near(a, b))
        })
        .min_by(|x, y| {
            (x[0] - q[0])
                .hypot(x[1] - q[1])
                .total_cmp(&(y[0] - q[0]).hypot(y[1] - q[1]))
        })
        .unwrap_or(q);
    let d = [best[0] - q[0], best[1] - q[1]];
    let dist = d[0].hypot(d[1]);
    if dist < 1e-12 {
        return (0.0, [0.0, 0.0]);
    }
    let s = (dist + physics::BALANCE_MARGIN) / dist;
    (dist, [d[0] * s, d[1] * s])
}

/// How far (degrees) a local rotation is past a bone's range: the largest excess on any axis.
pub fn limit_excess(l: &crate::fullbody::Limits, bone: usize, q: Quat) -> f64 {
    let rv = quat::scale3(quat::log(quat::align(q, quat::IDENTITY)), 2.0);
    let (Some(lo), Some(hi)) = (l.lo.get(bone), l.hi.get(bone)) else {
        return 0.0;
    };
    (0..3)
        .map(|k| (lo[k] - rv[k]).max(rv[k] - hi[k]).max(0.0))
        .fold(0.0, f64::max)
        .to_degrees()
}

fn limits(
    doc: &Document,
    l: &crate::fullbody::Limits,
    poses: &[Pose],
    lo: i64,
    a: i64,
    b: i64,
) -> Vec<Spans> {
    let names = &doc.skeleton.names;
    let mut per: Vec<Spans> = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let mut s = Spans::default();
        for f in a..=b {
            let Some(q) = poses.get((f - lo) as usize).and_then(|p| p.rotations.get(i)) else {
                continue;
            };
            let over = limit_excess(l, i, *q);
            if over > LIMIT_SLACK_DEG && s.add(f, over, name) {
                s.clamped = Some(l.clamp(i, *q));
            }
        }
        if !s.frames.is_empty() {
            per.push(s);
        }
    }
    per.sort_by(|x, y| y.worst.total_cmp(&x.worst).then(x.at.cmp(&y.at)));
    per
}

/// Joints whose change of speed at a key is within this (m/s) of each other tie, and the first in
/// rig order (the root first) is the one named: when the whole body jumps, every joint's number is
/// the same but for rounding (a planted foot's IK moves it by nanometres), and "root" says so.
pub const POP_TIE: f64 = 1e-6;

fn pops(doc: &Document, frames: &[Frame], lo: i64, a: i64, b: i64, fps: f64) -> Vec<Pop> {
    let mut keyed: Vec<i64> = doc
        .clip
        .layers
        .iter()
        .flat_map(|l| l.keys.iter().map(|k| k.frame))
        .filter(|f| *f >= a && *f <= b)
        .collect();
    keyed.sort_unstable();
    keyed.dedup();
    let names = &doc.skeleton.names;
    keyed
        .into_iter()
        .filter_map(|k| {
            let p0 = frames.get(usize::try_from(k - 1 - lo).ok()?)?;
            let p1 = frames.get((k - lo) as usize)?;
            let p2 = frames.get((k + 1 - lo) as usize)?;
            let mut best: Option<(f64, usize)> = None;
            for (j, ((x0, x1), x2)) in p0.pos.iter().zip(&p1.pos).zip(&p2.pos).enumerate() {
                let v0 = quat::scale3(quat::sub3(*x1, *x0), fps);
                let v1 = quat::scale3(quat::sub3(*x2, *x1), fps);
                let dv = quat::len3(quat::sub3(v1, v0));
                if best.is_none_or(|(d, _)| dv > d + POP_TIE) {
                    best = Some((dv, j));
                }
            }
            let (dv, j) = best?;
            Some(Pop {
                frame: k,
                dv,
                joint: names.get(j).cloned().unwrap_or_default(),
            })
        })
        .collect()
}

/// The largest change of any bone's spin across a frame (rad/s), and the bone: each bone's angular
/// velocity is its local rotation from the frame before (`prev` to `at`) and to the frame after
/// (`at` to `next`), as rotation vectors times `fps`; the change is the length of their
/// difference, so a spin that reverses counts as much as one that stops. `None`: the poses
/// don't match in size.
pub fn spin_change(prev: &[Quat], at: &[Quat], next: &[Quat], fps: f64) -> Option<(f64, usize)> {
    if prev.len() != at.len() || next.len() != at.len() {
        return None;
    }
    let omega = |from: Quat, to: Quat| {
        quat::scale3(
            quat::log(quat::align(quat::mul(quat::conj(from), to), quat::IDENTITY)),
            2.0 * fps,
        )
    };
    let mut best: Option<(f64, usize)> = None;
    for (i, ((p, q), n)) in prev.iter().zip(at).zip(next).enumerate() {
        let dw = quat::len3(quat::sub3(omega(*q, *n), omega(*p, *q)));
        if best.is_none_or(|(d, _)| dw > d + 1e-12) {
            best = Some((dw, i));
        }
    }
    best
}

/// The joints a person sees as the body: a finger or an eye only moves with what it hangs from.
const BODY_JOINTS: [&str; 13] = [
    "root", "pelvis", "spine", "neck", "head", "clavicle", "shoulder", "upperarm", "lowerarm", "wrist",
    "upperleg", "lowerleg", "foot",
];

/// How much a joint changes speed at frame `at` (m/s).
fn change_at(frames: &[Frame], at: i64, lo: i64, fps: f64, joint: usize) -> Option<f64> {
    let p = |f: i64| {
        let i = usize::try_from(f - lo).ok()?;
        frames.get(i)?.pos.get(joint).copied()
    };
    let (x0, x1, x2) = (p(at - 1)?, p(at)?, p(at + 1)?);
    let v0 = quat::scale3(quat::sub3(x1, x0), fps);
    let v1 = quat::scale3(quat::sub3(x2, x1), fps);
    Some(quat::len3(quat::sub3(v1, v0)))
}

/// What a person sees as a pop: at each key, the body joint whose change of speed most stands out
/// from its own two frames either side. A swinging arm changes speed on every frame of a recording;
/// that isn't one.
fn spikes(doc: &Document, frames: &[Frame], lo: i64, a: i64, b: i64, fps: f64) -> Vec<Pop> {
    let mut keyed: Vec<i64> = doc
        .clip
        .layers
        .iter()
        .flat_map(|l| l.keys.iter().map(|k| k.frame))
        .filter(|f| *f >= a && *f <= b)
        .collect();
    keyed.sort_unstable();
    keyed.dedup();
    let names = &doc.skeleton.names;
    let joints: Vec<usize> = (0..names.len())
        .filter(|&j| {
            names
                .get(j)
                .is_some_and(|n| BODY_JOINTS.iter().any(|b| n.starts_with(b)))
        })
        .collect();
    keyed
        .into_iter()
        .filter_map(|k| {
            let mut best: Option<(f64, usize)> = None;
            for &j in &joints {
                let dv = change_at(frames, k, lo, fps, j)?;
                let near = [k - 2, k + 2].map(|f| change_at(frames, f, lo, fps, j));
                let known: Vec<f64> = near.into_iter().flatten().collect();
                let usual = if known.is_empty() {
                    0.0
                } else {
                    known.iter().sum::<f64>() / known.len() as f64
                };
                let excess = dv - usual;
                if best.is_none_or(|(d, _)| excess > d + 1e-12) {
                    best = Some((excess, j));
                }
            }
            let (dv, j) = best?;
            Some(Pop {
                frame: k,
                dv,
                joint: names.get(j).cloned().unwrap_or_default(),
            })
        })
        .collect()
}

/// Round for a result: centimetres, degrees and m/s to 0.1 mm / 0.001° / 0.1 mm/s.
fn r4(x: f64) -> f64 {
    (x * 1e4).round() / 1e4
}

impl Spans {
    fn json(&self, unit: &str) -> Value {
        let mut v = json!({
            "frames": self.frames.len(),
            "spans": self.runs(),
            "worst_frame": self.worst_frame,
        });
        v[format!("max_{unit}")] = json!(r4(self.worst));
        v
    }
}

impl Metrics {
    /// The result of `clip.metrics`.
    pub fn json(&self) -> Value {
        let mut pen = self.penetration.json("cm");
        pen["joint"] = json!((!self.penetration.at.is_empty()).then_some(&self.penetration.at));
        let mut bal = self.balance.json("cm");
        bal["moving_frames"] = json!(self.moving_outside);
        let limits = self.limits.as_ref().map(|ls| {
            ls.iter()
                .map(|s| {
                    let mut v = s.json("deg");
                    v["bone"] = json!(s.at);
                    v
                })
                .collect::<Vec<_>>()
        });
        json!({
            "from": self.from,
            "to": self.to,
            "frames": self.to - self.from + 1,
            "motion": if self.shown { "shown" } else { "keys" },
            "sliding": self.sliding.iter().map(|s| json!({
                "foot": s.foot, "from": s.from, "to": s.to, "slide_cm": r4(s.slide_cm),
            })).collect::<Vec<_>>(),
            "sliding_max_cm": r4(self.sliding.iter().map(|s| s.slide_cm).fold(0.0, f64::max)),
            "penetration": pen,
            "balance": bal,
            "capture": self.capture.json("cm"),
            "limits": limits,
            "keys": self.keys.iter().map(|p| json!({ "frame": p.frame, "dv": r4(p.dv), "joint": p.joint })).collect::<Vec<_>>(),
            "pops": self.keys.iter().filter(|p| p.dv > POP_SPEED).count(),
            "notes": self.notes,
        })
    }
}
