//! A whole clip through the drive, and how the character's feet did: the IK weights from the
//! rig's foot phases (full on frames the foot is down, fading over [`BLEND`] frames into the
//! swing), the planted feet's slide (over each planted span, how far the ankle and the toe
//! drift from where they came down while they are down: `slide`; and their per-frame travel
//! added up, as `clip.metrics` counts it: `travel`), the lowest the feet got below the
//! character's floor, the frames a leg could not reach its target, and the nodes nothing
//! drives. The problems come out as spans for the timeline and the command's result.

use crate::contact::Phase;
use crate::quat::{Quat, Vec3};
use crate::rig::Skeleton;

use super::Character;
use super::drive::{Posed, Weights, drive};
use super::plan::Plan;

/// Frames the IK fades over on each side of a contact.
pub const BLEND: usize = 3;
/// A planted foot that drifts more than this (m) over its span is marked.
pub const SLIDE_LIMIT: f64 = 0.01;
/// A foot point this much (m) above its rest height is off the floor and not counted.
pub const NEAR_DOWN: f64 = 0.04;
/// A foot deeper than this (m) under the floor is marked.
pub const BELOW_LIMIT: f64 = 0.01;
/// A leg that stays further than this (m) from its target is marked.
pub const UNREACHED_LIMIT: f64 = 0.005;
/// Most frames one check drives.
pub const MAX_FRAMES: usize = crate::physics::MAX_FRAMES;

#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub from: usize,
    pub to: usize,
    /// `foot-slide`, `below-floor`, `unreached`, `undriven`.
    pub kind: &'static str,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FootReport {
    pub planted_frames: usize,
    /// Planted spans: (from, to, drift in m).
    pub spans: Vec<(usize, usize, f64)>,
    /// The most a planted foot drifted from where it came down (m).
    pub slide: f64,
    /// The most a planted foot's per-frame travel added up to over a span (m).
    pub travel: f64,
    /// The lowest the ankle or toe got above the floor (m; negative: below it).
    pub lowest: f64,
    pub unreached_frames: usize,
    pub unreached_max: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub frames: Vec<Posed>,
    pub weights: Vec<Weights>,
    pub feet: [FootReport; 2],
    pub problems: Vec<Problem>,
}

/// The IK weight of each foot on each frame from its phases, and of the hands (1 when solved).
pub fn weights(phases: &[Vec<Phase>; 2], frames: usize, hands: bool) -> Vec<Weights> {
    let foot = |side: usize| -> Vec<f64> {
        let down: Vec<bool> = (0..frames)
            .map(|f| phases[side].get(f).is_some_and(|p| p.down()))
            .collect();
        (0..frames)
            .map(|f| {
                if down[f] {
                    return 1.0;
                }
                // the nearest down frame within the blend
                (1..=BLEND)
                    .find(|&k| (f >= k && down[f - k]) || down.get(f + k).copied().unwrap_or(false))
                    .map_or(0.0, |k| 1.0 - k as f64 / (BLEND + 1) as f64)
            })
            .collect()
    };
    let (l, r) = (foot(0), foot(1));
    (0..frames)
        .map(|f| Weights {
            feet: [l[f], r[f]],
            hands: if hands { [1.0; 2] } else { [0.0; 2] },
        })
        .collect()
}

fn spans_of(flags: &[bool]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (f, &on) in flags.iter().enumerate() {
        match out.last_mut() {
            Some(s) if on && s.1 + 1 == f => s.1 = f,
            _ if on => out.push((f, f)),
            _ => {}
        }
    }
    out
}

/// Drive every frame and measure the feet. `rots`/`roots` are the rig's motion as shown (the
/// physics pass included where it plays); `phases` its foot phases on those frames.
pub fn drive_clip(
    plan: &Plan,
    c: &Character,
    sk: &Skeleton,
    rots: &[Vec<Quat>],
    roots: &[Vec3],
    phases: &[Vec<Phase>; 2],
) -> Result<Report, String> {
    let n = rots.len();
    if n != roots.len() {
        return Err("rotations and roots differ in length".into());
    }
    if n == 0 {
        return Err("no frames to drive".into());
    }
    if n > MAX_FRAMES {
        return Err(format!("{n} frames (most {MAX_FRAMES})"));
    }
    let weights = weights(phases, n, plan.hands);
    let mut frames = Vec::with_capacity(n);
    for f in 0..n {
        frames.push(drive(plan, c, sk, &rots[f], roots[f], weights[f])?);
    }
    let mut feet = [FootReport::default(), FootReport::default()];
    let mut problems = Vec::new();
    for side in 0..2 {
        let leg = plan.legs[side];
        let who = if side == 0 { "left" } else { "right" };
        let toe = plan
            .drives
            .iter()
            .find(|d| d.role == format!("{}_toe", if side == 0 { "l" } else { "r" }))
            .map(|d| d.node);
        let points: Vec<Vec<Vec3>> = frames
            .iter()
            .map(|p| {
                let mut v = vec![p.worlds.get(leg.end).map_or([0.0; 3], |w| w.p)];
                if let Some(t) = toe {
                    v.push(p.worlds.get(t).map_or([0.0; 3], |w| w.p));
                }
                v
            })
            .collect();
        // each point's rest height above the floor: it is down when near it
        let mut rest_h = vec![plan.ankle_height[1]];
        if let Some(t) = toe {
            rest_h.push(plan.rest.get(t).map_or(0.0, |w| w.p[2]) - plan.floor);
        }
        let down = |f: usize, k: usize| -> bool {
            points
                .get(f)
                .and_then(|p| p.get(k))
                .zip(rest_h.get(k))
                .is_some_and(|(p, h)| p[2] - plan.floor < h + NEAR_DOWN)
        };
        let planted: Vec<bool> = (0..n)
            .map(|f| phases[side].get(f).is_some_and(|p| *p == Phase::Planted))
            .collect();
        let mut rep = FootReport {
            planted_frames: planted.iter().filter(|p| **p).count(),
            lowest: f64::INFINITY,
            ..FootReport::default()
        };
        for (a, b) in spans_of(&planted) {
            let mut drift = vec![0.0; points[a].len()];
            let mut travel = vec![0.0; points[a].len()];
            for (k, (d, t)) in drift.iter_mut().zip(travel.iter_mut()).enumerate() {
                let mut start: Option<Vec3> = None;
                for f in a..=b {
                    if !down(f, k) {
                        continue;
                    }
                    let p = points[f][k];
                    match start {
                        None => start = Some(p),
                        Some(s) => *d = f64::max(*d, (p[0] - s[0]).hypot(p[1] - s[1])),
                    }
                    if f > a && down(f - 1, k) {
                        let q = points[f - 1][k];
                        *t += (p[0] - q[0]).hypot(p[1] - q[1]);
                    }
                }
            }
            let most = drift.iter().copied().fold(0.0, f64::max);
            rep.spans.push((a, b, most));
            rep.slide = rep.slide.max(most);
            rep.travel = rep.travel.max(travel.iter().copied().fold(0.0, f64::max));
            if most > SLIDE_LIMIT {
                problems.push(Problem {
                    from: a,
                    to: b,
                    kind: "foot-slide",
                    text: format!("The {who} foot slides {:.1} cm while planted", most * 100.0),
                });
            }
        }
        let low: Vec<f64> = points
            .iter()
            .map(|pts| {
                pts.iter()
                    .map(|p| p[2] - plan.floor)
                    .fold(f64::INFINITY, f64::min)
            })
            .collect();
        rep.lowest = low.iter().copied().fold(f64::INFINITY, f64::min);
        let below: Vec<bool> = low.iter().map(|z| *z < -BELOW_LIMIT).collect();
        for (a, b) in spans_of(&below) {
            let worst = low[a..=b].iter().copied().fold(0.0, f64::min);
            problems.push(Problem {
                from: a,
                to: b,
                kind: "below-floor",
                text: format!("The {who} foot is {:.1} cm under the floor", -worst * 100.0),
            });
        }
        let gaps: Vec<f64> = frames.iter().map(|p| p.unreached[side]).collect();
        rep.unreached_frames = gaps.iter().filter(|g| **g > UNREACHED_LIMIT).count();
        rep.unreached_max = gaps.iter().copied().fold(0.0, f64::max);
        let missed: Vec<bool> = gaps.iter().map(|g| *g > UNREACHED_LIMIT).collect();
        for (a, b) in spans_of(&missed) {
            let worst = gaps[a..=b].iter().copied().fold(0.0, f64::max);
            problems.push(Problem {
                from: a,
                to: b,
                kind: "unreached",
                text: format!(
                    "The {who} leg can't reach its foot: {:.1} cm short",
                    worst * 100.0
                ),
            });
        }
        feet[side] = rep;
    }
    if !plan.undriven.is_empty() {
        let names: Vec<&str> = plan
            .undriven
            .iter()
            .filter_map(|&i| c.nodes.get(i).map(|n| n.name.as_str()))
            .take(12)
            .collect();
        let more = plan.undriven.len().saturating_sub(names.len());
        problems.push(Problem {
            from: 0,
            to: n - 1,
            kind: "undriven",
            text: format!(
                "{} nodes nothing drives keep their rest pose: {}{}",
                plan.undriven.len(),
                names.join(", "),
                if more > 0 {
                    format!(", ... ({more} more)")
                } else {
                    String::new()
                }
            ),
        });
    }
    problems.sort_by_key(|p| (p.from, p.to));
    Ok(Report {
        frames,
        weights,
        feet,
        problems,
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

    #[test]
    fn weights_fade_into_the_swing() {
        use Phase::*;
        let phases = [
            vec![
                Swing, Swing, Swing, Swing, Landing, Planted, Release, Swing, Swing, Swing, Swing,
            ],
            vec![Swing; 11],
        ];
        let w = weights(&phases, 11, false);
        let l: Vec<f64> = w.iter().map(|x| x.feet[0]).collect();
        assert_eq!(l[4], 1.0);
        assert_eq!(l[6], 1.0);
        assert!((l[3] - 0.75).abs() < 1e-12 && (l[2] - 0.5).abs() < 1e-12 && (l[1] - 0.25).abs() < 1e-12);
        assert_eq!(l[0], 0.0);
        assert!((l[7] - 0.75).abs() < 1e-12 && l[10] == 0.0);
        assert!(w.iter().all(|x| x.feet[1] == 0.0 && x.hands == [0.0; 2]));
        assert_eq!(spans_of(&[true, true, false, true]), [(0, 1), (3, 3)]);
    }
}
