//! Do a fill's steps stay regular (roadmap 153500): the feet take turns, and no step is under half
//! the fill's median step. Simple enough to check by hand, and `training/ac_train/evaluate.py`
//! (`step_report`) is the same rule:
//!
//! - A **contact** is a run of frames a foot is down ([`contact::clip_phases`], so a clip standing
//!   above the floor still lands) that lands inside the span; a foot already down on the span's
//!   first frame was not stepped onto. Its place is the ankle on the contact's middle frame (of the
//!   frames inside the span).
//! - A foot lifted and **replanted** within [`REPLANT_M`] of its place, the other foot not landing
//!   in between, has not stepped: the two are one contact, at the first one's place.
//! - A **step** goes from one contact to the next, in landing order. Its length is signed, along
//!   the way the root travelled between those two middle frames: the feet stand apart sideways,
//!   so a foot landing level with the other is a step of about 0, and one landing behind it is
//!   under 0. With the root still (under [`STILL_M`]) it is the plain distance.
//!
//! - A step over which the root travelled under [`WALKING_M`] is the walker standing (a last step
//!   closing the feet, a shuffle): its length is reported, but it is neither judged nor in the median.
//!
//! With fewer than [`MIN_STEPS`] judged steps there is nothing to compare: `regular` is `None`.

use crate::contact;
use crate::document::Document;
use crate::physics::Body;

/// A step under this share of the median step is short.
pub const SHORT_SHARE: f64 = 0.5;
/// Fewer steps than this say nothing about their regularity.
pub const MIN_STEPS: usize = 2;
/// The root moving less than this (m) between two contacts gives no way of travel.
pub const STILL_M: f64 = 0.01;
/// The same foot coming down nearer than this (m) to its last place has not stepped.
pub const REPLANT_M: f64 = 0.10;
/// The root travelling less than this (m) between two contacts is not walking that step.
pub const WALKING_M: f64 = 0.10;

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
    /// The judged steps under [`SHORT_SHARE`] of the median, as indexes into `lengths`.
    pub short: Vec<usize>,
    /// `None`: too few judged steps to say.
    pub regular: Option<bool>,
}

/// The rule, on contacts in landing order.
pub fn judge(contacts: Vec<Contact>) -> Steps {
    let lengths: Vec<f64> = contacts.windows(2).map(|w| length(&w[0], &w[1])).collect();
    let alternate = contacts.windows(2).all(|w| w[0].foot != w[1].foot);
    let judged: Vec<usize> = contacts
        .windows(2)
        .enumerate()
        .filter(|(_, w)| travel(&w[0], &w[1]) >= WALKING_M)
        .map(|(i, _)| i)
        .collect();
    let median_m = median(&judged.iter().map(|&i| lengths[i]).collect::<Vec<_>>());
    let enough = judged.len() >= MIN_STEPS;
    let short = match median_m {
        Some(m) if enough => judged
            .iter()
            .copied()
            .filter(|&i| lengths[i] < SHORT_SHARE * m)
            .collect(),
        _ => Vec::new(),
    };
    let regular = enough.then_some(alternate && short.is_empty());
    Steps {
        contacts,
        lengths,
        median_m,
        alternate,
        short,
        regular,
    }
}

fn travel(a: &Contact, b: &Contact) -> f64 {
    (b.root[0] - a.root[0]).hypot(b.root[1] - a.root[1])
}

fn length(a: &Contact, b: &Contact) -> f64 {
    let (dx, dy) = (b.at[0] - a.at[0], b.at[1] - a.at[1]);
    let (rx, ry) = (b.root[0] - a.root[0], b.root[1] - a.root[1]);
    let way = travel(a, b);
    if way < STILL_M {
        dx.hypot(dy)
    } else {
        (dx * rx + dy * ry) / way
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

/// Contacts with whether each landed inside the span -> one contact per place, in landing order: a
/// replant joins the contact before it, so one that began before the span stays out.
fn merged(mut raw: Vec<(Contact, bool)>) -> Vec<Contact> {
    raw.sort_by_key(|(c, _)| c.from);
    let mut out: Vec<(Contact, bool)> = Vec::with_capacity(raw.len());
    for (c, landed) in raw {
        match out.last_mut() {
            Some((p, _)) if p.foot == c.foot && (c.at[0] - p.at[0]).hypot(c.at[1] - p.at[1]) < REPLANT_M => {
                p.to = c.to;
            }
            _ => out.push((c, landed)),
        }
    }
    out.into_iter().filter(|(_, landed)| *landed).map(|(c, _)| c).collect()
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
    let mut joints = Vec::with_capacity(rots.len());
    for (r, o) in rots.iter().zip(&roots) {
        joints.push(sk.fk(r, *o).map_err(|e| e.to_string())?.1);
    }
    let phases = contact::clip_phases(&body, sk, &rots, &roots, doc.clip.fps, &doc.clip.surfaces)?;
    let mut raw = Vec::new();
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
            if from > b {
                continue;
            }
            let mid = ((from.max(a) + to.min(b)) / 2 - lo) as usize;
            let pos = joints.get(mid).ok_or("a contact's frame was not sampled")?;
            let (Some(p), Some(r)) = (pos.get(ankle), pos.first()) else {
                return Err("a contact's joints were not posed".into());
            };
            raw.push((
                Contact {
                    foot: name,
                    from,
                    to: to.min(b),
                    at: [p[0], p[1]],
                    root: [r[0], r[1]],
                },
                from > a,
            ));
        }
    }
    Ok(judge(merged(raw)))
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

    /// 0.5, 0.5, 0.5, then a last step of 0.1 that closes the feet as the root stops (0.05 m of
    /// travel): reported, not judged, and the median stays 0.5.
    #[test]
    fn a_step_while_stopping_is_not_judged() {
        let c = |foot, y: f64, ry: f64| Contact {
            foot,
            from: 0,
            to: 0,
            at: [0.0, y],
            root: [0.0, ry],
        };
        let s = judge(vec![
            c("L", 0.0, 0.0),
            c("R", -0.5, -0.5),
            c("L", -1.0, -1.0),
            c("R", -1.5, -1.5),
            c("L", -1.6, -1.55),
        ]);
        near(s.lengths[3], 0.1);
        near(s.median_m.unwrap(), 0.5);
        assert!(s.short.is_empty());
        assert_eq!(s.regular, Some(true));
    }

    /// The fill of `ai-fill-long-gap`: the right foot replanted 0.0039 m on before the first step
    /// joins one contact (frames 2-19, at y 0.015). Steps by y 0.4327, 0.5, -0.0142, 0.45, 0.6365:
    /// in order -0.0142, 0.4327, 0.45, 0.5, 0.6365, so a median of 0.45, half of it 0.225: the
    /// owner's half step alone is short, and the feet alternate.
    #[test]
    fn a_replant_is_one_contact_and_the_half_step_still_fails() {
        let c = |foot, from: i64, to: i64, y: f64, ry: f64| {
            let contact = Contact {
                foot,
                from,
                to,
                at: [0.0, y],
                root: [0.0, ry],
            };
            (contact, true)
        };
        let contacts = merged(vec![
            c("R", 2, 14, 0.015, 0.0),
            c("R", 16, 19, 0.0189, -0.1),
            c("L", 22, 30, -0.4177, -0.4),
            c("R", 32, 40, -0.9178, -0.7),
            c("L", 42, 50, -0.9036, -1.0),
            c("R", 52, 60, -1.3534, -1.3),
            c("L", 62, 70, -1.9899, -1.6),
        ]);
        assert_eq!((contacts[0].from, contacts[0].to), (2, 19));
        near(contacts[0].at[1], 0.015);
        let s = judge(contacts);
        assert_eq!(s.contacts.len(), 6);
        assert!(s.alternate);
        near(s.median_m.unwrap(), 0.45);
        assert_eq!(s.short, [2]);
        assert_eq!(s.regular, Some(false));
    }

    /// A foot down from before the span, lifted and put back in place, still was not stepped onto.
    /// A foot moved 0.1 m or more has stepped: the same foot twice, which doesn't alternate.
    #[test]
    fn a_replant_keeps_its_first_contact() {
        let c = |from: i64, y: f64| Contact {
            foot: "L",
            from,
            to: from + 5,
            at: [0.0, y],
            root: [0.0, 0.0],
        };
        assert!(merged(vec![(c(0, 0.0), false), (c(8, -0.05), true)]).is_empty());
        assert_eq!(merged(vec![(c(1, 0.0), true), (c(8, -0.1), true)]).len(), 2);
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
