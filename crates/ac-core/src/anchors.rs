//! Anchor poses inside a long AI fill: a gap over the model's limit ([`crate::edits::MAX_FILL`])
//! is split into spans of about [`TARGET`] frames by poses borrowed from the library, and each
//! span is filled by the model as a gap of its own ([`crate::ai::AiFiller`] chains them).
//!
//! The rule, deterministic by construction:
//! - **frames**: evenly spaced between the two keys, the fewest that keep every span at or under
//!   [`TARGET`] frames key to key (keys 0 and 90: anchors on 30 and 60), rounded to whole frames;
//! - **the motion**: of every library animation made for this rig, every start frame `f` and
//!   every speed in [`SPEEDS`] (library frames per animation frame), the segment whose pose on
//!   `f` is nearest the first key and whose pose `speed * (b - a)` frames later is nearest the
//!   second: the mean angle between the two poses' local rotations over the model's bones, the
//!   segment turned so its first pose faces the first key's way (so a walk captured facing +X
//!   fits keys facing -Y, and a turn between the keys is matched by a turn in the library). A
//!   looping animation wraps. Ties go to the first library id (`natural_cmp` order), then the
//!   lower start frame, then the slower speed;
//! - **the poses**: the segment's pose on `f + speed * (frame - a)` for each anchor frame, turned
//!   the same way; its root on the keys' own interpolation at that frame (the spline through the
//!   two keys, AI keys taken out) plus the segment's own sway from its chord, so the body still
//!   bobs and weaves as it was captured and lands on both keys.
//!
//! Anchors land as AI keys tagged [`ANCHOR`] `<id> @<frame>` ([`tag`]), so an edit treats them
//! like the fill around them (cleared and made again with it), the timeline shows where they
//! came from, and the ease at a key between two fills ([`crate::ease`]) goes through them.

use crate::clip::{Clip, Links, NoLinks};
use crate::document::Pose;
use crate::posesource::PoseSource;
use crate::quat::{self, Quat, Vec3};
use crate::rig::Skeleton;

/// The model tag prefix of an anchor key: `"anchor Locomotion/Walk 12 @40"`.
pub const ANCHOR: &str = "anchor";
/// Frames key to key the spans between anchors aim for (under the model's limit, where it was
/// trained most).
pub const TARGET: i64 = 30;
/// Most frames between two keys a long fill takes (one minute at 30 fps): past it, key a pose.
pub const MAX_LONG: i64 = 1800;
/// Library frames per animation frame a segment may be played at (1: as captured).
pub const SPEEDS: [f64; 5] = [0.5, 0.75, 1.0, 1.5, 2.0];
/// Most frames of one library clip looked at (a longer one is cut here).
pub const MAX_CLIP_FRAMES: i64 = 3000;

type R<T> = Result<T, String>;

/// An anchor pose placed inside a long fill: where, and where it came from.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Anchor {
    pub frame: i64,
    /// The library animation (`<group>/<name>`).
    pub id: String,
    /// The frame of that animation the pose is.
    pub source_frame: i64,
}

/// The library segment the anchors were taken from.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Segment {
    pub id: String,
    /// Its frame matched to the first key, and to the second.
    pub from: i64,
    pub to: i64,
    /// Library frames per animation frame.
    pub speed: f64,
    /// The mean angle (degrees) between its two end poses and the two keys.
    pub match_deg: f64,
}

/// The anchors of a gap and the poses to key on them, in frame order.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub anchors: Vec<Anchor>,
    pub poses: Vec<Pose>,
    pub segment: Segment,
    /// What was left out and why (clips that could not be read or fit another rig).
    pub notes: Vec<String>,
}

/// Whether an AI key's model tag says it is an anchor pose.
pub fn is_anchor(model: &str) -> bool {
    model
        .strip_prefix(ANCHOR)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
}

/// The model tag of an anchor key.
pub fn tag(a: &Anchor) -> String {
    format!("{ANCHOR} {} @{}", a.id, a.source_frame)
}

/// The anchor frames strictly between keys `a` and `b`: none when the model takes the gap in
/// one (`max_gap` frames inside), else the fewest evenly spaced ones that keep every span at or
/// under [`TARGET`] frames key to key (and under `max_gap` frames inside).
pub fn frames(a: i64, b: i64, max_gap: i64) -> Vec<i64> {
    let len = b - a;
    let target = TARGET.min(max_gap).max(2);
    if len - 1 <= max_gap || len <= target {
        return Vec::new();
    }
    let spans = (len + target - 1) / target;
    (1..spans)
        .map(|i| a + (2 * i * len + spans) / (2 * spans))
        .collect()
}

/// The body's facing: the root's +X about Z (0 at rest).
fn heading(root: Quat) -> f64 {
    let m = quat::to_mat3(root);
    m[1][0].atan2(m[0][0])
}

fn yaw(angle: f64) -> Quat {
    let (s, c) = (angle / 2.0).sin_cos();
    [0.0, 0.0, s, c]
}

/// `v` turned about Z by `angle`.
fn turn(v: Vec3, angle: f64) -> Vec3 {
    let (s, c) = angle.sin_cos();
    [c * v[0] - s * v[1], s * v[0] + c * v[1], v[2]]
}

/// The angle (radians) between two rotations.
fn angle(p: Quat, q: Quat) -> f64 {
    let dot = (p[0] * q[0] + p[1] * q[1] + p[2] * q[2] + p[3] * q[3])
        .abs()
        .min(1.0);
    2.0 * dot.acos()
}

/// A library clip sampled on every whole frame.
struct Sampled<'a> {
    id: &'a str,
    rots: Vec<Vec<Quat>>,
    roots: Vec<Vec3>,
    loops: bool,
}

impl Sampled<'_> {
    fn len(&self) -> i64 {
        self.rots.len() as i64
    }

    /// The frame to read for `f` (a loop wraps; `None`: past the end).
    fn at(&self, f: i64) -> Option<usize> {
        let n = self.len();
        if n == 0 || f < 0 {
            return None;
        }
        if f < n {
            Some(f as usize)
        } else if self.loops {
            Some((f % n) as usize)
        } else {
            None
        }
    }
}

/// The mean angle between a key's rotations and a sampled pose's, the latter turned by `yaw0`
/// at the root, over `bones` (the root index `root`).
fn distance(bones: &[usize], root: Option<usize>, key: &[Quat], pose: &[Quat], yaw0: f64) -> Option<f64> {
    let mut sum = 0.0;
    let mut n = 0usize;
    for &i in bones {
        let (Some(&k), Some(&p)) = (key.get(i), pose.get(i)) else {
            continue;
        };
        let p = if Some(i) == root {
            quat::normalize(quat::mul(yaw(yaw0), p))
        } else {
            p
        };
        sum += angle(k, p);
        n += 1;
    }
    (n > 0).then(|| sum / n as f64)
}

/// Anchor frames and poses for the gap `span` (`a..b`) of `clip` (its AI keys already cleared,
/// so its sample is the keys' own spline), matching over `bones` (the model's).
pub fn plan(
    sk: &Skeleton,
    clip: &Clip,
    links: &dyn Links,
    bones: &[usize],
    source: &dyn PoseSource,
    span: (i64, i64),
    max_gap: i64,
) -> R<Plan> {
    let (a, b) = span;
    let len = b - a;
    if len - 1 > MAX_LONG {
        return Err(crate::error::hinted(
            format!(
                "the gap {a}–{b} holds {} frames, more than a long fill takes ({MAX_LONG})",
                len - 1
            ),
            "key a pose inside the gap and fill each side",
        ));
    }
    if bones.is_empty() {
        return Err("no bones to match anchor poses over".into());
    }
    let root = sk.bone("root");
    let sourced = source.clips()?;
    let mut notes = sourced.notes.clone();
    let mut sampled: Vec<Sampled> = Vec::new();
    let mut other_rig = 0usize;
    for sc in &sourced.clips {
        if sc.clip.bones != sk.names {
            other_rig += 1;
            continue;
        }
        let n = sc.clip.length_with(&NoLinks).clamp(0, MAX_CLIP_FRAMES);
        if n < 2 {
            continue;
        }
        let fr: Vec<f64> = (0..n).map(|f| f as f64).collect();
        let (rots, roots) = sc.clip.sample_with(&fr, &NoLinks)?;
        sampled.push(Sampled {
            id: &sc.id,
            rots,
            roots,
            loops: sc.clip.loop_,
        });
    }
    if other_rig > 0 {
        notes.push(format!(
            "{other_rig} library animation{} for another rig left out of the anchor poses",
            if other_rig == 1 { "" } else { "s" }
        ));
    }
    if sampled.is_empty() {
        return Err(crate::error::hinted(
            format!(
                "no library animation made for this rig to take anchor poses from ({})",
                source.name()
            ),
            "key a pose inside the gap, or import animations into the library",
        ));
    }
    // the two keys
    let (keys, _) = clip.sample_with(&[a as f64, b as f64], links)?;
    let (Some(ka), Some(kb)) = (keys.first(), keys.get(1)) else {
        return Err("the keys could not be sampled".into());
    };
    let ha = root.and_then(|i| ka.get(i).copied()).map_or(0.0, heading);
    // the best segment: (cost, clip, f, speed index)
    let mut best: Option<(f64, usize, i64, usize)> = None;
    for (ci, s) in sampled.iter().enumerate() {
        for f in 0..s.len() {
            let Some(fi) = s.at(f) else { continue };
            let yaw0 = ha - root.and_then(|i| s.rots[fi].get(i).copied()).map_or(0.0, heading);
            let Some(da) = distance(bones, root, ka, &s.rots[fi], yaw0) else {
                continue;
            };
            for (si, &speed) in SPEEDS.iter().enumerate() {
                let g = f + (speed * len as f64).round() as i64;
                let Some(gi) = s.at(g) else { continue };
                let Some(db) = distance(bones, root, kb, &s.rots[gi], yaw0) else {
                    continue;
                };
                let cost = (da + db) / 2.0;
                if best.is_none_or(|(c, _, _, _)| cost < c) {
                    best = Some((cost, ci, f, si));
                }
            }
        }
    }
    let Some((cost, ci, f, si)) = best else {
        return Err(crate::error::hinted(
            format!(
                "no library animation is long enough to span {len} frames (even at {}x speed)",
                SPEEDS.first().copied().unwrap_or(1.0)
            ),
            "key a pose inside the gap, or import a longer animation into the library",
        ));
    };
    let s = sampled.get(ci).ok_or("segment index")?;
    let speed = SPEEDS.get(si).copied().unwrap_or(1.0);
    let g = f + (speed * len as f64).round() as i64;
    let (fi, gi) = (s.at(f).ok_or("segment start")?, s.at(g).ok_or("segment end")?);
    let yaw0 = ha - root.and_then(|i| s.rots[fi].get(i).copied()).map_or(0.0, heading);
    let (root_f, root_g) = (s.roots[fi], s.roots[gi]);
    let frames = frames(a, b, max_gap);
    let at: Vec<f64> = frames.iter().map(|&t| t as f64).collect();
    let (_, path) = clip.sample_with(&at, links)?;
    let mut anchors = Vec::with_capacity(frames.len());
    let mut poses = Vec::with_capacity(frames.len());
    for (&t, &on_path) in frames.iter().zip(&path) {
        let src = f + (speed * (t - a) as f64).round() as i64;
        let i = s
            .at(src)
            .ok_or_else(|| format!("segment frame {src} is past the end"))?;
        let mut rotations = s.rots[i].clone();
        if let Some(r) = root
            && let Some(slot) = rotations.get_mut(r)
        {
            *slot = quat::normalize(quat::mul(yaw(yaw0), *slot));
        }
        // the segment's sway from its own chord, turned the keys' way, on top of the keys' path
        let u = (t - a) as f64 / len as f64;
        let chord = quat::add3(root_f, quat::scale3(quat::sub3(root_g, root_f), u));
        let sway = turn(quat::sub3(s.roots[i], chord), yaw0);
        anchors.push(Anchor {
            frame: t,
            id: s.id.to_string(),
            source_frame: src,
        });
        poses.push(Pose {
            rotations,
            root: quat::add3(on_path, sway),
        });
    }
    Ok(Plan {
        anchors,
        poses,
        segment: Segment {
            id: s.id.to_string(),
            from: f,
            to: g,
            speed,
            match_deg: (cost.to_degrees() * 10.0).round() / 10.0,
        },
        notes,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn anchors_split_a_gap_evenly_and_only_when_it_is_long() {
        assert_eq!(frames(0, 90, 45), vec![30, 60]);
        assert_eq!(frames(0, 46, 45), Vec::<i64>::new());
        assert_eq!(frames(0, 47, 45), vec![24]);
        assert_eq!(frames(10, 100, 45), vec![40, 70]);
        assert_eq!(frames(0, 91, 45), vec![23, 46, 68]);
        for (a, b) in [(0, 47), (0, 90), (0, 91), (5, 300), (0, 1801)] {
            let f = frames(a, b, 45);
            let mut bounds = vec![a];
            bounds.extend(&f);
            bounds.push(b);
            for w in bounds.windows(2) {
                assert!(w[1] - w[0] <= TARGET, "{a}-{b}: {f:?}");
                assert!(w[1] - w[0] >= 2, "{a}-{b}: {f:?}");
            }
        }
        // a model that takes less than TARGET frames keeps its spans inside its limit
        let f = frames(0, 90, 20);
        let mut bounds = vec![0];
        bounds.extend(&f);
        bounds.push(90);
        for w in bounds.windows(2) {
            assert!(w[1] - w[0] - 1 <= 20);
        }
    }

    #[test]
    fn anchor_tags_are_their_own() {
        let a = Anchor {
            frame: 30,
            id: "Locomotion/Walk".into(),
            source_frame: 8,
        };
        assert_eq!(tag(&a), "anchor Locomotion/Walk @8");
        assert!(is_anchor(&tag(&a)));
        assert!(is_anchor("anchor"));
        assert!(!is_anchor("anchored"));
        assert!(!is_anchor("parts walk"));
        assert!(!is_anchor("text A person waves."));
    }

    #[test]
    fn a_turned_pose_is_the_same_pose_facing_another_way() {
        let q = quat::from_euler([10.0, 0.0, 0.0]);
        let turned = quat::normalize(quat::mul(yaw(1.0), q));
        assert!((heading(turned) - heading(q) - 1.0).abs() < 1e-9);
        assert!(angle(q, q) < 1e-9);
        assert!((angle(q, quat::from_euler([40.0, 0.0, 0.0])) - 30f64.to_radians()).abs() < 1e-9);
        // matching turns the candidate's root back onto the key's: zero distance for a turned copy
        let key = vec![q, quat::from_euler([0.0, 20.0, 0.0])];
        let cand = vec![turned, quat::from_euler([0.0, 20.0, 0.0])];
        let d = distance(&[0, 1], Some(0), &key, &cand, -1.0).unwrap();
        assert!(d < 1e-6, "{d}");
        let v = turn([1.0, 0.0, 0.5], std::f64::consts::FRAC_PI_2);
        assert!((v[0]).abs() < 1e-12 && (v[1] - 1.0).abs() < 1e-12 && v[2] == 0.5);
    }

    #[test]
    fn a_loop_wraps_and_a_clip_ends() {
        let s = Sampled {
            id: "x",
            rots: vec![vec![quat::IDENTITY]; 10],
            roots: vec![[0.0; 3]; 10],
            loops: true,
        };
        assert_eq!(s.at(12), Some(2));
        assert_eq!(s.at(-1), None);
        let s = Sampled { loops: false, ..s };
        assert_eq!(s.at(12), None);
        assert_eq!(s.at(9), Some(9));
    }
}
