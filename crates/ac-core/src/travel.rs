//! Travel: walking (or running) between two keys by motion matching (after cf-animate's `travel.py`).
//!
//! Every frame of the library's Locomotion clips goes into a database with its future trajectory,
//! facing and feet. A controller walks the path from the first key to the second: every few frames
//! it picks the database frame that best matches where it has to go and how its feet are now,
//! crossfades into it, and moves the body by that frame's own root motion, steered along the path.
//! The end lands exactly on the second key and planted feet are held still with leg IK.
//!
//! Deterministic: clips are taken in id order, ties pick the first, no randomness. Root offsets are
//! in hip heights, joint positions in metres (as in the clips).
//!
//! Not a byte-for-byte port: foot heights use the ankle and ball joints (the reference used points
//! of the rest mesh's sole), which only shifts every height by the same amount.

use std::collections::BTreeMap;

use crate::clip::{Clip, Interp, Key, KeySource, Links};
use crate::ik;
use crate::quat::{self, Quat, Vec3};
use crate::rig::Skeleton;

type R<T> = Result<T, String>;

/// Which Locomotion clips each gait may use (by name prefix; "Walk " leaves out the in-place
/// built-in "Walk" loop, which has no root motion to travel with).
pub const GAITS: &[(&str, &[&str])] = &[
    (
        "auto",
        &[
            "Walk ",
            "Walk in a circle",
            "Small steps",
            "Stop",
            "Side step",
            "Run ",
            "Sprint",
        ],
    ),
    ("walk", &["Walk ", "Walk in a circle", "Small steps", "Stop"]),
    ("run", &["Run ", "Sprint", "Stop"]),
    ("crouch", &["Crouch walk"]),
];
pub const GROUP: &str = "Locomotion";
const HORIZON: [usize; 2] = [10, 20];
const MAX_H: usize = 20;
const EVERY: usize = 10;
const BLEND: usize = 6;
const RELEASE: usize = 4;
const APPROACH: usize = 8;
const NEAR: f64 = 0.05;
const NF: usize = 22;
/// Longest travel (frames): a planned layer holds a key per frame.
pub const MAX_FRAMES: i64 = 3000;

pub fn gait_ok(gait: &str) -> bool {
    GAITS.iter().any(|(g, _)| *g == gait)
}

/// Whether a library id is a clip the gait walks with.
pub fn uses(gait: &str, id: &str) -> bool {
    let names = GAITS.iter().find(|(g, _)| *g == gait).map_or(GAITS[0].1, |g| g.1);
    id.strip_prefix(GROUP)
        .and_then(|r| r.strip_prefix('/'))
        .is_some_and(|n| names.iter().any(|p| n.starts_with(p)))
}

fn idx(sk: &Skeleton, name: &str) -> R<usize> {
    sk.names
        .iter()
        .position(|n| n == name)
        .ok_or_else(|| format!("the rig has no {name}"))
}

/// Yaw (radians) the body faces, from the hip line (+X = the figure's left, faces -Y).
fn facing(p: &[Vec3], l: usize, r: usize) -> f64 {
    let left = [p[l][0] - p[r][0], p[l][1] - p[r][1]];
    (-left[0]).atan2(left[1])
}

/// A 2D vector turned by -yaw: into a frame that faces along yaw.
fn rot2(v: [f64; 2], yaw: f64) -> [f64; 2] {
    let (s, c) = yaw.sin_cos();
    [c * v[0] + s * v[1], -s * v[0] + c * v[1]]
}

fn qz(a: f64) -> Quat {
    [0.0, 0.0, (a / 2.0).sin(), (a / 2.0).cos()]
}

fn wrap(a: f64) -> f64 {
    (a + std::f64::consts::PI).rem_euclid(2.0 * std::f64::consts::PI) - std::f64::consts::PI
}

fn sub2(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn xy(v: Vec3) -> [f64; 2] {
    [v[0], v[1]]
}

/// numpy's default (linear) percentile.
fn percentile(v: &[f64], pct: f64) -> f64 {
    let mut s: Vec<f64> = v.iter().copied().filter(|x| x.is_finite()).collect();
    if s.is_empty() {
        return 0.0;
    }
    s.sort_by(f64::total_cmp);
    let pos = pct / 100.0 * (s.len() - 1) as f64;
    let (lo, hi) = (pos.floor() as usize, pos.ceil() as usize);
    s[lo] + (s[hi] - s[lo]) * (pos - lo as f64)
}

fn median(v: &[f64]) -> f64 {
    percentile(v, 50.0)
}

/// Positions of every bone on every frame.
fn positions(sk: &Skeleton, rots: &[Vec<Quat>], roots: &[Vec3]) -> R<Vec<Vec<Vec3>>> {
    rots.iter()
        .zip(roots)
        .map(|(r, o)| sk.fk(r, *o).map(|x| x.1).map_err(|e| e.to_string()))
        .collect()
}

/// Each foot's lowest point (ankle or ball) per frame: `[side][frame]`.
fn foot_heights(sk: &Skeleton, p: &[Vec<Vec3>]) -> R<[Vec<f64>; 2]> {
    let mut out = [Vec::new(), Vec::new()];
    for (side, s) in ["L", "R"].iter().enumerate() {
        let (f, t) = (idx(sk, &format!("foot.{s}"))?, idx(sk, &format!("toe3-1.{s}"))?);
        out[side] = p.iter().map(|fr| fr[f][2].min(fr[t][2])).collect();
    }
    Ok(out)
}

/// Per-frame root height change (hip heights) that puts planted feet on one floor.
fn floor_shift(sk: &Skeleton, p: &[Vec<Vec3>]) -> R<Vec<f64>> {
    let n = p.len();
    if n < 2 {
        return Ok(vec![0.0; n]);
    }
    let h = foot_heights(sk, p)?;
    let lows: Vec<f64> = (0..n).map(|k| h[0][k].min(h[1][k])).collect();
    let ground = percentile(&lows, 20.0);
    let low: Vec<f64> = (0..n)
        .map(|k| {
            [h[0][k], h[1][k]]
                .into_iter()
                .filter(|&x| x - ground < NEAR)
                .fold(f64::INFINITY, f64::min)
        })
        .collect();
    let on: Vec<usize> = (0..n).filter(|&k| low[k].is_finite()).collect();
    if on.is_empty() {
        return Ok(vec![0.0; n]);
    }
    let floor = median(&on.iter().map(|&k| low[k]).collect::<Vec<_>>());
    // flight: from take-off to landing; the ends hold
    let mut out = vec![0.0; n];
    for (k, o) in out.iter_mut().enumerate() {
        let after = on.iter().position(|&j| j >= k);
        let v = match after {
            Some(0) => floor - low[on[0]],
            None => floor - low[on[on.len() - 1]],
            Some(i) if on[i] == k => floor - low[k],
            Some(i) => {
                let (a, b) = (on[i - 1], on[i]);
                let t = (k - a) as f64 / (b - a) as f64;
                (floor - low[a]) * (1.0 - t) + (floor - low[b]) * t
            }
        };
        *o = v / sk.hip_height;
    }
    Ok(out)
}

struct Entry {
    rots: Vec<Vec<Quat>>,
    roots: Vec<Vec3>,
    yaws: Vec<f64>,
    contact: Vec<[bool; 2]>,
}

/// Every usable frame of some clips with its matching features.
pub struct Database {
    clips: Vec<Entry>,
    where_: Vec<(usize, usize)>,
    raw: Vec<[f64; NF]>,
    norm: Vec<[f64; NF]>,
    mean: [f64; NF],
    std: [f64; NF],
    w: [f64; NF],
}

impl Database {
    pub fn new(sk: &Skeleton, clips: &[(String, Clip)]) -> R<Database> {
        let (ul, ur) = (idx(sk, "upperleg01.L")?, idx(sk, "upperleg01.R")?);
        let fi = [idx(sk, "foot.L")?, idx(sk, "foot.R")?];
        let mut sorted: Vec<&(String, Clip)> = clips.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        let mut entries = Vec::new();
        let mut where_ = Vec::new();
        let mut raw = Vec::new();
        for (_, c) in sorted {
            let n = c.length().max(0) as usize;
            if n < MAX_H + 3 {
                continue;
            }
            let frames: Vec<f64> = (0..n).map(|f| f as f64).collect();
            let (r, o) = c.sample(&frames)?;
            let p = positions(sk, &r, &o)?;
            // unwrapped facing
            let mut yaw: Vec<f64> = p.iter().map(|fr| facing(fr, ul, ur)).collect();
            for k in 1..n {
                yaw[k] = yaw[k - 1] + wrap(yaw[k] - yaw[k - 1]);
            }
            // the mocap's own foot contacts: low and nearly still
            let mut contact = vec![[false; 2]; n];
            for (side, &i) in fi.iter().enumerate() {
                let zmin = p.iter().map(|fr| fr[i][2]).fold(f64::INFINITY, f64::min);
                for k in 0..n {
                    let v = if k == 0 {
                        0.0
                    } else {
                        quat::len3(quat::sub3(p[k][i], p[k - 1][i]))
                    };
                    contact[k][side] = p[k][i][2] - zmin < 0.06 && v < 0.01;
                }
            }
            let ci = entries.len();
            for f in 1..n - MAX_H {
                let y = yaw[f];
                let mut v = Vec::with_capacity(NF);
                for h in HORIZON {
                    v.extend(rot2(sub2(xy(o[f + h]), xy(o[f])), y));
                }
                for h in HORIZON {
                    v.extend(rot2([yaw[f + h].cos(), yaw[f + h].sin()], y));
                }
                for &i in &fi {
                    v.extend(rot2(sub2(xy(p[f][i]), xy(o[f])), y));
                    v.push(p[f][i][2]);
                }
                for &i in &fi {
                    let d = quat::sub3(p[f + 1][i], p[f - 1][i]);
                    v.extend(rot2([d[0] / 2.0, d[1] / 2.0], y));
                    v.push(d[2] / 2.0);
                }
                let d = sub2(xy(o[f + 1]), xy(o[f - 1]));
                v.extend(rot2([d[0] / 2.0, d[1] / 2.0], y));
                let mut a = [0.0; NF];
                a.copy_from_slice(&v);
                raw.push(a);
                where_.push((ci, f));
            }
            entries.push(Entry {
                rots: r,
                roots: o,
                yaws: yaw,
                contact,
            });
        }
        if raw.is_empty() {
            return Err("no Locomotion clips (of at least 23 frames) to walk with".into());
        }
        let m = raw.len() as f64;
        let mut mean = [0.0; NF];
        let mut std = [0.0; NF];
        for j in 0..NF {
            mean[j] = raw.iter().map(|r| r[j]).sum::<f64>() / m;
            std[j] = (raw.iter().map(|r| (r[j] - mean[j]).powi(2)).sum::<f64>() / m).sqrt() + 1e-6;
        }
        let mut w = [0.0; NF];
        for (j, x) in w.iter_mut().enumerate() {
            *x = match j {
                0..4 => 1.0,
                4..8 => 0.6,
                8..14 => 0.75,
                14..20 => 0.5,
                _ => 1.0,
            };
        }
        let norm = raw
            .iter()
            .map(|r| {
                let mut a = [0.0; NF];
                for j in 0..NF {
                    a[j] = (r[j] - mean[j]) / std[j] * w[j];
                }
                a
            })
            .collect();
        Ok(Database {
            clips: entries,
            where_,
            raw,
            norm,
            mean,
            std,
            w,
        })
    }

    /// (clip, frame) of the best match; the first of equal costs.
    fn search(&self, q: &[f64; NF]) -> (usize, usize) {
        let mut qn = [0.0; NF];
        for j in 0..NF {
            qn[j] = (q[j] - self.mean[j]) / self.std[j] * self.w[j];
        }
        let mut best = (f64::INFINITY, 0);
        for (i, row) in self.norm.iter().enumerate() {
            let d: f64 = row.iter().zip(&qn).map(|(a, b)| (a - b) * (a - b)).sum();
            if d < best.0 {
                best = (d, i);
            }
        }
        self.where_[best.1]
    }

    fn features_at(&self, c: usize, f: usize) -> Option<&[f64; NF]> {
        self.where_
            .iter()
            .position(|&w| w == (c, f))
            .map(|i| &self.raw[i])
    }
}

/// A polyline on the floor, walked by arc length.
struct Route {
    p: Vec<[f64; 2]>,
    cum: Vec<f64>,
    length: f64,
}

impl Route {
    fn new(p: Vec<[f64; 2]>) -> Route {
        let mut cum = vec![0.0];
        for w in p.windows(2) {
            let d = sub2(w[1], w[0]);
            cum.push(cum[cum.len() - 1] + d[0].hypot(d[1]));
        }
        let length = cum[cum.len() - 1];
        Route { p, cum, length }
    }

    fn at(&self, s: f64) -> [f64; 2] {
        if self.length < 1e-9 {
            return self.p[0];
        }
        let s = s.clamp(0.0, self.length);
        let i = self
            .cum
            .iter()
            .rposition(|&c| c <= s)
            .unwrap_or(0)
            .min(self.p.len() - 2);
        let t = (s - self.cum[i]) / (self.cum[i + 1] - self.cum[i]).max(1e-12);
        let (a, b) = (self.p[i], self.p[i + 1]);
        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
    }

    fn heading(&self, s: f64) -> f64 {
        let d = sub2(self.at(s + 0.05), self.at(s - 0.05));
        d[1].atan2(d[0])
    }
}

type Planned = (Vec<Vec<Quat>>, Vec<Vec3>, Vec<[bool; 2]>);

/// Rotations, roots and planted feet for `frames + 1` frames walking from `start` to `end`
/// through `waypoints` (root units).
fn plan(
    db: &Database,
    sk: &Skeleton,
    start: (&[Quat], Vec3),
    end: (&[Quat], Vec3),
    frames: usize,
    waypoints: &[[f64; 2]],
) -> R<Planned> {
    let (ul, ur) = (idx(sk, "upperleg01.L")?, idx(sk, "upperleg01.R")?);
    let fi = [idx(sk, "foot.L")?, idx(sk, "foot.R")?];
    let (r0, o0) = start;
    let (_, o1) = end;
    let p0 = sk.fk(r0, o0).map_err(|e| e.to_string())?.1;
    let p1 = sk.fk(end.0, o1).map_err(|e| e.to_string())?.1;
    let (sp, goal) = (xy(o0), xy(o1));
    let mut pts = vec![sp];
    pts.extend_from_slice(waypoints);
    pts.push(goal);
    let route = Route::new(pts);
    let dist = route.length;
    let fr = frames.max(1) as f64;
    let face0 = facing(&p0, ul, ur);
    let face1 = facing(&p1, ul, ur);
    let heading_at = |t: usize| {
        if dist > 1e-6 {
            route.heading(route.length * (t as f64 / fr).min(1.0))
        } else {
            face0
        }
    };
    let query = |pos: [f64; 2], theta: f64, t: usize, cur: Option<&[f64; NF]>| -> [f64; NF] {
        let left = frames.saturating_sub(t);
        let mut v = Vec::with_capacity(NF);
        for h in HORIZON {
            let k = h.min(left);
            let want = if dist > 1e-6 {
                route.at(route.length * ((t + k) as f64 / fr).min(1.0))
            } else {
                sp
            };
            v.extend(rot2(sub2(want, pos), theta));
        }
        for h in HORIZON {
            let a = if left > 0 {
                heading_at(t + h.min(left))
            } else {
                face1
            };
            v.extend(rot2([a.cos(), a.sin()], theta));
        }
        match cur {
            Some(f) => v.extend_from_slice(&f[8..]),
            None => {
                for &i in &fi {
                    v.extend(rot2(sub2(xy(p0[i]), sp), theta));
                    v.push(p0[i][2]);
                }
                v.extend([0.0; 8]);
            }
        }
        let mut a = [0.0; NF];
        a.copy_from_slice(&v);
        a
    };

    let n = frames + 1;
    let nb = r0.len();
    let mut rots: Vec<Vec<Quat>> = Vec::with_capacity(n);
    let mut roots: Vec<Vec3> = Vec::with_capacity(n);
    let mut planted = Vec::with_capacity(n);
    let mut theta = face0;
    let mut pos = sp;
    let (mut c, mut f) = db.search(&query(pos, theta, 0, None));
    let mut offset: Option<Vec<Quat>> = None;
    let mut zoff = 0.0;
    let mut fade = 0usize;
    for t in 0..n {
        let len_c = db.clips[c].rots.len();
        if (t > 0 && t % EVERY == 0) || f + 1 >= len_c - MAX_H {
            let here = if f < len_c - MAX_H {
                db.features_at(c, f)
            } else {
                None
            };
            let (c2, f2) = db.search(&query(pos, theta, t, here));
            if (c2, f2) != (c, f) && !(c2 == c && f2.abs_diff(f) <= 2) {
                let e = &db.clips[c2];
                let mut new = e.rots[f2].clone();
                new[0] = quat::mul(qz(theta - e.yaws[f2]), new[0]);
                if t > 0 {
                    let prev = &rots[t - 1];
                    offset = Some(
                        (0..nb)
                            .map(|b| quat::normalize(quat::mul(prev[b], quat::conj(new[b]))))
                            .collect(),
                    );
                    zoff = roots[t - 1][2] - e.roots[f2][2];
                    fade = BLEND;
                } else {
                    offset = None;
                    zoff = 0.0;
                    fade = 0;
                }
                c = c2;
                f = f2;
            }
        }
        let e = &db.clips[c];
        let mut pose = e.rots[f].clone();
        pose[0] = quat::mul(qz(theta - e.yaws[f]), pose[0]);
        let mut z = e.roots[f][2];
        if fade > 0
            && let Some(off) = &offset
        {
            let w = fade as f64 / BLEND as f64;
            let w = w * w * (3.0 - 2.0 * w);
            for b in 0..nb {
                pose[b] = quat::normalize(quat::mul(quat::slerp(quat::IDENTITY, off[b], w), pose[b]));
            }
            z += zoff * w;
            fade -= 1;
        }
        rots.push(pose);
        roots.push([pos[0], pos[1], z]);
        planted.push(e.contact[f]);
        if f + 1 < e.rots.len() {
            let step = rot2(sub2(xy(e.roots[f + 1]), xy(e.roots[f])), e.yaws[f]);
            let (st, ct) = theta.sin_cos();
            pos = [
                pos[0] + ct * step[0] - st * step[1],
                pos[1] + st * step[0] + ct * step[1],
            ];
            theta += wrap(e.yaws[f + 1] - e.yaws[f]);
            if dist > 1e-6 && t + EVERY < frames {
                theta += 0.08 * wrap(heading_at(t + EVERY) - theta);
            }
            f += 1;
        }
    }
    // land exactly on the end key: spread the remaining error over the walk
    let last = roots[n - 1];
    let err = sub2(goal, xy(last));
    for (k, o) in roots.iter_mut().enumerate() {
        let s = if n > 1 { k as f64 / (n - 1) as f64 } else { 1.0 };
        o[0] += err[0] * s;
        o[1] += err[1] * s;
    }
    // keep to the path: pull the hips nearly onto it (smoothed over 9 frames)
    if dist > 1e-6 && route.p.len() > 2 {
        let dense: Vec<[f64; 2]> = (0..200)
            .map(|i| route.at(route.length * i as f64 / 199.0))
            .collect();
        let dev: Vec<[f64; 2]> = roots
            .iter()
            .map(|o| {
                let q = xy(*o);
                let near = dense
                    .iter()
                    .copied()
                    .min_by(|a, b| {
                        let (da, db_) = (sub2(*a, q), sub2(*b, q));
                        (da[0] * da[0] + da[1] * da[1]).total_cmp(&(db_[0] * db_[0] + db_[1] * db_[1]))
                    })
                    .unwrap_or(q);
                sub2(near, q)
            })
            .collect();
        for (k, o) in roots.iter_mut().enumerate() {
            let mut s = [0.0; 2];
            for j in 0..9 {
                let i = (k + j).saturating_sub(4).min(n - 1);
                s[0] += dev[i][0] / 9.0;
                s[1] += dev[i][1] / 9.0;
            }
            o[0] += 0.9 * s[0];
            o[1] += 0.9 * s[1];
        }
    }
    // and on the keys' own height: the start's offset easing into the end's
    let (z0, z1) = (roots[0][2], roots[n - 1][2]);
    for (k, o) in roots.iter_mut().enumerate() {
        let s = if n > 1 { k as f64 / (n - 1) as f64 } else { 1.0 };
        o[2] += (o0[2] - z0) * (1.0 - s) + (o1[2] - z1) * s;
    }
    Ok((rots, roots, planted))
}

/// A footstep: side ("L"/"R"), first and last frame, where it is planted.
pub type Step = (String, i64, i64, Vec3);

/// Hold each foot still through the frames it is planted, with leg IK; `moves` {"L0": [dx, dy]}
/// shift footsteps. Returns the rotations and the footsteps.
fn hold_feet(
    sk: &Skeleton,
    mut rots: Vec<Vec<Quat>>,
    roots: &[Vec3],
    planted: &[[bool; 2]],
    moves: &BTreeMap<String, [f64; 2]>,
) -> R<(Vec<Vec<Quat>>, Vec<Step>)> {
    const GAP: usize = 3;
    let p = positions(sk, &rots, roots)?;
    let n = rots.len();
    let mut steps: Vec<Step> = Vec::new();
    for (side, s) in ["L", "R"].iter().enumerate() {
        let i = idx(sk, &format!("foot.{s}"))?;
        let knee = idx(sk, &format!("lowerleg01.{s}"))?;
        let mut on: Vec<bool> = planted.iter().map(|x| x[side]).collect();
        // fill short breaks in a contact
        let mut f = 0;
        while f < n {
            if on[f] {
                let mut g = f + 1;
                while g < n && !on[g] {
                    g += 1;
                }
                if g < n && g - f - 1 > 0 && g - f - 1 <= GAP {
                    on[f + 1..g].iter_mut().for_each(|x| *x = true);
                }
                f = g;
            } else {
                f += 1;
            }
        }
        let mut spans = Vec::new();
        let mut f = 0;
        while f < n {
            if on[f] {
                let mut g = f;
                while g < n && on[g] {
                    g += 1;
                }
                spans.push((f, g));
                f = g;
            } else {
                f += 1;
            }
        }
        let mut shift = vec![[0.0f64; 3]; n];
        for (k, &(f, g)) in spans.iter().enumerate() {
            let Some(d) = moves.get(&format!("{s}{k}")) else {
                continue;
            };
            let d = [d[0], d[1], 0.0];
            shift[f..g].iter_mut().for_each(|x| *x = d);
            for j in 1..=APPROACH {
                if j > f || on[f - j] {
                    break;
                }
                let w = 1.0 - j as f64 / (APPROACH + 1) as f64;
                let w = w * w * (3.0 - 2.0 * w);
                shift[f - j] = [d[0] * w, d[1] * w, 0.0];
            }
        }
        let mut pin: Option<Vec3> = None;
        let mut prev: Option<Vec3> = None;
        let mut release: Option<(Vec3, usize, usize)> = None;
        let mut k_span = 0usize;
        for k in 0..n {
            let raw = quat::add3(p[k][i], shift[k]);
            let mut target = None;
            if on[k] {
                let here = match pin {
                    Some(x) => x,
                    None => {
                        let x = prev.unwrap_or(raw);
                        release = None;
                        if let Some(&(f0, g0)) = spans.get(k_span) {
                            steps.push((s.to_string(), f0 as i64, g0 as i64 - 1, x));
                        }
                        k_span += 1;
                        pin = Some(x);
                        x
                    }
                };
                target = Some(here);
            } else {
                if let Some(pn) = pin.take() {
                    let far = quat::len3(quat::sub3(raw, pn));
                    release = Some((pn, ((far / 0.06).ceil() as usize).clamp(RELEASE, 15), 0));
                }
                if let Some((frm, total, done)) = release {
                    let done = done + 1;
                    if done > total {
                        release = None;
                    } else {
                        let w = done as f64 / (total + 1) as f64;
                        let w = w * w * (3.0 - 2.0 * w);
                        target = Some(quat::add3(frm, quat::scale3(quat::sub3(raw, frm), w)));
                        release = Some((frm, total, done));
                    }
                }
                if target.is_none() && shift[k] != [0.0; 3] {
                    target = Some(raw);
                }
            }
            match target {
                Some(t) => {
                    if let Ok(r) = ik::two_bone(
                        sk,
                        &rots[k],
                        roots[k],
                        &format!("leg.{s}"),
                        t,
                        Some(p[k][knee]),
                        None,
                    ) {
                        rots[k] = r;
                    }
                    prev = Some(t);
                }
                None => prev = Some(raw),
            }
        }
    }
    steps.sort_by(|a, b| (a.1, &a.0).cmp(&(b.1, &b.0)));
    Ok((rots, steps))
}

/// The keys (AI, model "travel", one per frame a..=b) and footsteps of the walk from the pose at
/// `a` to the pose at `b` of the layers under `below`, through `waypoints` (metres on the floor).
#[allow(clippy::too_many_arguments)]
pub fn travel_keys(
    sk: &Skeleton,
    clip: &Clip,
    a: i64,
    b: i64,
    below: usize,
    links: &dyn Links,
    db: &Database,
    waypoints: &[[f64; 2]],
    moves: &BTreeMap<String, [f64; 2]>,
) -> R<(Vec<Key>, Vec<Step>)> {
    if b - a < 2 {
        return Err("travel needs at least 2 frames between the keys".into());
    }
    if b - a > MAX_FRAMES {
        return Err(format!("travel is limited to {MAX_FRAMES} frames"));
    }
    let (r, o) = clip.sample_below(&[a as f64, b as f64], below, links)?;
    let wp: Vec<[f64; 2]> = waypoints
        .iter()
        .map(|w| [w[0] / sk.hip_height, w[1] / sk.hip_height])
        .collect();
    let (rots, mut roots, planted) = plan(db, sk, (&r[0], o[0]), (&r[1], o[1]), (b - a) as usize, &wp)?;
    // the mocap's hips sit at its actors' height: planted feet on one level, the start key's floor
    let p = positions(sk, &rots, &roots)?;
    for (o, s) in roots.iter_mut().zip(floor_shift(sk, &p)?) {
        o[2] += s;
    }
    let p = positions(sk, &rots, &roots)?;
    let h = foot_heights(sk, &p)?;
    let lows: Vec<f64> = (0..p.len()).map(|k| h[0][k].min(h[1][k])).collect();
    let g = percentile(&lows, 20.0);
    let level = median(&lows.iter().copied().filter(|x| x - g < NEAR).collect::<Vec<_>>());
    let kp = sk.fk(&r[0], o[0]).map_err(|e| e.to_string())?.1;
    let kh = foot_heights(sk, &[kp])?;
    let key = kh[0][0].min(kh[1][0]);
    for o in roots.iter_mut() {
        o[2] += (key - level) / sk.hip_height;
    }
    let (rots, mut steps) = hold_feet(sk, rots, &roots, &planted, moves)?;
    for st in steps.iter_mut() {
        st.1 += a;
        st.2 += a;
    }
    let keys = rots
        .into_iter()
        .zip(roots)
        .enumerate()
        .map(|(t, (r, o))| {
            let mut k = Key::new(a + t as i64, r, o, Interp::Linear);
            k.source = KeySource::Ai;
            k.model = "travel".into();
            k
        })
        .collect();
    Ok((keys, steps))
}
