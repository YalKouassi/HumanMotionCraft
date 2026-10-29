//! Viewing aids for the viewport: which frames the onion skin ghosts, and the path a joint takes
//! (a motion trail). Queries only: they read the keyed motion and change nothing.

use serde_json::{Map, Value, json};

use crate::command::{self as cmd, CommandSpec, Kind, ParamSpec};
use crate::document::Document;

/// Most ghosts on each side of the playhead.
pub const MAX_ONION: i64 = 5;
/// Most frames one trail covers.
pub const MAX_TRAIL: i64 = 2000;

pub static SPECS: &[CommandSpec] = &[
    CommandSpec::new("onion.frames", "Onion-skin frames", onion_frames)
        .icon("onion")
        .inputs(&[
            ParamSpec::new("frame", Kind::Number, "around this frame (default: the playhead)")
                .range(0.0, 1e7),
            ParamSpec::new("count", Kind::Integer, "ghosts on each side (default 2)")
                .range(1.0, MAX_ONION as f64),
            ParamSpec::new("step", Kind::Integer, "frames between ghosts (default 2)").range(1.0, 1000.0),
            ParamSpec::new(
                "mode",
                Kind::Choice(&["frames", "keys"]),
                "every `step` frames, or the nearest keys",
            ),
        ])
        .outputs(&[
            ParamSpec::new("before", Kind::IntList, "frames before, nearest first").required(),
            ParamSpec::new("after", Kind::IntList, "frames after, nearest first").required(),
            ParamSpec::new("note", Kind::Text, "why there is nothing to ghost"),
        ]),
    CommandSpec::new("trail.points", "Motion trail", trail_points)
        .icon("trail")
        .inputs(&[
            ParamSpec::new("bone", Kind::Bone, "the joint to trace").required(),
            ParamSpec::new("from", Kind::Integer, "first frame (default 0)").range(0.0, 1e7),
            ParamSpec::new("to", Kind::Integer, "last frame (default: the clip's last)").range(0.0, 1e7),
        ])
        .outputs(&[
            ParamSpec::new("frames", Kind::IntList, "every frame from `from` to `to`").required(),
            ParamSpec::new(
                "points",
                Kind::Vec3,
                "one [x, y, z] a frame: the joint's head (metres)",
            )
            .list()
            .required(),
            ParamSpec::new(
                "keys",
                Kind::IntList,
                "frames in the range with a key that sets the joint",
            )
            .required(),
        ]),
];

type P = Map<String, Value>;

fn int_or(p: &P, name: &str, default: i64) -> i64 {
    p.get(name).and_then(Value::as_i64).unwrap_or(default)
}

/// The base layer's key frames.
fn key_frames(doc: &Document) -> Vec<i64> {
    doc.clip.base().keys.iter().map(|k| k.frame).collect()
}

/// Frames to ghost around a frame: `count` each side, `step` apart and inside the clip, or the
/// nearest keys (a key on the playhead itself is the pose shown, not a ghost). An empty clip has
/// none, and says so.
fn onion_frames(doc: &mut Document, p: &P) -> Result<Value, String> {
    let frame = p.get("frame").and_then(Value::as_f64).unwrap_or(doc.frame);
    if !frame.is_finite() {
        return Err(format!("frame {frame} is not a number"));
    }
    let count = int_or(p, "count", 2).clamp(1, MAX_ONION) as usize;
    let step = int_or(p, "step", 2).max(1);
    let keys = key_frames(doc);
    if keys.is_empty() {
        return Ok(
            json!({ "before": [], "after": [], "note": "the animation has no keys: nothing to ghost" }),
        );
    }
    let here = frame.round() as i64;
    let (before, after): (Vec<i64>, Vec<i64>) = if cmd::text(p, "mode") == Some("keys") {
        (
            keys.iter()
                .rev()
                .copied()
                .filter(|&k| k < here)
                .take(count)
                .collect(),
            keys.iter().copied().filter(|&k| k > here).take(count).collect(),
        )
    } else {
        let last = (doc.clip.length_with(&doc.links) - 1).max(0);
        (
            (1..=count as i64)
                .map(|i| here - step * i)
                .filter(|f| *f >= 0)
                .collect(),
            (1..=count as i64)
                .map(|i| here + step * i)
                .filter(|f| *f <= last)
                .collect(),
        )
    };
    Ok(json!({ "before": before, "after": after }))
}

/// The keyed path of a joint over a range of frames, with the frames of the keys that set it.
fn trail_points(doc: &mut Document, p: &P) -> Result<Value, String> {
    let name = cmd::text(p, "bone").unwrap_or_default();
    let bone = doc.bone(name)?;
    if doc.clip.base().keys.is_empty() {
        return Err("the animation has no keys: there is no motion to trace".into());
    }
    let last = (doc.clip.length_with(&doc.links) - 1).max(0);
    let (from, to) = (int_or(p, "from", 0), int_or(p, "to", last));
    if from > to {
        return Err(format!("from {from} is after to {to}"));
    }
    if to - from + 1 > MAX_TRAIL {
        return Err(format!(
            "{} frames: a trail covers at most {MAX_TRAIL}",
            to - from + 1
        ));
    }
    let frames: Vec<i64> = (from..=to).collect();
    let at: Vec<f64> = frames.iter().map(|&f| f as f64).collect();
    let points: Vec<[f64; 3]> = doc
        .positions(&at)?
        .iter()
        .map(|joints| joints.get(bone).copied().ok_or("no such joint in the pose"))
        .collect::<Result<_, _>>()?;
    let keys: Vec<i64> = doc
        .clip
        .base()
        .keys
        .iter()
        .filter(|k| (from..=to).contains(&k.frame))
        .filter(|k| k.only.as_ref().is_none_or(|o| o.contains(&bone)))
        .map(|k| k.frame)
        .collect();
    Ok(json!({ "frames": frames, "points": points, "keys": keys }))
}
