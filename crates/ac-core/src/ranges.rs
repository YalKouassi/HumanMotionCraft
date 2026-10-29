//! Coupled joint range-of-motion tables (`assets/rig/ranges.json`) and clip range measurements
//! (`clip.range`).
//!
//! A per-bone box (`assets/rig/limits.json`) gives each bone one fixed min and max on its own X, Y
//! and Z. A real joint's range shifts with its neighbours and its other axes, so each pair here
//! plots one anatomical angle (`crate::anatomy`) against another:
//!
//! - `hip_knee`, `ankle_knee`: the hamstrings and the calf cross two joints, so a straight knee
//!   leaves the hip and the ankle less room;
//! - `hip_swing`, `shoulder_swing`, `trunk_bend`, `neck_bend`: where a segment can point, forward
//!   and sideways at once.
//!
//! `assets/rig/ranges.json` (built by `python -m ac_train.ranges`) carries, per pair:
//!
//! - `hard` (and `x_hard` for the horizontal angle): published clinical limits, each number from a
//!   listed source (`limits_note` says which and what joining between them is ours);
//! - `typical`: the 1st–99th percentile band of the recorded mocap's clean frames;
//! - `grid`: a 24 × 24 log-scaled density of those frames, for the Range panel's cloud;
//!
//! and `checks`: poses with the angles the Python builder measured, which the tests recompute.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::anatomy::{self, Joint};
use crate::document::{Document, Pose};
use crate::metrics::{LIMIT_SLACK_DEG, MAX_FRAMES};
use crate::product;
use crate::quat::Quat;
use crate::rig::Skeleton;

type R<T> = Result<T, String>;

/// Pair ids built into `assets/rig/ranges.json`.
pub const PAIR_IDS: &[&str] = &[
    "hip_knee",
    "ankle_knee",
    "hip_swing",
    "shoulder_swing",
    "trunk_bend",
    "neck_bend",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub title: String,
    pub url: String,
    pub note: String,
}

/// One anatomical angle: a joint (`anatomy::JOINTS`) and its component (`anatomy::COMPONENTS`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AxisSpec {
    pub joint: String,
    pub component: String,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BandPoint {
    pub x: f64,
    pub lo: f64,
    pub hi: f64,
    #[serde(default)]
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Grid {
    pub x_range: [f64; 2],
    pub y_range: [f64; 2],
    pub cols: usize,
    pub rows: usize,
    /// Rows bottom (lowest y) first, each left (lowest x) first.
    pub cells: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairSpec {
    pub id: String,
    pub label: String,
    pub sided: bool,
    pub note: String,
    #[serde(default)]
    pub limits_note: String,
    pub x: AxisSpec,
    pub y: AxisSpec,
    /// The clinical range of the horizontal angle (`None`: no published limit).
    #[serde(default)]
    pub x_hard: Option<[f64; 2]>,
    pub hard: Vec<BandPoint>,
    pub typical: Vec<BandPoint>,
    pub grid: Grid,
}

/// A pose and the angles the builder measured on it (`"hip.L"`, `"trunk"`: [flexion, abduction,
/// rotation]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Check {
    pub rotations: Vec<Quat>,
    pub angles: BTreeMap<String, [f64; 3]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ranges {
    pub format: String,
    pub version: u32,
    pub space: String,
    pub sets: Vec<String>,
    pub frames: u64,
    pub takes: u64,
    pub sources: Vec<Source>,
    pub pairs: Vec<PairSpec>,
    #[serde(default)]
    pub checks: Vec<Check>,
}

/// Linearly interpolate `(lo, hi)` of a sorted `[BandPoint]` curve at `x` (clamped at the ends).
pub fn interp_band(pts: &[BandPoint], x: f64) -> Option<(f64, f64)> {
    let first = pts.first()?;
    let last = pts.last()?;
    if x <= first.x {
        return Some((first.lo, first.hi));
    }
    if x >= last.x {
        return Some((last.lo, last.hi));
    }
    for w in pts.windows(2) {
        let [a, b] = w else { continue };
        if x >= a.x && x <= b.x {
            let span = (b.x - a.x).max(1e-12);
            let t = ((x - a.x) / span).clamp(0.0, 1.0);
            return Some((a.lo + (b.lo - a.lo) * t, a.hi + (b.hi - a.hi) * t));
        }
    }
    Some((last.lo, last.hi))
}

impl AxisSpec {
    fn index(&self) -> R<usize> {
        anatomy::component(&self.component)
    }
}

impl Ranges {
    pub fn from_json(text: &str, sk: &Skeleton) -> R<Ranges> {
        let r: Ranges = serde_json::from_str(text).map_err(|e| format!("ranges: {e}"))?;
        if !product::is_format(&r.format, "ranges") {
            return Err(format!("ranges: expected <product>.ranges, got {:?}", r.format));
        }
        if r.version != 2 {
            return Err(format!(
                "ranges: version {} (this build reads version 2; rebuild with python -m ac_train.ranges)",
                r.version
            ));
        }
        if r.pairs.is_empty() {
            return Err("ranges: no pairs".into());
        }
        for p in &r.pairs {
            for ax in [&p.x, &p.y] {
                ax.index().map_err(|e| format!("ranges: {}: {e}", p.id))?;
                for side in sides(p.sided) {
                    Joint::new(sk, &ax.joint, side).map_err(|e| format!("ranges: {}: {e}", p.id))?;
                }
            }
            let sorted = p.hard.windows(2).all(|w| matches!(w, [a, b] if a.x <= b.x));
            if p.hard.is_empty() || !sorted || p.hard.iter().any(|b| !ordered(b.lo, b.hi)) {
                return Err(format!(
                    "ranges: {}: the hard band must be sorted with lo <= hi",
                    p.id
                ));
            }
            if p.x_hard.is_some_and(|[lo, hi]| !ordered(lo, hi)) {
                return Err(format!("ranges: {}: x_hard has lo above hi", p.id));
            }
        }
        Ok(r)
    }

    pub fn pair(&self, id: &str) -> R<&PairSpec> {
        self.pairs.iter().find(|p| p.id == id).ok_or_else(|| {
            let names: Vec<&str> = self.pairs.iter().map(|p| p.id.as_str()).collect();
            let near = crate::suggest::closest(id, names.iter().copied());
            let mut m = format!("no range pair {id:?} (available: {})", names.join(", "));
            if !near.is_empty() {
                m.push_str("; ");
                m.push_str(&crate::suggest::did_you_mean(&near));
            }
            m
        })
    }
}

/// `lo <= hi`, and neither is NaN.
fn ordered(lo: f64, hi: f64) -> bool {
    matches!(
        lo.partial_cmp(&hi),
        Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
    )
}

fn sides(sided: bool) -> &'static [&'static str] {
    if sided { &["L", "R"] } else { &["L"] }
}

/// A pair's two angles resolved on a rig for one side.
pub struct Probe {
    x: (Joint, usize),
    y: (Joint, usize),
}

impl Probe {
    pub fn new(sk: &Skeleton, spec: &PairSpec, side: &str) -> R<Probe> {
        let side = if spec.sided { side } else { "L" };
        Ok(Probe {
            x: (Joint::new(sk, &spec.x.joint, side)?, spec.x.index()?),
            y: (Joint::new(sk, &spec.y.joint, side)?, spec.y.index()?),
        })
    }

    /// The pair's (x, y) degrees on a pose's local rotations.
    pub fn point(&self, sk: &Skeleton, rotations: &[Quat]) -> R<(f64, f64)> {
        let x = self.x.0.angles(sk, rotations)?;
        let y = self.y.0.angles(sk, rotations)?;
        Ok((
            x.get(self.x.1).copied().unwrap_or(0.0),
            y.get(self.y.1).copied().unwrap_or(0.0),
        ))
    }
}

/// Where a point sits against a pair's limits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Judged {
    pub hard_lo: f64,
    pub hard_hi: f64,
    pub typical_lo: f64,
    pub typical_hi: f64,
    /// `0` in the middle of the clinical range, `1` on its edge, `> 1` past it (either angle).
    pub closeness: f64,
    pub typical_closeness: f64,
    /// Degrees past the clinical limit (either angle) and past the typical band.
    pub past_hard: f64,
    pub past_typical: f64,
}

/// How close `v` is to `[lo, hi]`: `0` at the midpoint, `1` on either edge, `> 1` outside.
fn closeness(v: f64, lo: f64, hi: f64) -> (f64, f64) {
    let mid = 0.5 * (lo + hi);
    let half = (0.5 * (hi - lo)).max(1e-9);
    ((v - mid).abs() / half, (lo - v).max(v - hi).max(0.0))
}

pub fn judge(spec: &PairSpec, x: f64, y: f64) -> Judged {
    let (h_lo, h_hi) = interp_band(&spec.hard, x).unwrap_or((-180.0, 180.0));
    let (t_lo, t_hi) = interp_band(&spec.typical, x).unwrap_or((h_lo, h_hi));
    let (mut c, mut past) = closeness(y, h_lo, h_hi);
    if let Some([lo, hi]) = spec.x_hard {
        let (cx, px) = closeness(x, lo, hi);
        c = c.max(cx);
        past = past.max(px);
    }
    let (tc, tp) = closeness(y, t_lo, t_hi);
    Judged {
        hard_lo: h_lo,
        hard_hi: h_hi,
        typical_lo: t_lo,
        typical_hi: t_hi,
        closeness: c,
        typical_closeness: tc,
        past_hard: past,
        past_typical: tp,
    }
}

fn r4(x: f64) -> f64 {
    (x * 1e4).round() / 1e4
}

#[derive(Debug, Default)]
struct SpanAcc {
    frames: Vec<i64>,
    worst: f64,
    worst_frame: Option<i64>,
}

impl SpanAcc {
    fn add(&mut self, frame: i64, amount: f64) {
        self.frames.push(frame);
        if self.worst_frame.is_none() || amount > self.worst {
            self.worst = amount;
            self.worst_frame = Some(frame);
        }
    }

    fn runs(&self) -> Vec<[i64; 2]> {
        let mut out: Vec<[i64; 2]> = Vec::new();
        for &f in &self.frames {
            match out.last_mut() {
                Some(r) if r[1] + 1 == f => r[1] = f,
                _ => out.push([f, f]),
            }
        }
        out
    }

    fn json(&self) -> Value {
        json!({
            "frames": self.frames.len(),
            "spans": self.runs(),
            "max_deg": r4(self.worst),
            "worst_frame": self.worst_frame,
        })
    }
}

fn axis_json(ax: &AxisSpec, hard: Option<[f64; 2]>) -> Value {
    json!({ "joint": ax.joint, "component": ax.component, "label": ax.label, "hard": hard })
}

/// The poses `clip.range` measures: frames `from..=to` of the clip, or the pose shown when the
/// clip has no keys.
fn poses(
    doc: &Document,
    from: Option<i64>,
    to: Option<i64>,
    shown: bool,
    notes: &mut Vec<String>,
) -> R<(i64, i64, Vec<Pose>)> {
    if doc.clip.layers.iter().all(|l| l.keys.is_empty()) {
        let f = doc.frame.round() as i64;
        notes.push("the animation has no keys: measuring the pose shown alone".into());
        return Ok((f, f, vec![doc.pose.clone()]));
    }
    let last = (doc.clip.length_with(&doc.links) - 1).max(0);
    let (a, b) = (from.unwrap_or(0), to.unwrap_or(last));
    if a < 0 || b < a {
        return Err(format!("the range {a}..{b} is empty"));
    }
    if b > last {
        return Err(crate::error::hinted(
            format!("frame {b} is past the end of the animation (its last frame is {last})"),
            format!("measure up to frame {last}"),
        ));
    }
    if b - a + 1 > MAX_FRAMES {
        return Err(format!("{} frames is more than {MAX_FRAMES} at once", b - a + 1));
    }
    let fr: Vec<f64> = (a..=b).map(|f| f as f64).collect();
    let p = if shown {
        if !doc.physics_shown() {
            notes.push("the physics pass is off: the motion shown is the keys".into());
        }
        doc.shown_poses(&fr)?
    } else {
        let (r, o) = doc.clip.sample_with(&fr, &doc.links)?;
        r.into_iter()
            .zip(o)
            .map(|(rotations, root)| Pose { rotations, root })
            .collect()
    };
    Ok((a, b, p))
}

/// Measure coupled range of motion on `doc` for `pair_id` and `side` (`"L"` or `"R"`), plus a
/// summary of every pair and side over the same frames.
pub fn measure(
    doc: &Document,
    pair_id: Option<&str>,
    side_arg: Option<&str>,
    from: Option<i64>,
    to: Option<i64>,
    shown: bool,
) -> R<Value> {
    let Some(ranges) = doc.ranges.as_deref() else {
        return Err(crate::error::hinted(
            "the range-of-motion tables are not loaded (assets/rig/ranges.json)",
            "build them with python -m ac_train.ranges, or start with --assets <dir>",
        ));
    };
    let spec = ranges.pair(pair_id.unwrap_or("hip_knee"))?;
    let side = match side_arg {
        Some("L" | "l" | "left") | None => "L",
        Some("R" | "r" | "right") => "R",
        Some(other) => return Err(format!("side must be \"L\" or \"R\", got {other:?}")),
    };
    let side = if spec.sided { side } else { "L" };
    let mut notes = Vec::new();
    let (a, b, poses) = poses(doc, from, to, shown, &mut notes)?;
    let sk = &doc.skeleton;

    let probe = Probe::new(sk, spec, side)?;
    let mut path = Vec::with_capacity(poses.len());
    let (mut hard, mut typical) = (SpanAcc::default(), SpanAcc::default());
    let mut max_closeness = 0.0_f64;
    for (f, pose) in (a..).zip(&poses) {
        let (x, y) = probe.point(sk, &pose.rotations)?;
        let j = judge(spec, x, y);
        max_closeness = max_closeness.max(j.closeness);
        if j.past_hard > LIMIT_SLACK_DEG {
            hard.add(f, j.past_hard);
        }
        if j.past_typical > LIMIT_SLACK_DEG {
            typical.add(f, j.past_typical);
        }
        path.push(json!({
            "frame": f,
            "x": r4(x),
            "y": r4(y),
            "hard_lo": r4(j.hard_lo),
            "hard_hi": r4(j.hard_hi),
            "typical_lo": r4(j.typical_lo),
            "typical_hi": r4(j.typical_hi),
            "closeness": r4(j.closeness),
            "typical_closeness": r4(j.typical_closeness),
            "past_hard_deg": r4(j.past_hard),
            "past_typical_deg": r4(j.past_typical),
        }));
    }

    let (cx, cy) = probe.point(sk, &doc.pose.rotations)?;
    let cj = judge(spec, cx, cy);

    // Every pair and side over the same frames, so an extreme shows without checking each pair.
    let mut summary = Vec::new();
    for p in &ranges.pairs {
        for &s in sides(p.sided) {
            let probe = Probe::new(sk, p, s)?;
            let mut sp = SpanAcc::default();
            let (mut peak, mut peak_frame) = (0.0_f64, a);
            for (f, pose) in (a..).zip(&poses) {
                let (x, y) = probe.point(sk, &pose.rotations)?;
                let j = judge(p, x, y);
                if j.closeness > peak {
                    peak = j.closeness;
                    peak_frame = f;
                }
                if j.past_hard > LIMIT_SLACK_DEG {
                    sp.add(f, j.past_hard);
                }
            }
            summary.push(json!({
                "pair": p.id,
                "label": p.label,
                "side": if p.sided { Value::from(s) } else { Value::Null },
                "max_closeness": r4(peak),
                "peak_frame": peak_frame,
                "outside_hard_frames": sp.frames.len(),
                "outside_hard_spans": sp.runs(),
                "max_past_hard_deg": r4(sp.worst),
                "worst_frame": sp.worst_frame,
            }));
        }
    }

    Ok(json!({
        "pair": spec.id,
        "label": spec.label,
        "sided": spec.sided,
        "side": if spec.sided { Value::from(side) } else { Value::Null },
        "note": spec.note,
        "limits_note": spec.limits_note,
        "from": a,
        "to": b,
        "frames": b - a + 1,
        "motion": if shown { "shown" } else { "keys" },
        "x": axis_json(&spec.x, spec.x_hard),
        "y": axis_json(&spec.y, None),
        "hard": spec.hard,
        "typical": spec.typical,
        "playhead": {
            "frame": doc.frame,
            "x": r4(cx),
            "y": r4(cy),
            "hard_lo": r4(cj.hard_lo),
            "hard_hi": r4(cj.hard_hi),
            "typical_lo": r4(cj.typical_lo),
            "typical_hi": r4(cj.typical_hi),
            "closeness": r4(cj.closeness),
            "past_hard_deg": r4(cj.past_hard),
            "past_typical_deg": r4(cj.past_typical),
        },
        "path": path,
        "max_closeness": r4(max_closeness),
        "outside_hard": hard.json(),
        "outside_typical": typical.json(),
        "summary": summary,
        "sources": ranges.sources,
        "notes": notes,
    }))
}

/// Check every [`Check`] pose: the largest difference (degrees) between the angles the builder
/// wrote and the ones `crate::anatomy` measures, and where.
pub fn check_drift(r: &Ranges, sk: &Skeleton) -> R<(f64, String)> {
    let mut worst = (0.0_f64, String::new());
    for (n, c) in r.checks.iter().enumerate() {
        for (key, want) in &c.angles {
            let (joint, side) = key.split_once('.').unwrap_or((key.as_str(), "L"));
            let got = Joint::new(sk, joint, side)?.angles(sk, &c.rotations)?;
            for (k, (g, w)) in got.iter().zip(want).enumerate() {
                let d = (g - w).abs();
                if d > worst.0 || !d.is_finite() {
                    worst = (d, format!("check {n} {key} component {k}: {g} vs {w}"));
                }
            }
        }
    }
    Ok(worst)
}
