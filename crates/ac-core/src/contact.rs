//! When a foot is on the ground: the one test the physics pass, `clip.metrics` and `feet.lock` use.
//! Each foot has a phase on each frame, worked out from its foot points (see
//! [`crate::physics::Body::points`]):
//!
//! - A foot **comes down** when one of its points is within [`physics::NEAR`] of the ground and
//!   moving slower than [`physics::PLANT_SPEED`] across it.
//! - It **stays down** until its lowest point rises past [`LEAVE_HEIGHT`] or no low point is moving
//!   slower than [`LEAVE_SPEED`]. The gap between coming down and leaving keeps a foot near a limit
//!   from flickering.
//! - A contact lasts at least [`MIN_CONTACT`] frames: a foot that passes the floor isn't planted.
//! - The first [`EDGE`] frames of a contact are `Landing` and the last are `Release`, except at the
//!   clip's two ends, where nothing was seen arriving or leaving. The rest are `Planted`.
//!
//! Deterministic, and simple enough to check by hand.

use crate::clip::Surface;
use crate::physics;
use crate::quat::Vec3;

/// A foot's lowest point past this height (m) lifts it off.
pub const LEAVE_HEIGHT: f64 = 0.08;
/// Every low point of a foot faster than this (m/s, across the floor) lifts it off.
pub const LEAVE_SPEED: f64 = 0.8;
/// Fewer frames than this down is passing the floor, not a contact.
pub const MIN_CONTACT: usize = 3;
/// Frames of landing and of release at the ends of a contact.
pub const EDGE: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Swing,
    Landing,
    Planted,
    Release,
}

impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Phase::Swing => "swing",
            Phase::Landing => "landing",
            Phase::Planted => "planted",
            Phase::Release => "release",
        }
    }

    /// The foot is on the ground.
    pub fn down(self) -> bool {
        self != Phase::Swing
    }
}

/// Each foot's phase on each frame (left, then right). `frames`: each frame's foot points with the
/// foot each belongs to. `lift`: where the ground counts as being, above its real height (0 for the
/// physics pass; a clip keyed above the floor passes its lowest point).
pub fn phases(frames: &[Vec<(usize, Vec3)>], fps: f64, surfaces: &[Surface], lift: f64) -> [Vec<Phase>; 2] {
    [0, 1].map(|foot| one_foot(frames, foot, fps, surfaces, lift))
}

fn one_foot(
    frames: &[Vec<(usize, Vec3)>],
    foot: usize,
    fps: f64,
    surfaces: &[Surface],
    lift: f64,
) -> Vec<Phase> {
    let n = frames.len();
    let mut down = vec![false; n];
    let mut on = false;
    for (f, slot) in down.iter_mut().enumerate() {
        let prev = frames.get(if f > 0 { f - 1 } else { f + 1 });
        // per point: its height, and how fast it crosses the floor
        let pts: Vec<(f64, f64)> = frames[f]
            .iter()
            .enumerate()
            .filter(|(_, (ft, _))| *ft == foot)
            .map(|(k, (_, p))| {
                let height = p[2] - physics::ground(surfaces, p[0], p[1]) - lift;
                let speed = prev
                    .and_then(|q| q.get(k))
                    .map_or(0.0, |(_, q)| (p[0] - q[0]).hypot(p[1] - q[1]) * fps);
                (height, speed)
            })
            .collect();
        let low = pts.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
        let slowest = |limit: f64| {
            pts.iter()
                .filter(|p| p.0 < limit)
                .map(|p| p.1)
                .fold(f64::INFINITY, f64::min)
        };
        on = if on {
            low < LEAVE_HEIGHT && slowest(LEAVE_HEIGHT) < LEAVE_SPEED
        } else {
            slowest(physics::NEAR) < physics::PLANT_SPEED
        };
        *slot = on;
    }
    let mut out = vec![Phase::Swing; n];
    let mut f = 0;
    while f < n {
        if !down[f] {
            f += 1;
            continue;
        }
        let a = f;
        while f < n && down[f] {
            f += 1;
        }
        if f - a < MIN_CONTACT.min(n) {
            continue;
        }
        for (i, slot) in out.iter_mut().enumerate().take(f).skip(a) {
            *slot = if a > 0 && i < a + EDGE {
                Phase::Landing
            } else if f < n && i + EDGE >= f {
                Phase::Release
            } else {
                Phase::Planted
            };
        }
    }
    out
}
