//! The built-in Walk loop as a gait: a walk between footsteps that moves like the library's
//! Locomotion/Walk (the hand-made loop that walks on the spot), with each foot planted where its
//! footstep put it.
//!
//! The loop is read once into a table (four samples a frame): each ankle's place along the walk and
//! its height, the hips' height and sway, and how far the body has gone (the planted foot slides
//! back as the loop walks on the spot, so its slide is the body's travel). A step plays the loop
//! from one heel strike to the next. The swing foot covers its share of the way to its landing as
//! the loop's foot does, at the loop's height; the planted foot stays put; leg IK puts both there.
//! The first step eases in from the standing key and the last eases out into the end key.
//!
//! Deterministic: the loop ships with the app (assets/library), no randomness.

use crate::clip::Clip;
use crate::quat::{self, Quat, Vec3};
use crate::rig::Skeleton;

type R<T> = Result<T, String>;

const LOOP: &str = include_str!("../../../assets/library/animations/Locomotion/Walk.clip.json");
/// Table samples a frame.
const RES: f64 = 4.0;
const SIDES: [&str; 2] = ["L", "R"];

/// The Walk loop, measured.
pub struct Cycle {
    clip: Clip,
    period: f64,
    /// Heel strike phases (frames into the loop), left then right.
    strike: [f64; 2],
    /// Each ankle along the walk (m, the loop's own on-the-spot frame), left then right.
    fwd: [Vec<f64>; 2],
    /// Each ankle's height (m).
    height: [Vec<f64>; 2],
    /// How far the body has walked since phase 0 (m).
    travel: Vec<f64>,
    /// The hips' height (m) and sway to the figure's left (m, about its mean).
    hips_z: Vec<f64>,
    hips_side: Vec<f64>,
    /// Metres from the back foot to the front one at a heel strike.
    pub step: f64,
    /// Where the hips are at a heel strike: the share of the way from the back foot to the front.
    pub share: f64,
    /// An ankle's height flat on the floor.
    floor: f64,
    /// The way the loop faces (yaw, as [`facing`]).
    yaw: f64,
    /// Frames from one heel strike to the next.
    pub step_frames: f64,
}

/// Yaw (radians) a pose faces, from its hip line: forward is (cos, sin), so a figure facing -Y
/// (its left +X) has -pi/2.
pub fn facing(sk: &Skeleton, joints: &[Vec3]) -> R<f64> {
    let l = joint(sk, joints, "upperleg01.L")?;
    let r = joint(sk, joints, "upperleg01.R")?;
    Ok((r[0] - l[0]).atan2(l[1] - r[1]))
}

fn joint(sk: &Skeleton, joints: &[Vec3], name: &str) -> R<Vec3> {
    sk.bone(name)
        .and_then(|i| joints.get(i).copied())
        .ok_or_else(|| format!("the rig has no {name}"))
}

pub fn qz(a: f64) -> Quat {
    [0.0, 0.0, (a / 2.0).sin(), (a / 2.0).cos()]
}

pub fn wrap(a: f64) -> f64 {
    (a + std::f64::consts::PI).rem_euclid(2.0 * std::f64::consts::PI) - std::f64::consts::PI
}

fn dot2(a: Vec3, b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

impl Cycle {
    /// The loop that ships with the app, measured on this rig.
    pub fn builtin(sk: &Skeleton) -> R<Cycle> {
        let clip = crate::clipfile::load_clip(LOOP).map_err(|e| format!("the built-in Walk loop: {e}"))?;
        if clip.bones != sk.names {
            return Err("the built-in Walk loop is for the default rig: this rig has other bones".into());
        }
        Cycle::measure(sk, clip)
    }

    fn measure(sk: &Skeleton, clip: Clip) -> R<Cycle> {
        let period = clip.length() as f64;
        let n = (period * RES).round() as usize;
        if n < 8 {
            return Err("the Walk loop is too short to walk with".into());
        }
        let phases: Vec<f64> = (0..=n).map(|i| i as f64 / RES).collect();
        let (rots, roots) = clip.sample(&phases)?;
        let mut joints = Vec::with_capacity(rots.len());
        for (r, o) in rots.iter().zip(&roots) {
            joints.push(sk.fk(r, *o).map_err(|e| e.to_string())?.1);
        }
        let first = joints.first().ok_or("the Walk loop has no frames")?;
        let yaw = facing(sk, first)?;
        // the loop's forward and left, flat
        let (fw, left) = ([yaw.cos(), yaw.sin()], [-yaw.sin(), yaw.cos()]);
        let feet = [sk.bone("foot.L"), sk.bone("foot.R")];
        let [Some(fl), Some(fr)] = feet else {
            return Err("the rig has no feet".into());
        };
        let at = |j: &Vec<Vec3>, i: usize| j.get(i).copied().unwrap_or_default();
        let fwd = [
            joints.iter().map(|j| dot2(at(j, fl), fw)).collect::<Vec<_>>(),
            joints.iter().map(|j| dot2(at(j, fr), fw)).collect::<Vec<_>>(),
        ];
        let height = [
            joints.iter().map(|j| at(j, fl)[2]).collect::<Vec<_>>(),
            joints.iter().map(|j| at(j, fr)[2]).collect::<Vec<_>>(),
        ];
        let hips_fwd: Vec<f64> = joints.iter().map(|j| dot2(at(j, 0), fw)).collect();
        let hips_z: Vec<f64> = joints.iter().map(|j| at(j, 0)[2]).collect();
        let side: Vec<f64> = joints.iter().map(|j| dot2(at(j, 0), left)).collect();
        let mean = side[..n].iter().sum::<f64>() / n as f64;
        let hips_side = side.iter().map(|s| s - mean).collect();
        // a foot strikes the floor at its furthest forward
        let strike = [0, 1].map(|s| {
            let v = &fwd[s][..n];
            let i = (0..n).fold(0, |b, i| if v[i] > v[b] { i } else { b });
            i as f64 / RES
        });
        let since = |p: f64, s: usize| (p - strike[s]).rem_euclid(period);
        // the planted foot is the one that struck last; its slide back is the body's travel
        let mut travel = vec![0.0; n + 1];
        for i in 0..n {
            let mid = (i as f64 + 0.5) / RES;
            let st = if since(mid, 0) < since(mid, 1) { 0 } else { 1 };
            travel[i + 1] = travel[i] + fwd[st][i] - fwd[st][i + 1];
        }
        let (mut step, mut share) = (0.0, 0.0);
        for s in 0..2 {
            let i = (strike[s] * RES).round() as usize;
            let (front, back) = (fwd[s][i], fwd[1 - s][i]);
            let gap = front - back;
            if gap <= 0.0 {
                return Err("the Walk loop's feet don't pass each other".into());
            }
            step += gap / 2.0;
            share += (hips_fwd[i] - back) / gap / 2.0;
        }
        let floor = height[0][..n]
            .iter()
            .chain(&height[1][..n])
            .fold(f64::INFINITY, |m, &z| m.min(z));
        let step_frames = (strike[1] - strike[0]).rem_euclid(period);
        Ok(Cycle {
            clip,
            period,
            strike,
            fwd,
            height,
            travel,
            hips_z,
            hips_side,
            step,
            share,
            floor,
            yaw,
            step_frames,
        })
    }

    /// A table's value at phase `p` (any frame: the loop repeats).
    fn at(&self, table: &[f64], p: f64) -> f64 {
        let q = p.rem_euclid(self.period) * RES;
        let i = (q.floor() as usize).min(table.len().saturating_sub(2));
        let u = q - i as f64;
        let (a, b) = (
            table.get(i).copied().unwrap_or_default(),
            table.get(i + 1).copied().unwrap_or_default(),
        );
        a + (b - a) * u
    }

    /// The body's travel at phase `p`, counting whole loops.
    fn walked(&self, p: f64) -> f64 {
        let total = self.travel.last().copied().unwrap_or_default();
        self.at(&self.travel, p) + (p / self.period).floor() * total
    }

    /// Where foot `s` is along the ground at phase `p` (it stays put while planted).
    fn ground(&self, s: usize, p: f64) -> f64 {
        self.at(&self.fwd[s], p) + self.walked(p)
    }
}

/// The walk to make: what the two keys hold and where the feet go.
pub struct Walk<'a> {
    pub sk: &'a Skeleton,
    pub a: i64,
    pub b: i64,
    pub ra: &'a [Quat],
    pub rb: &'a [Quat],
    pub oa: Vec3,
    pub ob: Vec3,
    /// Footfall frames: `a`, then each landing, then `b` (n + 1 frames for n steps).
    pub contacts: Vec<i64>,
    /// Per step: the foot that swings (0 left, 1 right), where it lifts off and where it lands.
    pub swings: Vec<(usize, Vec3, Vec3)>,
    /// Per step: where the other foot stands.
    pub stands: Vec<Vec3>,
    /// Hip to ankle, the most a leg reaches (m).
    pub reach: f64,
}

/// What [`walk`] made: a pose for every frame strictly between the keys, and how much the hips came
/// down so long strides reach.
pub struct Walked {
    pub poses: Vec<(i64, Vec<Quat>, Vec3)>,
    pub drop: f64,
}

fn ease(u: f64) -> f64 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// A slope through a point from the spans either side (flat at a turn, as smooth keys are).
fn slope(d_in: f64, d_out: f64, h_in: f64, h_out: f64) -> f64 {
    if h_in <= 0.0 || h_out <= 0.0 {
        return 0.0;
    }
    let (s_in, s_out) = (d_in / h_in, d_out / h_out);
    if s_in * s_out <= 0.0 {
        return 0.0;
    }
    let limit = 3.0 * s_in.abs().min(s_out.abs());
    ((d_in + d_out) / (h_in + h_out)).clamp(-limit, limit)
}

struct Frame {
    t: i64,
    rots: Vec<Quat>,
    hips: Vec3,
    w: f64,
    targets: [(usize, Vec3); 2],
}

/// The walk, a pose a frame.
pub fn walk(c: &Cycle, w: &Walk) -> R<Walked> {
    let sk = w.sk;
    let n = w.swings.len();
    if n == 0 || w.contacts.len() != n + 1 || w.stands.len() != n {
        return Err("a walk needs a footfall frame for each step".into());
    }
    if w.contacts.windows(2).any(|p| p[1] <= p[0]) {
        return Err("the footfalls must come one after another".into());
    }
    let (_, ja) = sk.fk(w.ra, w.oa).map_err(|e| e.to_string())?;
    let (_, jb) = sk.fk(w.rb, w.ob).map_err(|e| e.to_string())?;
    let (ya, yb) = (facing(sk, &ja)?, facing(sk, &jb)?);
    let (ha, hb) = (
        ja.first().copied().unwrap_or_default(),
        jb.first().copied().unwrap_or_default(),
    );
    let flat = [hb[0] - ha[0], hb[1] - ha[1]];
    let dist = (flat[0] * flat[0] + flat[1] * flat[1]).sqrt();
    let dir = if dist > 1e-9 {
        [flat[0] / dist, flat[1] / dist]
    } else {
        [ya.cos(), ya.sin()]
    };
    let left = [-dir[1], dir[0]];
    // the ankles' floor height, as the keys stand
    let feet = [sk.bone("foot.L"), sk.bone("foot.R")];
    let [Some(fl), Some(fr)] = feet else {
        return Err("the rig has no feet".into());
    };
    let z_of = |j: &[Vec3], i: usize| j.get(i).map_or(0.0, |p| p[2]);
    let floor = z_of(&ja, fl)
        .min(z_of(&ja, fr))
        .min(z_of(&jb, fl))
        .min(z_of(&jb, fr));

    // where the hips are along the walk at each footfall: the loop's share of the way from the back
    // foot to the front one; standing still on the two keys
    let mut along = vec![0.0];
    for ((_, _, front), back) in w.swings.iter().zip(&w.stands).take(n - 1) {
        let x = quat::add3(*back, quat::scale3(quat::sub3(*front, *back), c.share));
        along.push(dot2(x, dir) - dot2(ha, dir));
    }
    along.push(dot2(hb, dir) - dot2(ha, dir));
    let speed: Vec<f64> = (0..=n)
        .map(|k| {
            if k == 0 || k == n {
                return 0.0;
            }
            let h_in = (w.contacts[k] - w.contacts[k - 1]) as f64;
            let h_out = (w.contacts[k + 1] - w.contacts[k]) as f64;
            slope(along[k] - along[k - 1], along[k + 1] - along[k], h_in, h_out)
        })
        .collect();

    let span = (w.b - w.a) as f64;
    let mut frames = Vec::new();
    for t in (w.a + 1)..w.b {
        let k = w.contacts.iter().rposition(|&f| f <= t).unwrap_or(0).min(n - 1);
        let (c0, c1) = (w.contacts[k], w.contacts[k + 1]);
        let h = (c1 - c0) as f64;
        let u = (t - c0) as f64 / h;
        let (swing, from, to) = w.swings[k];
        let stand = w.stands[k];
        let other = 1 - swing;
        let p0 = c.strike[other];
        let hc = (c.strike[swing] - p0).rem_euclid(c.period);
        let p = p0 + hc * u;
        // in from the first key over the first step, out into the last over the last
        let mut weight: f64 = 1.0;
        let mut lift: f64 = 1.0;
        if k == 0 {
            weight = weight.min(ease(u));
            lift = lift.min(ease(2.0 * u));
        }
        if k == n - 1 {
            weight = weight.min(ease(1.0 - u));
            lift = lift.min(ease(2.0 * (1.0 - u)));
        }
        let v = (t - w.a) as f64 / span;

        // the loop's pose at this phase, turned to face as the keys do
        let (cr, _) = c.clip.sample(&[p.rem_euclid(c.period)])?;
        let mut cr = cr.into_iter().next().ok_or("the Walk loop gave no pose")?;
        let yaw = ya + wrap(yb - ya) * v;
        if let Some(r0) = cr.first_mut() {
            *r0 = quat::normalize(quat::mul(qz(yaw - c.yaw), *r0));
        }
        let rots: Vec<Quat> =
            w.ra.iter()
                .zip(w.rb)
                .zip(&cr)
                .map(|((x, y), q)| quat::slerp(quat::slerp(*x, *y, v), *q, weight))
                .collect();

        // the hips: along the walk (a smooth curve through the footfalls), with the loop's sway and bob
        let (s0, s1) = (along[k], along[k + 1]);
        let (m0, m1) = (speed[k] * h, speed[k + 1] * h);
        let (u2, u3) = (u * u, u * u * u);
        let s = (2.0 * u3 - 3.0 * u2 + 1.0) * s0
            + (u3 - 2.0 * u2 + u) * m0
            + (-2.0 * u3 + 3.0 * u2) * s1
            + (u3 - u2) * m1;
        let sway = weight * c.at(&c.hips_side, p);
        let key_z = ha[2] + (hb[2] - ha[2]) * v;
        let loop_z = c.at(&c.hips_z, p) - c.floor + floor;
        let hips = [
            ha[0] + s * dir[0] + sway * left[0],
            ha[1] + s * dir[1] + sway * left[1],
            key_z + (loop_z - key_z) * weight,
        ];

        // the feet: the swing foot its share of the way, as high as the loop's; the other planted
        let g0 = c.ground(swing, p0);
        let g1 = c.ground(swing, p0 + hc);
        let share = if (g1 - g0).abs() > 1e-9 {
            ((c.ground(swing, p) - g0) / (g1 - g0)).clamp(0.0, 1.0)
        } else {
            u
        };
        let mut swing_at = quat::add3(from, quat::scale3(quat::sub3(to, from), share));
        swing_at[2] += lift * (c.at(&c.height[swing], p) - c.floor).max(0.0);
        let mut stand_at = stand;
        stand_at[2] += weight * (c.at(&c.height[other], p) - c.floor).max(0.0);
        frames.push(Frame {
            t,
            rots,
            hips,
            w: weight,
            targets: [(swing, swing_at), (other, stand_at)],
        });
    }

    // a stride longer than the loop's needs lower hips: the most any walking frame needs, for all
    let mut drop: f64 = 0.0;
    for f in frames.iter().filter(|f| f.w > 0.5) {
        let (_, j) = sk.fk(&f.rots, offset(sk, f.hips)).map_err(|e| e.to_string())?;
        for (side, at) in f.targets {
            let hip = joint(sk, &j, &format!("upperleg01.{}", SIDES[side]))?;
            let flat = ((hip[0] - at[0]).powi(2) + (hip[1] - at[1]).powi(2)).sqrt();
            let high = hip[2] - at[2];
            if flat < w.reach && (flat * flat + high * high).sqrt() > w.reach {
                drop = drop.max(high - (w.reach * w.reach - flat * flat).sqrt());
            }
        }
    }
    let mut poses = Vec::with_capacity(frames.len());
    for f in frames {
        let mut hips = f.hips;
        hips[2] -= drop * f.w;
        let o = offset(sk, hips);
        let mut r = f.rots;
        for (side, at) in f.targets {
            r = crate::ik::two_bone(sk, &r, o, &format!("leg.{}", SIDES[side]), at, None, None)
                .map_err(|e| e.to_string())?;
        }
        poses.push((f.t, r, o));
    }
    Ok(Walked { poses, drop })
}

/// A root offset (hip heights from the rest root) for the root joint at `hips`.
fn offset(sk: &Skeleton, hips: Vec3) -> Vec3 {
    let rest = sk.heads.first().copied().unwrap_or_default();
    quat::scale3(quat::sub3(hips, rest), 1.0 / sk.hip_height)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn the_walk_loop_measures_as_a_walk() {
        let rig = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/rig/default_rig.json");
        let sk = Skeleton::from_json(&std::fs::read_to_string(rig).unwrap()).unwrap();
        let c = Cycle::builtin(&sk).unwrap();
        // two steps a loop, a step of about 0.42 m, the hips a bit behind the middle at a strike
        assert!((c.step - 0.42).abs() < 0.03, "step {}", c.step);
        assert!((c.step_frames - c.period / 2.0).abs() < 2.0, "{}", c.step_frames);
        assert!(c.share > 0.25 && c.share < 0.5, "share {}", c.share);
        // the body walks two steps a loop
        let total = c.travel.last().copied().unwrap();
        assert!((total - 2.0 * c.step).abs() < 0.05, "travel {total}");
    }
}
