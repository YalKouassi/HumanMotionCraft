//! Moving the body in the scene (the viewport's Move tool): one frame (`pose.move`) or the whole
//! animation (`clip.move`).

use serde_json::{Map, Value, json};

use crate::command::{self as cmd, CommandSpec, Kind, ParamSpec};
use crate::document::Document;
use crate::placement::Placement;

/// A slide of more than a kilometre is a typo.
const MAX_SLIDE: f64 = 1e3;

/// The inputs both moves take.
const MOVE: &[ParamSpec] = &[
    ParamSpec::new(
        "by",
        Kind::Vec3,
        "slide, metres (-Y is the way the rig faces at rest)",
    ),
    ParamSpec::new(
        "turn",
        Kind::Number,
        "turn about the vertical, degrees (counter-clockwise from above)",
    )
    .range(-360.0, 360.0),
    ParamSpec::new(
        "pivot",
        Kind::Vec3,
        "the turn's centre, metres (z unused; default: the root on the frame shown)",
    ),
];

pub static SPECS: &[CommandSpec] = &[
    CommandSpec::new("pose.move", "Move body on this frame", pose_move)
        .icon("move")
        .undoable()
        .inputs(MOVE)
        .outputs(&[
            ParamSpec::new("frame", Kind::Integer, "the frame moved"),
            ParamSpec::new(
                "keyed",
                Kind::Bool,
                "its key moved too (false: no key there, the move is an edit not keyed yet)",
            ),
            ParamSpec::new("notes", Kind::TextList, "what the user should know"),
        ]),
    CommandSpec::new("clip.move", "Move animation", clip_move)
        .icon("move")
        .undoable()
        .inputs(MOVE)
        .outputs(&[
            ParamSpec::new("keys", Kind::Integer, "keys moved"),
            ParamSpec::new("notes", Kind::TextList, "what stayed where it was"),
        ]),
];

/// `by`, `turn` and `pivot` as a placement (the pivot defaults to the root on the frame shown).
fn placement(doc: &mut Document, p: &Map<String, Value>) -> Result<Placement, String> {
    let by = match p.get("by") {
        None => [0.0; 3],
        Some(_) => cmd::vec3(p, "by").ok_or("`by` must be [x, y, z] metres")?,
    };
    if !by.iter().all(|x| x.is_finite() && x.abs() <= MAX_SLIDE) {
        return Err(format!("slide {by:?} is more than a kilometre"));
    }
    let turn = p.get("turn").and_then(Value::as_f64).unwrap_or(0.0);
    if by == [0.0; 3] && turn == 0.0 {
        return Err("nothing to move: give `by` or `turn`".into());
    }
    let pivot = match cmd::vec3(p, "pivot") {
        Some(v) if v.iter().all(|x| x.is_finite() && x.abs() <= MAX_SLIDE) => [v[0], v[1]],
        Some(v) => return Err(format!("pivot {v:?} is more than a kilometre away")),
        None => {
            let root = doc.joints()?.first().copied().ok_or("the rig has no root")?;
            [root[0], root[1]]
        }
    };
    Ok(Placement {
        by,
        turn: turn.to_radians(),
        pivot,
    })
}

fn pose_move(doc: &mut Document, p: &Map<String, Value>) -> Result<Value, String> {
    let m = placement(doc, p)?;
    let mut moved = doc.move_frame(&m, super::pose::MAX_ROOT)?;
    if moved.keyed {
        // the AI frames on both sides were made for the key where it was
        moved.notes.extend(doc.resync_ai()?);
    }
    Ok(json!({ "frame": moved.frame, "keyed": moved.keyed, "notes": moved.notes }))
}

fn clip_move(doc: &mut Document, p: &Map<String, Value>) -> Result<Value, String> {
    let m = placement(doc, p)?;
    let moved = doc.move_animation(&m, super::pose::MAX_ROOT)?;
    Ok(json!({ "keys": moved.keys, "notes": moved.notes }))
}
