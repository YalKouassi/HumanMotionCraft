//! Dynamic balance: where a moving body's weight is heading against the feet it has, or is about to
//! have. A rule simple enough to check by hand:
//!
//! - The capture point is the centre of mass plus its velocity over `sqrt(g / height)`: where a
//!   body would have to step to come to rest.
//! - The support is the hull of the foot points of every foot on the ground, and of every foot that
//!   comes down within [`LOOK_AHEAD`] seconds, where it comes down.
//! - A frame's gap is how far the capture point is outside that support, across the way the body
//!   is going over a second (ahead of the feet, the next step catches it).
//!
//! No gap is measured in flight, on a body resting on more than its feet, on one that stands still
//! (static balance covers it), or on a walk on the spot (no foot is planted).

use std::collections::BTreeSet;

use crate::clip::Surface;
use crate::contact;
use crate::physics::{self, Body};
use crate::quat::{Quat, Vec3, sub3};

/// How far ahead (s) a foot coming down counts as support.
pub const LOOK_AHEAD: f64 = 0.5;
/// A gap this size (m) is left alone: the recorded walks `Walk 12` to `17` reach 6.8 cm and the
/// sprints 7.9.
pub const TOLERATED: f64 = 0.08;
/// Over a second, a body that goes less than this (m) isn't going anywhere: it sways or stands.
const MIN_TRAVEL: f64 = 0.3;
/// The share of a second a foot must slide on the floor for a body going nowhere to be walking on
/// the spot.
const ON_THE_SPOT: f64 = 0.75;
const ROUNDS: usize = 3;
/// How many times a field of moves is softened.
const SOFTEN: usize = 12;
const G: f64 = 9.81;

/// One frame's gap: how far (m) outside the support, and the move across the floor that closes it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gap {
    pub outside: f64,
    pub by: [f64; 2],
}

/// A frame as the gap needs it.
pub struct Sample {
    /// Foot points with the foot each belongs to.
    pub feet: Vec<(usize, Vec3)>,
    pub com: Vec3,
    /// The body rests on the ground on more than its feet.
    pub rests: bool,
}

impl Sample {
    pub fn new(body: &Body, points: Vec<Vec3>, pos: &[Vec3], surfaces: &[Surface]) -> Sample {
        Sample {
            feet: points
                .into_iter()
                .enumerate()
                .map(|(k, q)| (body.foot_of(k), q))
                .collect(),
            com: body.com(pos),
            rests: body.rests(pos, surfaces),
        }
    }
}

/// Each frame's gap, `None` where there is none (it is inside, or isn't measured).
pub fn gaps(frames: &[Sample], fps: f64, surfaces: &[Surface]) -> Vec<Option<Gap>> {
    let n = frames.len();
    let feet: Vec<Vec<(usize, Vec3)>> = frames.iter().map(|s| s.feet.clone()).collect();
    let lift = feet
        .iter()
        .flatten()
        .map(|(_, p)| p[2] - physics::ground(surfaces, p[0], p[1]))
        .fold(f64::INFINITY, f64::min)
        .max(0.0);
    let phases = contact::phases(&feet, fps, surfaces, lift);
    let plain: Vec<Vec<Vec3>> = feet
        .iter()
        .map(|ps| ps.iter().map(|(_, p)| *p).collect())
        .collect();
    let coms: Vec<Vec3> = frames.iter().map(|s| s.com).collect();
    let stands = physics::standing(&plain, &coms, fps);
    let ahead = (LOOK_AHEAD * fps).round() as usize;
    (0..n)
        .map(|f| {
            let frame = frames.get(f)?;
            if frame.rests || stands.get(f).copied().unwrap_or(false) {
                return None;
            }
            let mut on: Vec<[f64; 2]> = Vec::new();
            for (foot, phase) in phases.iter().enumerate() {
                let last = (f + ahead).min(n.saturating_sub(1));
                // the next step may be past the clip's end
                if f + ahead >= n && !phase.get(f).is_some_and(|p| p.down()) {
                    return None;
                }
                if let Some(at) = (f..=last).find(|&g| phase.get(g).is_some_and(|p| p.down())) {
                    let points = feet.get(at)?.iter().filter(|(k, _)| *k == foot);
                    on.extend(points.map(|(_, p)| [p[0], p[1]]));
                }
            }
            if on.is_empty() {
                return None;
            }
            let (a, b) = (f.saturating_sub(1), (f + 1).min(n.saturating_sub(1)));
            let v = sub3(*coms.get(b)?, *coms.get(a)?);
            let dt = (b - a).max(1) as f64 / fps;
            let w = (G / frame.com[2].max(0.1)).sqrt();
            let cp = [frame.com[0] + v[0] / dt / w, frame.com[1] + v[1] / dt / w];
            let hull = physics::hull(&on);
            if physics::contains(&hull, cp, 1e-9) {
                return None;
            }
            let near = physics::nearest(&hull, cp);
            let by = [near[0] - cp[0], near[1] - cp[1]];
            let half = (fps / 2.0).round() as usize;
            let t = sub3(
                *coms.get((f + half).min(n.saturating_sub(1)))?,
                *coms.get(f.saturating_sub(half))?,
            );
            let l = t[0].hypot(t[1]);
            if l < MIN_TRAVEL && on_the_spot(&feet, f, half, fps, surfaces, lift) {
                return None;
            }
            let by = if l < MIN_TRAVEL {
                by
            } else {
                let (x, y) = (t[0] / l, t[1] / l);
                let along = by[0] * x + by[1] * y;
                [by[0] - along * x, by[1] - along * y]
            };
            let outside = by[0].hypot(by[1]);
            (outside > 1e-9).then_some(Gap { outside, by })
        })
        .collect()
}

/// Under a body going nowhere, a foot on the floor is moving as a swinging foot does for most of the
/// second around frame `f`: a walk on the spot. The feet slide back beneath it, none is planted, so
/// there is no support to measure against. (A foot that only skims the floor isn't one.)
fn on_the_spot(
    feet: &[Vec<(usize, Vec3)>],
    f: usize,
    half: usize,
    fps: f64,
    surfaces: &[Surface],
    lift: f64,
) -> bool {
    let (a, b) = (
        f.saturating_sub(half),
        (f + half).min(feet.len().saturating_sub(1)),
    );
    let sliding = (a..=b).filter(|&g| skates(feet, g, fps, surfaces, lift)).count();
    sliding as f64 >= ON_THE_SPOT * (b - a + 1) as f64
}

/// A foot point on the floor is moving as a swinging foot does on frame `f`.
fn skates(feet: &[Vec<(usize, Vec3)>], f: usize, fps: f64, surfaces: &[Surface], lift: f64) -> bool {
    let (a, b) = (f.saturating_sub(1), (f + 1).min(feet.len().saturating_sub(1)));
    let (Some(now), Some(p0), Some(p1)) = (feet.get(f), feet.get(a), feet.get(b)) else {
        return false;
    };
    let dt = (b - a).max(1) as f64 / fps;
    now.iter()
        .zip(p0.iter().zip(p1))
        .any(|((_, p), ((_, x), (_, y)))| {
            let height = p[2] - physics::ground(surfaces, p[0], p[1]) - lift;
            height < physics::NEAR && (y[0] - x[0]).hypot(y[1] - x[1]) / dt > physics::PLANT_SPEED
        })
}

/// Soften a field of moves, so the hips don't gain speed from them.
fn smooth(v: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut out = v.to_vec();
    for _ in 0..SOFTEN {
        let was = out.clone();
        for (f, slot) in out.iter_mut().enumerate() {
            let at = |i: usize| was.get(i).copied().unwrap_or([0.0; 2]);
            let (a, b, c) = (at(f.saturating_sub(1)), at(f), at(f + 1));
            *slot = [(a[0] + 2.0 * b[0] + c[0]) / 4.0, (a[1] + 2.0 * b[1] + c[1]) / 4.0];
        }
    }
    out
}

impl crate::document::Document {
    /// Each frame of the keys as the gap needs it.
    fn samples(&self, rots: &[Vec<Quat>], roots: &[Vec3], body: &Body) -> Result<Vec<Sample>, String> {
        rots.iter()
            .zip(roots)
            .map(|(r, o)| {
                let (w, p) = self.skeleton.fk(r, *o).map_err(|e| e.to_string())?;
                Ok(Sample::new(body, body.points(&w, &p), &p, &self.clip.surfaces))
            })
            .collect()
    }

    /// Move the hips, feet held, where a moving body's weight is over [`TOLERATED`] to the side of
    /// its feet; returns the frames changed, and the worst gap left (cm).
    pub fn transfer_weight(&mut self) -> Result<(usize, f64), String> {
        if self.clip.base().keys.is_empty() {
            return Ok((0, 0.0));
        }
        let sk = self.skeleton.clone();
        let limits = self.limits.clone();
        let frames: Vec<f64> = (0..self.clip.length()).map(|f| f as f64).collect();
        let (mut rots, mut roots) = self.clip.sample_with(&frames, &self.links)?;
        let body = Body::new(&sk)?;
        let fps = self.clip.fps;
        let mut changed = BTreeSet::new();
        let worst = |samples: &[Sample]| {
            gaps(samples, fps, &self.clip.surfaces)
                .iter()
                .flatten()
                .map(|g| g.outside)
                .fold(0.0, f64::max)
        };
        let mut samples = self.samples(&rots, &roots, &body)?;
        let mut left = worst(&samples);
        for _ in 0..ROUNDS {
            let mut over = Vec::new();
            let mut moved = vec![[0.0; 2]; rots.len()];
            for (f, g) in gaps(&samples, fps, &self.clip.surfaces).iter().enumerate() {
                if let (Some(g), Some(slot)) = (g.filter(|g| g.outside > TOLERATED), moved.get_mut(f)) {
                    let k = (g.outside - TOLERATED) / g.outside;
                    *slot = [g.by[0] * k, g.by[1] * k];
                    over.push(f);
                }
            }
            if over.is_empty() {
                break;
            }
            let field: Vec<[f64; 2]> = (0..rots.len())
                .map(|f| {
                    if over.binary_search(&f).is_ok() {
                        moved.get(f).copied().unwrap_or([0.0; 2])
                    } else {
                        physics::faded(&over, &moved, f)
                    }
                })
                .collect();
            let field = smooth(&field);
            let (mut r2, mut o2) = (rots.clone(), roots.clone());
            let mut touched = Vec::new();
            for (f, by) in field.iter().enumerate() {
                if by[0].hypot(by[1]) < 1e-5 || samples.get(f).is_none_or(|s| s.rests) {
                    continue;
                }
                let (Some(r), Some(o)) = (r2.get_mut(f), o2.get_mut(f)) else {
                    continue;
                };
                physics::shift_hips(&sk, &body, (r, o), *by, &self.clip.surfaces, limits.as_deref())?;
                touched.push(f);
            }
            // moving the hips changes their speed too: a round that doesn't help is thrown away
            let next = self.samples(&r2, &o2, &body)?;
            let now = worst(&next);
            if now >= left {
                break;
            }
            (rots, roots, samples, left) = (r2, o2, next, now);
            changed.extend(touched);
        }
        let left = left * 100.0;
        self.write_moved(&changed, &rots, &roots, "weight")?;
        self.go_to(self.frame)?;
        Ok((changed.len(), left))
    }
}
