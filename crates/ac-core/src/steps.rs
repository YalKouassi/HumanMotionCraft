//! Do a fill's steps stay regular (roadmap 153500): the feet take turns, and no step is under half
//! the fill's median step. Simple enough to check by hand, and `training/ac_train/evaluate.py`
//! (`step_report`) is the same rule:
//!
//! - A **contact** is a run of frames a foot is down ([`crate::contact`]) that lands inside the
//!   span; a foot already down on the span's first frame was not stepped onto. Its place is the
//!   ankle on the contact's middle frame (of the frames inside the span).
//! - A **step** goes from one contact to the next, in landing order. Its length is signed, along
//!   the way the root travelled between those two middle frames: the feet stand apart sideways,
//!   so a foot landing level with the other is a step of about 0, and one landing behind it is
//!   under 0. With the root still (under [`STILL_M`]) it is the plain distance.
//!
//! With fewer than [`MIN_STEPS`] steps there is nothing to compare: `regular` is `None`.

use crate::contact;
use crate::document::Document;
use crate::physics::Body;
use crate::quat;

/// A step under this share of the median step is short.
pub const SHORT_SHARE: f64 = 0.5;
/// Fewer steps than this say nothing about their regularity.
pub const MIN_STEPS: usize = 2;
/// The root moving less than this (m) between two contacts gives no way of travel.
pub const STILL_M: f64 = 0.01;

/// One foot landing inside the span.
#[derive(Debug, Clone, PartialEq)]
pub struct Contact {
    pub foot: &'static str,
    pub from: i64,
    pub to: i64,
    /// The ankle across the floor (m) on the contact's middle frame.
    pub at: [f64; 2],
    /// The root across the floor (m) on that frame.
    pub root: [f64; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Steps {
    pub contacts: Vec<Contact>,
    /// Each step's signed length (m): `lengths[i]` goes from contact `i` to contact `i + 1`.
    pub lengths: Vec<f64>,
    pub median_m: Option<f64>,
    /// No two contacts in a row are the same foot's.
    pub alternate: bool,
    /// The steps under [`SHORT_SHARE`] of the median, as indexes into `lengths`.
    pub short: Vec<usize>,
    /// `None`: too few steps to say.
    pub regular: Option<bool>,
}

/// The rule, on contacts in landing order.
pub fn judge(contacts: Vec<Contact>) -> Steps {
    let lengths: Vec<f64> = contacts.windows(2).map(|w| length(&w[0], &w[1])).collect();
    let alternate = contacts.windows(2).all(|w| w[0].foot != w[1].foot);
    let median_m = median(&lengths);
    let short = match median_m {
        Some(m) if lengths.len() >= MIN_STEPS => lengths
            .iter()
            .enumerate()
            .filter(|(_, l)| **l < SHORT_SHARE * m)
            .map(|(i, _)| i)
            .collect(),
        _ => Vec::new(),
    };
    let regular = (lengths.len() >= MIN_STEPS).then_some(alternate && short.is_empty());
    Steps {
        contacts,
        lengths,
        median_m,
        alternate,
        short,
        regular,
    }
}

fn length(a: &Contact, b: &Contact) -> f64 {
    let (dx, dy) = (b.at[0] - a.at[0], b.at[1] - a.at[1]);
    let (rx, ry) = (b.root[0] - a.root[0], b.root[1] - a.root[1]);
    let travel = rx.hypot(ry);
    if travel < STILL_M {
        dx.hypot(dy)
    } else {
        (dx * rx + dy * ry) / travel
    }
}

fn median(values: &[f64]) -> Option<f64> {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    let n = v.len();
    match n {
        0 => None,
        _ if n % 2 == 1 => v.get(n / 2).copied(),
        _ => Some((v.get(n / 2 - 1)? + v.get(n / 2)?) / 2.0),
    }
}

/// The steps of frames `a..=b` of the keys.
pub fn measure(doc: &Document, a: i64, b: i64) -> Result<Steps, String> {
    let last = (doc.clip.length_with(&doc.links) - 1).max(0);
    // a frame either side: a foot already down on `a` shows as landing before it
    let (lo, hi) = ((a - 1).max(0), (b + 1).min(last));
    let fr: Vec<f64> = (lo..=hi).map(|f| f as f64).collect();
    let (rots, roots) = doc.clip.sample_with(&fr, &doc.links)?;
    let sk = &doc.skeleton;
    let body = Body::new(sk)?;
    let ankle = |name: &str| {
        sk.names
            .iter()
            .position(|n| n == name)
            .ok_or_else(|| format!("the rig has no {name}"))
    };
    let ankles = [ankle("foot.L")?, ankle("foot.R")?];
    let mut feet = Vec::with_capacity(rots.len());
    let mut joints = Vec::with_capacity(rots.len());
    for (r, o) in rots.iter().zip(&roots) {
        let (world, pos) = sk.fk(r, *o).map_err(|e| e.to_string())?;
        feet.push(
            body.points(&world, &pos)
                .into_iter()
                .enumerate()
                .map(|(k, q)| (body.foot_of(k), q))
                .collect::<Vec<(usize, quat::Vec3)>>(),
        );
        joints.push(pos);
    }
    let phases = contact::phases(&feet, doc.clip.fps, &doc.clip.surfaces, 0.0);
    let mut contacts = Vec::new();
    for ((name, phase), ankle) in ["L", "R"].into_iter().zip(&phases).zip(ankles) {
        let mut f = 0;
        while f < phase.len() {
            if !phase.get(f).is_some_and(|p| p.down()) {
                f += 1;
                continue;
            }
            let start = f;
            while phase.get(f).is_some_and(|p| p.down()) {
                f += 1;
            }
            let (from, to) = (lo + start as i64, lo + f as i64 - 1);
            if from <= a || from > b {
                continue;
            }
            let mid = ((from + to.min(b)) / 2 - lo) as usize;
            let pos = joints.get(mid).ok_or("a contact's frame was not sampled")?;
            let (Some(p), Some(r)) = (pos.get(ankle), pos.first()) else {
                return Err("a contact's joints were not posed".into());
            };
            contacts.push(Contact {
                foot: name,
                from,
                to: to.min(b),
                at: [p[0], p[1]],
                root: [r[0], r[1]],
            });
        }
    }
    contacts.sort_by_key(|c| c.from);
    Ok(judge(contacts))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// Feet at x = 0, the root walking steadily along -y: a length is the foot's move that way.
    fn walk(feet: &[(&'static str, f64)]) -> Steps {
        judge(
            feet.iter()
                .enumerate()
                .map(|(i, &(foot, y))| Contact {
                    foot,
                    from: i as i64 * 10,
                    to: i as i64 * 10 + 5,
                    at: [0.0, y],
                    root: [0.0, -0.3 * i as f64],
                })
                .collect(),
        )
    }

    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} against {b}");
    }

    /// The owner's half step (roadmap 167000, anchors on 30 and 60): steps 0.50, a 0.02 step back,
    /// 0.45, 0.63. In order -0.02, 0.45, 0.50, 0.63: median 0.475, half of it 0.2375.
    #[test]
    fn the_half_step_is_short() {
        let s = walk(&[
            ("L", -0.41),
            ("R", -0.91),
            ("L", -0.89),
            ("R", -1.34),
            ("L", -1.97),
        ]);
        near(s.lengths[1], -0.02);
        near(s.median_m.unwrap(), 0.475);
        assert!(s.alternate);
        assert_eq!(s.short, [1]);
        assert_eq!(s.regular, Some(false));
    }

    /// Anchors on every source step (167000): 0.31, 0.33, 0.29, 0.28, 0.25; median 0.29.
    #[test]
    fn even_steps_are_regular() {
        let s = walk(&[
            ("L", -0.53),
            ("R", -0.84),
            ("L", -1.17),
            ("R", -1.46),
            ("L", -1.74),
            ("R", -1.99),
        ]);
        near(s.median_m.unwrap(), 0.29);
        assert_eq!(s.regular, Some(true));
    }

    /// Anchors on 22, 44, 66 (167000): the last step goes back 0.16 m. Unsigned it would pass (the
    /// median is 0.21, half 0.105); signed it is short.
    #[test]
    fn a_step_back_is_short() {
        let s = walk(&[
            ("L", -0.54),
            ("R", -0.75),
            ("L", -1.12),
            ("R", -1.33),
            ("L", -1.79),
            ("R", -1.63),
        ]);
        near(s.median_m.unwrap(), 0.21);
        assert_eq!(s.short, [4]);
    }

    /// Exactly half the median is not under it: 0.2, 0.4, 0.4 has median 0.4.
    #[test]
    fn half_the_median_itself_passes() {
        let s = walk(&[("L", 0.0), ("R", -0.2), ("L", -0.6), ("R", -1.0)]);
        assert!(s.short.is_empty());
        assert_eq!(s.regular, Some(true));
    }

    #[test]
    fn the_same_foot_twice_does_not_alternate() {
        let s = walk(&[("L", 0.0), ("L", -0.4), ("R", -0.8)]);
        assert!(!s.alternate);
        assert!(s.short.is_empty());
        assert_eq!(s.regular, Some(false));
    }

    /// One step, or none, has nothing to compare with.
    #[test]
    fn too_few_steps_say_nothing() {
        assert_eq!(walk(&[]).regular, None);
        assert_eq!(walk(&[("L", 0.0)]).median_m, None);
        assert_eq!(walk(&[("L", 0.0), ("R", -0.4)]).regular, None);
    }

    /// Sideways stance doesn't count as a step: 0.2 m across, level along the way of travel.
    #[test]
    fn a_step_is_along_the_way_of_travel() {
        let c = |foot, x: f64, y: f64, ry: f64| Contact {
            foot,
            from: 0,
            to: 0,
            at: [x, y],
            root: [0.0, ry],
        };
        near(length(&c("L", 0.1, -1.0, -1.0), &c("R", -0.1, -1.0, -1.3)), 0.0);
        // the root still: the plain distance, 0.2 m
        near(length(&c("L", 0.1, -1.0, -1.0), &c("R", -0.1, -1.0, -1.0)), 0.2);
    }
}
