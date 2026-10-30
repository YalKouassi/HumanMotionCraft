//! Heel-to-toe roll (`feet.roll`): a foot in contact turns about its heel or the ball of the foot
//! when a straight leg can't otherwise reach it. A rule simple enough to check by hand:
//!
//! - On each frame of a contact ([`crate::contact`]), a leg stretched past [`STRETCH`] of its length
//!   has to give if the hips have gone past the foot along its length. With the hips ahead of the
//!   ball, the heel rises about the ball (push-off). With them behind the heel, the toes rise about
//!   the heel (heel strike). Over the foot, or off to its side, a straight leg isn't a reason to roll.
//! - A contact that needs this is solved as one: the foot stays where it is on the contact's least
//!   stretched frame, and each frame turns it the least that brings the ankle in reach, up to
//!   [`MOST_ROLL`]. The toes stay flat through a push-off.
//! - The turn is smoothed over time and the change fades over [`FADE`] frames into the swing.
//!
//! A contact that doesn't need it stays exactly as it is, and so does a foot that already rolls by
//! more than [`ROLLS`], so a stand, a shuffle and a recorded walk don't change. Writes keys tagged
//! `foot-roll`.

use std::collections::BTreeSet;

use crate::contact::Phase;
use crate::document::Document;
use crate::ik;
use crate::physics::Body;
use crate::quat::{self, Mat3, Quat, Vec3};
use crate::rig::Skeleton;

type R<T> = Result<T, String>;

/// A leg stretched past this share of its length rolls the foot.
pub const STRETCH: f64 = 0.985;
/// The most a foot rolls (degrees).
pub const MOST_ROLL: f64 = 50.0;
/// Frames either side of a contact over which a roll fades out.
pub const FADE: usize = 5;
/// A foot whose heel moves this far (m) against its ball during a contact already rolls.
pub const ROLLS: f64 = 0.02;

/// A leg this far (m) past its length is short of the foot; less doesn't show.
const SHORT: f64 = 0.005;

/// A foot's place on one frame.
#[derive(Clone)]
struct Place {
    ankle: Vec3,
    ball: Vec3,
    heel: Vec3,
    turn: Mat3,
    toes: Vec<Mat3>,
}

impl Place {
    fn forward(&self) -> Vec3 {
        let v = quat::sub3(self.ball, self.heel);
        let l = v[0].hypot(v[1]).max(1e-9);
        [v[0] / l, v[1] / l, 0.0]
    }

    /// The world turn for `up` radians: positive lifts what is behind a pivot (the heel about the
    /// ball), negative what is ahead of it (the toes about the heel).
    fn lift(&self, up: f64) -> Mat3 {
        let f = self.forward();
        ik::axis_angle([-f[1], f[0], 0.0], up)
    }

    /// Where the ankle goes for `up`.
    fn ankle_at(&self, up: f64) -> Vec3 {
        let pivot = if up > 0.0 { self.ball } else { self.heel };
        let turned = quat::mat3_vec(&self.lift(up), quat::sub3(self.ankle, pivot));
        quat::add3(pivot, turned)
    }

    /// The turn (radians) this frame's hips ask of this foot: 0 when they ask nothing.
    fn needed(&self, hip: Vec3, reach: f64) -> f64 {
        if quat::len3(quat::sub3(hip, self.ankle)) <= reach {
            return 0.0;
        }
        let f = self.forward();
        let past = |p: Vec3| (hip[0] - p[0]) * f[0] + (hip[1] - p[1]) * f[1];
        let across = ((hip[0] - self.ankle[0]) * f[1] - (hip[1] - self.ankle[1]) * f[0]).abs();
        let sign = if past(self.ball) > across {
            1.0
        } else if -past(self.heel) > across {
            -1.0
        } else {
            return 0.0;
        };
        let mut deg = 1.0;
        while deg < MOST_ROLL {
            let up = sign * f64::to_radians(deg);
            if quat::len3(quat::sub3(hip, self.ankle_at(up))) <= reach {
                return up;
            }
            deg += 1.0;
        }
        sign * f64::to_radians(MOST_ROLL)
    }
}

/// Smooth a run of turns, so no frame's turn jumps.
fn smooth(v: &[f64]) -> Vec<f64> {
    let at = |i: isize| {
        v.get(i.clamp(0, v.len() as isize - 1) as usize)
            .copied()
            .unwrap_or(0.0)
    };
    (0..v.len() as isize)
        .map(|i| (at(i - 2) + 2.0 * at(i - 1) + 3.0 * at(i) + 2.0 * at(i + 1) + at(i + 2)) / 9.0)
        .collect()
}

fn ease(d: usize) -> f64 {
    let u = 1.0 - d as f64 / (FADE + 1) as f64;
    u * u * (3.0 - 2.0 * u)
}

fn transpose(m: &Mat3) -> Mat3 {
    [
        [m[0][0], m[1][0], m[2][0]],
        [m[0][1], m[1][1], m[2][1]],
        [m[0][2], m[1][2], m[2][2]],
    ]
}

/// The local rotation that gives a bone `world` under a parent at `parent`.
fn local_for(parent: &Mat3, world: &Mat3) -> Quat {
    quat::normalize(quat::from_mat3(&quat::mat3_mul(&transpose(parent), world)))
}

struct Leg {
    hip: usize,
    ankle: usize,
    ball: usize,
    toes: Vec<usize>,
    foot: usize,
    name: &'static str,
    reach: f64,
    length: f64,
}

impl Leg {
    fn new(sk: &Skeleton, side: &str, name: &'static str) -> R<Leg> {
        let bone = |n: String| sk.bone(&n).ok_or_else(|| format!("the rig has no {n}"));
        let (hip, knee, ankle) = (
            bone(format!("upperleg01.{side}"))?,
            bone(format!("lowerleg01.{side}"))?,
            bone(format!("foot.{side}"))?,
        );
        let h = |i: usize| sk.heads.get(i).copied().unwrap_or_default();
        let length = quat::len3(quat::sub3(h(knee), h(hip))) + quat::len3(quat::sub3(h(ankle), h(knee)));
        Ok(Leg {
            hip,
            ankle,
            ball: bone(format!("toe3-1.{side}"))?,
            toes: (0..sk.len())
                .filter(|&t| sk.parents.get(t).copied().flatten() == Some(ankle))
                .collect(),
            foot: usize::from(side == "R"),
            name,
            reach: length * STRETCH,
            length,
        })
    }

    /// Where this leg's hip and foot are in a pose.
    fn place(&self, sk: &Skeleton, body: &Body, r: &[Quat], root: Vec3) -> R<(Vec3, Place)> {
        let (w, p) = sk.fk(r, root).map_err(|e| e.to_string())?;
        let at = |i: usize| p.get(i).copied().ok_or_else(|| "a joint is missing".to_string());
        let turn = *w.get(self.ankle).ok_or("a joint is missing")?;
        Ok((
            at(self.hip)?,
            Place {
                ankle: at(self.ankle)?,
                ball: at(self.ball)?,
                heel: body.heel(self.foot, &w, &p).ok_or("a joint is missing")?,
                turn,
                toes: self
                    .toes
                    .iter()
                    .map(|&t| w.get(t).copied().ok_or_else(|| "a joint is missing".to_string()))
                    .collect::<R<_>>()?,
            },
        ))
    }

    /// A pose with the ankle at `target` and the foot and (given) toes turned to these world turns.
    fn solve(
        &self,
        sk: &Skeleton,
        r: &[Quat],
        root: Vec3,
        target: Vec3,
        foot: &Mat3,
        toes: Option<&[Mat3]>,
    ) -> R<Vec<Quat>> {
        let pole = ik::own_pole(sk, r, root, self.name).map_err(|e| e.to_string())?;
        let mut out =
            ik::two_bone(sk, r, root, self.name, target, pole, Some(false)).map_err(|e| e.to_string())?;
        let (w, _) = sk.fk(&out, root).map_err(|e| e.to_string())?;
        let parent = sk
            .parents
            .get(self.ankle)
            .copied()
            .flatten()
            .and_then(|i| w.get(i).copied())
            .ok_or("the foot has no parent")?;
        if let Some(slot) = out.get_mut(self.ankle) {
            *slot = local_for(&parent, foot);
        }
        if let Some(toes) = toes {
            for (&t, world) in self.toes.iter().zip(toes) {
                if let Some(slot) = out.get_mut(t) {
                    *slot = local_for(foot, world);
                }
            }
        }
        Ok(out)
    }
}

/// Runs of frames a foot is down, as `first..end`.
fn contacts(phases: &[Phase]) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut f = 0;
    while f < phases.len() {
        if phases[f].down() {
            let a = f;
            while f < phases.len() && phases[f].down() {
                f += 1;
            }
            out.push(a..f);
        } else {
            f += 1;
        }
    }
    out
}

impl Document {
    /// Roll the feet where the legs can't otherwise reach; returns the frames changed, and how many
    /// of them the leg still couldn't reach at the most the foot rolls.
    pub fn roll_feet(&mut self) -> R<(usize, usize)> {
        if self.clip.base().keys.is_empty() {
            return Ok((0, 0));
        }
        let sk = self.skeleton.clone();
        let frames: Vec<f64> = (0..self.clip.length()).map(|f| f as f64).collect();
        let (mut rots, roots) = self.clip.sample_with(&frames, &self.links)?;
        let body = Body::new(&sk)?;
        let phases =
            crate::contact::clip_phases(&body, &sk, &rots, &roots, self.clip.fps, &self.clip.surfaces)?;
        let mut changed = BTreeSet::new();
        let mut short = BTreeSet::new();
        let legs = [Leg::new(&sk, "L", "leg.L")?, Leg::new(&sk, "R", "leg.R")?];
        for (leg, phases) in legs.iter().zip(&phases) {
            let places = rots
                .iter()
                .zip(&roots)
                .map(|(r, o)| leg.place(&sk, &body, r, *o))
                .collect::<R<Vec<_>>>()?;
            for run in contacts(phases) {
                let Some(here) = places.get(run.clone()) else {
                    continue;
                };
                if here.iter().all(|(hip, p)| p.needed(*hip, leg.reach) == 0.0) {
                    continue;
                }
                let tilt = here.iter().map(|(_, p)| p.heel[2] - p.ball[2]);
                let (low, high) = tilt.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), t| {
                    (a.min(t), b.max(t))
                });
                if high - low > ROLLS {
                    continue;
                }
                let stretch = |(hip, p): &(Vec3, Place)| quat::len3(quat::sub3(*hip, p.ankle));
                let Some((_, anchor)) = here
                    .iter()
                    .min_by(|a, b| stretch(a).total_cmp(&stretch(b)))
                    .cloned()
                else {
                    continue;
                };
                let raw: Vec<f64> = here
                    .iter()
                    .map(|(hip, _)| anchor.needed(*hip, leg.reach))
                    .collect();
                // never less than the frame needs: smoothing mustn't leave a leg short
                let turns: Vec<f64> = smooth(&raw)
                    .into_iter()
                    .zip(&raw)
                    .map(|(s, r)| if r.abs() > s.abs() { *r } else { s })
                    .collect();
                for (k, up) in turns.iter().enumerate() {
                    let f = run.start + k;
                    let (Some(r), Some(o), Some((hip, _))) = (rots.get(f), roots.get(f), here.get(k)) else {
                        continue;
                    };
                    let lift = anchor.lift(*up);
                    let target = anchor.ankle_at(*up);
                    if quat::len3(quat::sub3(*hip, target)) > leg.length + SHORT {
                        short.insert(f);
                    }
                    let foot = quat::mat3_mul(&lift, &anchor.turn);
                    let toes: Vec<Mat3> = if *up > 0.0 {
                        anchor.toes.clone()
                    } else {
                        anchor.toes.iter().map(|t| quat::mat3_mul(&lift, t)).collect()
                    };
                    let solved = leg.solve(&sk, r, *o, target, &foot, Some(&toes))?;
                    if let Some(slot) = rots.get_mut(f) {
                        *slot = solved;
                    }
                    changed.insert(f);
                }
                // the change at each end of the contact fades into the swing
                let ends = [
                    (run.start, turns.first().copied(), false),
                    (run.end - 1, turns.last().copied(), true),
                ];
                for (edge, up, after) in ends {
                    let (Some(up), Some((_, at))) = (up, places.get(edge)) else {
                        continue;
                    };
                    let move_by = quat::sub3(anchor.ankle_at(up), at.ankle);
                    let foot = quat::mat3_mul(&anchor.lift(up), &anchor.turn);
                    let spin = quat::normalize(quat::from_mat3(&quat::mat3_mul(&foot, &transpose(&at.turn))));
                    for d in 1..=FADE {
                        let f = if after { edge + d } else { edge.wrapping_sub(d) };
                        if phases.get(f).is_none_or(|p| p.down()) {
                            break;
                        }
                        let (Some(r), Some(o), Some((_, p))) = (rots.get(f), roots.get(f), places.get(f))
                        else {
                            break;
                        };
                        let t = ease(d);
                        let target = quat::add3(p.ankle, quat::scale3(move_by, t));
                        let part = quat::to_mat3(quat::slerp(quat::IDENTITY, spin, t));
                        let solved = leg.solve(&sk, r, *o, target, &quat::mat3_mul(&part, &p.turn), None)?;
                        if let Some(slot) = rots.get_mut(f) {
                            *slot = solved;
                        }
                        changed.insert(f);
                    }
                }
            }
        }
        self.write_solved(&changed, 0, &rots, &roots, "foot-roll")?;
        self.go_to(self.frame)?;
        Ok((changed.len(), short.len()))
    }
}
