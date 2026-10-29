//! The physics pass: a sampled animation made physically plausible. Written fresh (cf-animate's
//! pass had known bugs); every rule is simple enough to check by hand. In order:
//!
//! 1. **Support**: no foot goes below what it stands over (the floor, z = 0, or the top of a box
//!    under it). A frame whose lowest foot point is below is lifted until it touches.
//! 2. **Contact**: a foot point is planted when it is within [`NEAR`] of the ground and moves
//!    (as keyed) slower than [`PLANT_SPEED`] across it; a swinging foot that skims the floor is not.
//!    A frame is on the ground when any foot point is within [`NEAR`] (planted or not).
//! 3. **Fulcrum**: a foot point on the ground does not slide. It is pinned where it touched down
//!    until it lifts; the body moves across the floor so that the pinned points stay (by the mean
//!    of their slips, when several pull different ways). The body travels only by stepping.
//! 4. **Balance**: on a frame on the ground where the body is nearly still (its keyed centre of
//!    mass moves slower than [`STILL_SPEED`]; a walk or a run is balanced by its motion, not
//!    statically), the centre of mass (over the floor) lies inside the support: the convex hull of the foot points on the ground. If it is outside, the hips move
//!    across (the feet held, the legs bending, the rest of the pose kept) until it is
//!    [`BALANCE_MARGIN`] inside the nearest edge.
//! 5. **Flight**: a run of at least [`MIN_FLIGHT`] frames off the ground, between a take-off frame
//!    `a` and a landing frame `b` at most [`MAX_AIR`] seconds apart, follows gravity: the hips
//!    travel at a constant speed across the floor, and their height is the one parabola that leaves
//!    `a` and reaches `b` in that time: `z(t) = z_a + (z_b - z_a) t / T + g t (T - t) / 2`.
//!    The take-off and landing frames are kept exactly; so is every pose in the air.
//!
//! Foot points are the heads of the toe joints and a heel under each ankle. The mass model is
//! de Leva's (1996) segment table. Deterministic, f64.

use crate::clip::Surface;
use crate::fullbody::{self, Goal, Limits};
use crate::quat::{self, Mat3, Quat, Vec3};
use crate::rig::Skeleton;

type R<T> = Result<T, String>;

pub const G: f64 = 9.81;
/// A foot point this close to the ground (m) carries the body.
pub const NEAR: f64 = 0.05;
/// Fewer frames off the ground than this is a step, not a flight.
pub const MIN_FLIGHT: usize = 3;
/// Longer than this in the air (s) is not a jump (a fall from a height, a cut): left alone.
pub const MAX_AIR: f64 = 1.0;
/// Most frames one solve takes.
pub const MAX_FRAMES: usize = 100_000;
/// A foot point moving faster than this (m/s, across the floor) is swinging, not planted.
pub const PLANT_SPEED: f64 = 0.5;
/// A body whose centre of mass moves slower than this (m/s) must stand in static balance.
pub const STILL_SPEED: f64 = 0.2;
/// How far inside the support's edge (m) balance puts the centre of mass.
pub const BALANCE_MARGIN: f64 = 0.01;
/// How far behind the ankle (m, along the foot) the heel point is.
const HEEL_BACK: f64 = 0.05;
/// Balance re-aims the hips this many times (each closes most of what is left).
const BALANCE_ROUNDS: usize = 6;

/// Segment masses (% of the body) and where each segment's centre is: `frac` of the way from
/// one joint's head to another's (de Leva 1996, adult male).
const SEGMENTS: &[(f64, &str, &str, f64)] = &[
    (6.94, "neck01", "head", 1.0),
    (43.46, "root", "neck01", 0.5),
    (2.71, "upperarm01.L", "lowerarm01.L", 0.577),
    (2.71, "upperarm01.R", "lowerarm01.R", 0.577),
    (1.62, "lowerarm01.L", "wrist.L", 0.457),
    (1.62, "lowerarm01.R", "wrist.R", 0.457),
    (0.61, "wrist.L", "wrist.L", 0.0),
    (0.61, "wrist.R", "wrist.R", 0.0),
    (14.16, "upperleg01.L", "lowerleg01.L", 0.41),
    (14.16, "upperleg01.R", "lowerleg01.R", 0.41),
    (4.33, "lowerleg01.L", "foot.L", 0.44),
    (4.33, "lowerleg01.R", "foot.R", 0.44),
    (1.37, "foot.L", "foot.L", 0.0),
    (1.37, "foot.R", "foot.R", 0.0),
];

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// Joint rotations, one list per frame (only balance changes them).
    pub rotations: Vec<Vec<Quat>>,
    /// Root offsets (hip heights), one per frame.
    pub roots: Vec<Vec3>,
    /// (take-off frame, landing frame) of each flight.
    pub flights: Vec<(usize, usize)>,
    /// Frames lifted out of the ground.
    pub lifted: usize,
    /// Frames the fulcrum rule moved (a planted foot had slid more than 0.1 mm).
    pub slid: usize,
    /// Frames that were still and out of balance (and were balanced).
    pub balanced: Vec<usize>,
    /// The frames counted in `lifted`, in order.
    pub lifted_frames: Vec<usize>,
    /// The frames counted in `slid`, in order.
    pub slid_frames: Vec<usize>,
}

/// Height of what is under a floor point: the highest box containing it, else the floor (0).
pub fn ground(surfaces: &[Surface], x: f64, y: f64) -> f64 {
    surfaces
        .iter()
        .filter(|s| s.contains(x, y))
        .map(|s| s.top)
        .fold(0.0, f64::max)
}

/// One foot: its ankle, its toe joints, and the heel's offset from the ankle in the foot's frame.
struct Foot {
    ankle: usize,
    toes: Vec<usize>,
    heel: Vec3,
}

/// The rig's feet and mass model, found once.
pub struct Body {
    feet: Vec<Foot>,
    segments: Vec<(f64, usize, usize, f64)>,
}

impl Body {
    pub fn new(sk: &Skeleton) -> R<Body> {
        let rest = sk
            .fk(&vec![quat::IDENTITY; sk.len()], [0.0; 3])
            .map_err(|e| e.to_string())?
            .1;
        let mut feet = Vec::new();
        for side in ["L", "R"] {
            let ankle = sk
                .bone(&format!("foot.{side}"))
                .ok_or_else(|| format!("the rig has no foot.{side}"))?;
            let toes: Vec<usize> = (0..sk.len())
                .filter(|&i| sk.names[i].starts_with("toe") && sk.names[i].ends_with(&format!(".{side}")))
                .collect();
            if toes.is_empty() {
                return Err(format!("the rig has no toe joints on side {side}"));
            }
            // the rest foot is flat: the heel is as low as the lowest toe, a little behind the ankle
            let low = toes.iter().map(|&t| rest[t][2]).fold(f64::INFINITY, f64::min);
            feet.push(Foot {
                ankle,
                toes,
                heel: [0.0, HEEL_BACK, low - rest[ankle][2]],
            });
        }
        let segments = SEGMENTS
            .iter()
            .map(|&(m, a, b, f)| {
                Ok((
                    m / 100.0,
                    sk.bone(a).ok_or_else(|| format!("the rig has no {a}"))?,
                    sk.bone(b).ok_or_else(|| format!("the rig has no {b}"))?,
                    f,
                ))
            })
            .collect::<R<Vec<_>>>()?;
        Ok(Body { feet, segments })
    }

    /// Foot points (toe heads, then the heel, per foot) from a posed rig.
    pub fn points(&self, world: &[Mat3], pos: &[Vec3]) -> Vec<Vec3> {
        let mut out = Vec::new();
        for f in &self.feet {
            out.extend(f.toes.iter().map(|&t| pos[t]));
            out.push(quat::add3(pos[f.ankle], quat::mat3_vec(&world[f.ankle], f.heel)));
        }
        out
    }

    /// The centre of mass (metres, rig world) of a posed rig.
    pub fn com(&self, pos: &[Vec3]) -> Vec3 {
        let mut c = [0.0; 3];
        let mut total = 0.0;
        for &(m, a, b, f) in &self.segments {
            let p = quat::add3(pos[a], quat::scale3(quat::sub3(pos[b], pos[a]), f));
            c = quat::add3(c, quat::scale3(p, m));
            total += m;
        }
        quat::scale3(c, 1.0 / total)
    }

    /// Joints that hold a foot still: the ankle and the toe joints hung off it.
    fn holds(&self, sk: &Skeleton, foot: usize) -> Vec<usize> {
        let f = &self.feet[foot];
        let mut v = vec![f.ankle];
        v.extend(f.toes.iter().copied().filter(|&t| sk.parents[t] == Some(f.ankle)));
        v
    }

    /// Which foot a point (index into [`Body::points`]) belongs to.
    pub fn foot_of(&self, k: usize) -> usize {
        let mut n = 0;
        for (i, f) in self.feet.iter().enumerate() {
            n += f.toes.len() + 1;
            if k < n {
                return i;
            }
        }
        self.feet.len() - 1
    }
}

/// Balance on one posed frame: the centre of mass over the floor, the support it must lie in,
/// and whether it does (`None`: no foot is on the ground).
pub struct Balance {
    pub com: [f64; 2],
    pub support: Vec<[f64; 2]>,
    pub inside: bool,
}

pub fn balance(
    sk: &Skeleton,
    body: &Body,
    rots: &[Quat],
    root: Vec3,
    surfaces: &[Surface],
) -> R<Option<Balance>> {
    let (world, pos) = sk.fk(rots, root).map_err(|e| e.to_string())?;
    let pts = body.points(&world, &pos);
    let on: Vec<[f64; 2]> = pts
        .iter()
        .filter(|p| p[2] - ground(surfaces, p[0], p[1]) < NEAR)
        .map(|p| [p[0], p[1]])
        .collect();
    if on.is_empty() {
        return Ok(None);
    }
    let hull = hull(&on);
    let c = body.com(&pos);
    let com = [c[0], c[1]];
    let inside = contains(&hull, com, 1e-9);
    Ok(Some(Balance {
        com,
        support: hull,
        inside,
    }))
}

/// What the viewport draws over one frame of the motion shown: the centre of mass, the support
/// area, the foot points touching the ground (pinned where the fulcrum rule holds them), the jump
/// arc the frame is in, and the keyed pose where the physics pass moved the body.
pub struct Overlay {
    pub frame: usize,
    /// Centre of mass (metres, rig world).
    pub com: Vec3,
    pub balance: Option<Balance>,
    /// Foot points on the ground (within [`NEAR`]).
    pub contacts: Vec<Contact>,
    /// The flight (take-off frame, landing frame) the frame is in, and the centre of mass on
    /// every frame of it.
    pub flight: Option<(usize, usize, Vec<Vec3>)>,
    /// How far (metres) the pass moved the root head from the keyed pose: the ghost's offset.
    pub keyed_moved: f64,
}

/// A foot point on the ground.
#[derive(Debug, Clone, PartialEq)]
pub struct Contact {
    pub point: Vec3,
    /// It also moves slower than [`PLANT_SPEED`] since the frame before, so the fulcrum rule
    /// pins it.
    pub pinned: bool,
    /// Which foot: 0 left, 1 right.
    pub foot: usize,
}

/// Contacts and balance of a posed frame; `before`: the same frame's foot points one frame
/// earlier (for the speed), at `fps`.
pub fn contacts(
    sk: &Skeleton,
    body: &Body,
    pose: (&[Quat], Vec3),
    before: Option<(&[Quat], Vec3)>,
    fps: f64,
    surfaces: &[Surface],
) -> R<(Vec3, Vec<Contact>)> {
    let (world, pos) = sk.fk(pose.0, pose.1).map_err(|e| e.to_string())?;
    let pts = body.points(&world, &pos);
    let prev = match before {
        Some((r, o)) => {
            let (w, p) = sk.fk(r, o).map_err(|e| e.to_string())?;
            body.points(&w, &p)
        }
        None => pts.clone(),
    };
    let on = pts
        .iter()
        .zip(&prev)
        .enumerate()
        .filter(|(_, (p, _))| p[2] - ground(surfaces, p[0], p[1]) < NEAR)
        .map(|(k, (p, q))| Contact {
            point: *p,
            pinned: (p[0] - q[0]).hypot(p[1] - q[1]) * fps < PLANT_SPEED,
            foot: body.foot_of(k),
        })
        .collect();
    Ok((body.com(&pos), on))
}

fn cross2(o: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// Convex hull, counter-clockwise (Andrew's monotone chain); 1 or 2 points for degenerate input.
pub fn hull(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut p = points.to_vec();
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    p.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-12 && (a[1] - b[1]).abs() < 1e-12);
    if p.len() < 3 {
        return p;
    }
    let mut h: Vec<[f64; 2]> = Vec::new();
    for pass in 0..2 {
        let start = h.len();
        let iter: Box<dyn Iterator<Item = &[f64; 2]>> = if pass == 0 {
            Box::new(p.iter())
        } else {
            Box::new(p.iter().rev())
        };
        for &q in iter {
            while h.len() >= start + 2 && cross2(h[h.len() - 2], h[h.len() - 1], q) <= 0.0 {
                h.pop();
            }
            h.push(q);
        }
        h.pop();
    }
    if h.len() < 3 {
        // all on a line: its two ends
        return vec![p[0], p[p.len() - 1]];
    }
    h
}

/// Whether `q` is inside a counter-clockwise hull (within `eps` m). A point or a segment holds
/// only what lies on it.
pub fn contains(hull: &[[f64; 2]], q: [f64; 2], eps: f64) -> bool {
    if hull.len() < 3 {
        let n = nearest(hull, q);
        return (n[0] - q[0]).hypot(n[1] - q[1]) <= eps;
    }
    (0..hull.len()).all(|i| {
        let (a, b) = (hull[i], hull[(i + 1) % hull.len()]);
        cross2(a, b, q) / (b[0] - a[0]).hypot(b[1] - a[1]) >= -eps
    })
}

fn nearest_on_segment(a: [f64; 2], b: [f64; 2], q: [f64; 2]) -> [f64; 2] {
    let d = [b[0] - a[0], b[1] - a[1]];
    let l = d[0] * d[0] + d[1] * d[1];
    let t = if l == 0.0 {
        0.0
    } else {
        (((q[0] - a[0]) * d[0] + (q[1] - a[1]) * d[1]) / l).clamp(0.0, 1.0)
    };
    [a[0] + d[0] * t, a[1] + d[1] * t]
}

/// Nearest point of the hull's outline to `q`.
fn nearest(hull: &[[f64; 2]], q: [f64; 2]) -> [f64; 2] {
    match hull.len() {
        0 => q,
        1 => hull[0],
        n => (0..n)
            .map(|i| nearest_on_segment(hull[i], hull[(i + 1) % n], q))
            .min_by(|a, b| {
                (a[0] - q[0])
                    .hypot(a[1] - q[1])
                    .total_cmp(&(b[0] - q[0]).hypot(b[1] - q[1]))
            })
            .unwrap_or(q),
    }
}

/// Where balance aims the centre of mass: the nearest edge point, moved [`BALANCE_MARGIN`] toward
/// the middle of the support.
fn balance_target(hull: &[[f64; 2]], com: [f64; 2]) -> [f64; 2] {
    let n = nearest(hull, com);
    let k = hull.len() as f64;
    let mid = [
        hull.iter().map(|p| p[0]).sum::<f64>() / k,
        hull.iter().map(|p| p[1]).sum::<f64>() / k,
    ];
    let d = [mid[0] - n[0], mid[1] - n[1]];
    let l = d[0].hypot(d[1]);
    if l < 1e-12 {
        return n;
    }
    let s = BALANCE_MARGIN.min(l) / l;
    [n[0] + d[0] * s, n[1] + d[1] * s]
}

/// Move the hips (feet held) until the centre of mass is over the support.
fn rebalance(
    sk: &Skeleton,
    body: &Body,
    rots: &mut Vec<Quat>,
    root: &mut Vec3,
    surfaces: &[Surface],
    limits: Option<&Limits>,
) -> R<()> {
    let widened = limits.map(|l| l.widened(rots));
    let (world, pos) = sk.fk(rots, *root).map_err(|e| e.to_string())?;
    // the feet on the ground stay exactly where they are
    let pts = body.points(&world, &pos);
    let mut held: Vec<usize> = pts
        .iter()
        .enumerate()
        .filter(|(_, p)| p[2] - ground(surfaces, p[0], p[1]) < NEAR)
        .map(|(k, _)| body.foot_of(k))
        .collect();
    held.dedup();
    let mut feet_goals: Vec<Goal> = Vec::new();
    for f in held {
        feet_goals.extend(body.holds(sk, f).into_iter().map(|j| Goal {
            bone: j,
            target: pos[j],
        }));
    }
    for _ in 0..BALANCE_ROUNDS {
        let Some(b) = balance(sk, body, rots, *root, surfaces)? else {
            return Ok(());
        };
        let t = balance_target(&b.support, b.com);
        let d = [t[0] - b.com[0], t[1] - b.com[1]];
        if d[0].hypot(d[1]) < 1e-4 {
            return Ok(());
        }
        let p = sk.fk(rots, *root).map_err(|e| e.to_string())?.1;
        let mut goals = feet_goals.clone();
        goals.push(Goal {
            bone: 0,
            target: [p[0][0] + d[0], p[0][1] + d[1], p[0][2]],
        });
        let s = fullbody::solve(sk, (rots, *root), &goals, widened.as_ref())?;
        *rots = s.rotations;
        *root = s.root;
    }
    Ok(())
}

pub fn solve(
    sk: &Skeleton,
    rots: &[Vec<Quat>],
    roots: &[Vec3],
    fps: f64,
    surfaces: &[Surface],
    limits: Option<&Limits>,
) -> R<Report> {
    let n = rots.len();
    if n != roots.len() {
        return Err("rotations and roots differ in length".into());
    }
    if n > MAX_FRAMES {
        return Err(format!("{n} frames (most {MAX_FRAMES})"));
    }
    if !(fps.is_finite() && fps > 0.0) {
        return Err(format!("fps {fps} must be above 0"));
    }
    let body = Body::new(sk)?;
    let h = sk.hip_height;
    let mut out_rots = rots.to_vec();
    let mut out = roots.to_vec();
    // foot points and centre of mass per frame, as keyed
    let mut pts: Vec<Vec<Vec3>> = Vec::with_capacity(n);
    let mut coms: Vec<Vec3> = Vec::with_capacity(n);
    for (r, o) in rots.iter().zip(roots) {
        let (w, p) = sk.fk(r, *o).map_err(|e| e.to_string())?;
        pts.push(body.points(&w, &p));
        coms.push(body.com(&p));
    }
    // speed across the floor (m/s) of a keyed point: backward difference (forward on frame 0)
    let speed = |a: Vec3, b: Vec3| (a[0] - b[0]).hypot(a[1] - b[1]) * fps;
    let near = |f: usize| if f > 0 { f - 1 } else { (f + 1).min(n - 1) };
    let point_speed: Vec<Vec<f64>> = (0..n)
        .map(|f| {
            pts[f]
                .iter()
                .zip(&pts[near(f)])
                .map(|(a, b)| speed(*a, *b))
                .collect()
        })
        .collect();
    let still: Vec<bool> = (0..n)
        .map(|f| speed(coms[f], coms[near(f)]) < STILL_SPEED)
        .collect();
    // 1. support
    let mut lifted_frames = Vec::new();
    for (f, (o, ps)) in out.iter_mut().zip(pts.iter_mut()).enumerate() {
        let low = ps
            .iter()
            .map(|p| p[2] - ground(surfaces, p[0], p[1]))
            .fold(f64::INFINITY, f64::min);
        if low < 0.0 {
            o[2] -= low / h;
            ps.iter_mut().for_each(|p| p[2] -= low);
            lifted_frames.push(f);
        }
    }
    // 2 + 3. contact and fulcrum: walk forward, pinning each point where it touched down
    let mut on = vec![false; n];
    let mut anchors: Vec<Option<[f64; 2]>> = vec![None; pts.first().map_or(0, Vec::len)];
    let mut off = [0.0f64; 2];
    let mut slid_frames = Vec::new();
    for f in 0..n {
        let mut sum = [0.0; 2];
        let mut count = 0.0;
        for (k, p) in pts[f].iter().enumerate() {
            let (x, y) = (p[0] + off[0], p[1] + off[1]);
            let low = p[2] - ground(surfaces, x, y) < NEAR;
            on[f] |= low;
            if low && point_speed[f][k] < PLANT_SPEED {
                let a = *anchors[k].get_or_insert([x, y]);
                sum[0] += a[0] - p[0];
                sum[1] += a[1] - p[1];
                count += 1.0;
            } else {
                anchors[k] = None;
            }
        }
        if count > 0.0 {
            let next = [sum[0] / count, sum[1] / count];
            if (next[0] - off[0]).hypot(next[1] - off[1]) > 1e-4 {
                slid_frames.push(f);
            }
            off = next;
        }
        out[f][0] += off[0] / h;
        out[f][1] += off[1] / h;
    }
    // 4. balance, on the ground and still
    let mut balanced = Vec::new();
    for f in (0..n).filter(|&f| on[f] && still[f]) {
        if let Some(b) = balance(sk, &body, &out_rots[f], out[f], surfaces)?
            && !b.inside
        {
            rebalance(sk, &body, &mut out_rots[f], &mut out[f], surfaces, limits)?;
            balanced.push(f);
        }
    }
    // 5. flights
    let mut flights = Vec::new();
    let mut k = 0;
    while k < n {
        if on[k] {
            k += 1;
            continue;
        }
        let start = k;
        while k < n && !on[k] {
            k += 1;
        }
        if start == 0 || k == n {
            continue; // starts or ends in the air: no take-off or no landing to aim between
        }
        let (a, b) = (start - 1, k);
        let total = (b - a) as f64 / fps;
        if b - a - 1 < MIN_FLIGHT || total > MAX_AIR {
            continue;
        }
        let (oa, ob) = (out[a], out[b]);
        for (j, o) in out.iter_mut().enumerate().take(b).skip(a + 1) {
            let t = (j - a) as f64 / fps;
            let s = t / total;
            o[0] = oa[0] + (ob[0] - oa[0]) * s;
            o[1] = oa[1] + (ob[1] - oa[1]) * s;
            o[2] = oa[2] + (ob[2] - oa[2]) * s + 0.5 * G * t * (total - t) / h;
        }
        flights.push((a, b));
    }
    Ok(Report {
        rotations: out_rots,
        roots: out,
        flights,
        lifted: lifted_frames.len(),
        slid: slid_frames.len(),
        balanced,
        lifted_frames,
        slid_frames,
    })
}
