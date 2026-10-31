//! What `posefill.fill` asks the model for: the keys of yours from A to B (every key between
//! them is a step of the chain), each gap checked against the model's range at its own frame
//! rate, the context poses before key A and the target poses from key B on, and the chain run
//! segment by segment, each later segment's context being the earlier fill's last frames.

use std::sync::Arc;

use ac_core::Document;
use ac_core::clip::{Clip, LayerMode, LayerSource};
use ac_core::edits::{self, Filler};
use ac_core::error::hinted;
use ac_core::quat::{Quat, Vec3};
use serde_json::{Map, Value};

use crate::engine::{Fill, Pose, Request};
use crate::{CONTEXT, MAX_FRAMES, MAX_TARGETS, MIN_FRAMES, MODEL_FPS};

type P = Map<String, Value>;

/// A fill planned: the keys of yours it goes through (A first, B last), and what to tell the user
/// before the model is asked.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub keys: Vec<i64>,
    pub fps: f64,
    pub rig: String,
    pub notes: Vec<String>,
}

impl Plan {
    /// (A, B) of each segment.
    pub fn segments(&self) -> Vec<(i64, i64)> {
        self.keys
            .windows(2)
            .filter_map(|w| match w {
                [a, b] => Some((*a, *b)),
                _ => None,
            })
            .collect()
    }

    pub fn from(&self) -> i64 {
        self.keys.first().copied().unwrap_or(0)
    }

    pub fn to(&self) -> i64 {
        self.keys.last().copied().unwrap_or(0)
    }
}

/// The rig file's whole path: the helper runs in its own folder, so a relative one wouldn't find it.
pub(crate) fn rig_path(assets: &std::path::Path) -> Result<String, String> {
    let path = std::path::absolute(assets.join("rig/default_rig.json")).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

/// The edited layer takes whole poses.
pub(crate) fn whole_layer(clip: &Clip) -> Result<(), String> {
    let l = clip.layers.get(clip.edit).ok_or("no layer to edit")?;
    if l.source != LayerSource::Keys || l.mode == LayerMode::Additive || l.only.is_some() {
        return Err(hinted(
            format!("layer {:?} doesn't take whole poses", l.name),
            "a pose fill writes the whole body: edit a full keys layer (the base layer is one)",
        ));
    }
    Ok(())
}

/// The model's range at `fps`, as frames of the animation ("18 to 58 frames (0.6 to 1.9 s)").
fn range_at(fps: f64) -> String {
    let (lo, hi) = (MIN_FRAMES - 1, MAX_FRAMES - 1);
    let at = |n: i64| ((n as f64) * fps / MODEL_FPS).round() as i64;
    format!(
        "{} to {} frames ({:.1} to {:.2} s)",
        at(lo),
        at(hi),
        lo as f64 / MODEL_FPS,
        hi as f64 / MODEL_FPS
    )
}

/// The gap `a`-`b` fits the model at `fps`.
fn fits(a: i64, b: i64, fps: f64) -> Result<(), String> {
    let frames = b - a;
    if frames < 2 {
        return Err(format!("{a} and {b} have no frames between them to fill"));
    }
    let n = ac_helper::frames::model_frames(frames + 1, fps, MODEL_FPS);
    let said = format!(
        "the gap {a}-{b} is {frames} frames ({:.2} s at {fps} fps): {n} frames at the model's {MODEL_FPS} fps, and it fills {}",
        frames as f64 / fps,
        range_at(fps)
    );
    if n > MAX_FRAMES {
        return Err(hinted(
            format!("{said} at most"),
            "put a key inside the gap to aim for (the fill then goes through it), or make the gap shorter",
        ));
    }
    if n < MIN_FRAMES {
        return Err(hinted(
            format!("{said} at least"),
            "make the gap longer, or fill it with the in-betweener (ai.fill)",
        ));
    }
    Ok(())
}

/// The frame rate's step between model frames, in frames of the animation.
fn step(fps: f64) -> f64 {
    fps / MODEL_FPS
}

/// The plan for `p`: `from` and `to` (keys of yours; every key between them is gone through), or
/// the keys around the playhead.
pub fn plan(doc: &Document, p: &P) -> Result<Plan, String> {
    whole_layer(&doc.clip)?;
    let (a, b) = match (
        p.get("from").and_then(Value::as_i64),
        p.get("to").and_then(Value::as_i64),
    ) {
        (None, None) => edits::span_around(&doc.clip, doc.key_frame())
            .ok_or("put the playhead between two of your keys, or give `from` and `to`")?,
        (Some(a), Some(b)) => (a, b),
        _ => return Err("give both `from` and `to`, or neither".into()),
    };
    let m = edits::manual_frames(&doc.clip);
    if !m.contains(&a) || !m.contains(&b) {
        return Err(format!("{a} and {b} are not both keys of yours"));
    }
    if a >= b {
        return Err(format!("`from` {a} must come before `to` {b}"));
    }
    let fps = doc.clip.fps;
    let keys: Vec<i64> = m.into_iter().filter(|&f| a <= f && f <= b).collect();
    for w in keys.windows(2) {
        if let [a, b] = w {
            fits(*a, *b, fps)?;
        }
    }
    let rig = rig_path(
        doc.assets
            .as_ref()
            .ok_or("no assets folder is set (start the app with one)")?,
    )?;
    let mut notes = Vec::new();
    let before = (CONTEXT - 1) as f64 * step(fps);
    if (a as f64) < before {
        notes.push(format!(
            "key {a} has fewer than {} frames before it: the first context pose was repeated to make the model's {CONTEXT}",
            before.ceil() as i64
        ));
    }
    Ok(Plan {
        keys,
        fps,
        rig,
        notes,
    })
}

/// The pose on frame `f` (fractions between keys), as the animation plays it.
fn pose_at(doc: &Document, f: f64) -> Result<Pose, String> {
    let (r, o) = doc.clip.sample_with(&[f], &doc.links)?;
    Ok(Pose {
        rotations: r.into_iter().next().ok_or("no pose")?,
        root: o.into_iter().next().ok_or("no pose")?,
    })
}

/// The [`CONTEXT`] poses ending on key `a`, a model frame apart; those before frame 0 are the
/// first one repeated.
fn context(doc: &Document, a: i64, fps: f64) -> Result<Vec<Pose>, String> {
    let s = step(fps);
    let mut poses = Vec::with_capacity(CONTEXT);
    for i in 0..CONTEXT {
        let t = a as f64 - (CONTEXT - 1 - i) as f64 * s;
        if t >= 0.0 {
            poses.push(pose_at(doc, t)?);
        }
    }
    let first = poses.first().cloned().ok_or("no pose before the key")?;
    while poses.len() < CONTEXT {
        poses.insert(0, first.clone());
    }
    Ok(poses)
}

/// Key `b`'s pose and, on the chain's last segment, up to [`MAX_TARGETS`] - 1 poses after it
/// where the animation goes on (a key of yours lies on or past them).
fn targets(doc: &Document, b: i64, fps: f64, last: bool) -> Result<Vec<Pose>, String> {
    let mut poses = vec![pose_at(doc, b as f64)?];
    if last {
        let end = edits::manual_frames(&doc.clip).last().copied().unwrap_or(b) as f64;
        let s = step(fps);
        for j in 1..MAX_TARGETS {
            let t = b as f64 + j as f64 * s;
            if t <= end {
                poses.push(pose_at(doc, t)?);
            }
        }
    }
    Ok(poses)
}

/// The last [`CONTEXT`] frames of a fill, as the next segment's context.
fn tail(fill: &Fill) -> Vec<Pose> {
    let n = fill.rotations.len();
    let from = n.saturating_sub(CONTEXT);
    fill.rotations
        .iter()
        .zip(&fill.roots)
        .skip(from)
        .map(|(r, o)| Pose {
            rotations: r.clone(),
            root: *o,
        })
        .collect()
}

/// Make every segment's fill in order, through `make` (the engine, or only what is ready).
pub fn run(
    doc: &Document,
    plan: &Plan,
    make: impl Fn(&Request) -> Result<Arc<Fill>, String>,
) -> Result<Vec<Arc<Fill>>, String> {
    let segments = plan.segments();
    let mut fills: Vec<Arc<Fill>> = Vec::with_capacity(segments.len());
    for (i, &(a, b)) in segments.iter().enumerate() {
        let last = i + 1 == segments.len();
        let context = match fills.last() {
            Some(prev) => tail(prev),
            None => context(doc, a, plan.fps)?,
        };
        let req = Request {
            rig: plan.rig.clone(),
            frames: usize::try_from(b - a + 1).map_err(|e| e.to_string())?,
            fps: plan.fps,
            context,
            targets: targets(doc, b, plan.fps, last)?,
        };
        fills.push(make(&req)?);
    }
    Ok(fills)
}

/// One segment's fill, handed to `edits::refill` (frames strictly between the keys).
pub struct Made<'a> {
    pub fill: &'a Fill,
    pub tag: String,
}

impl Filler for Made<'_> {
    fn fill(&self, _: &Clip, a: i64, b: i64) -> Result<(Vec<Vec<Quat>>, Vec<Vec3>, String), String> {
        let inner = usize::try_from(b - a - 1).map_err(|e| e.to_string())?;
        let rots = self
            .fill
            .rotations
            .get(1..=inner)
            .ok_or("the fill is too short")?;
        let roots = self.fill.roots.get(1..=inner).ok_or("the fill is too short")?;
        Ok((rots.to_vec(), roots.to_vec(), self.tag.clone()))
    }

    fn long(&self) -> bool {
        true
    }
}
