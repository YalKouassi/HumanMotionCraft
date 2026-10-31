//! The timeline's own commands: markers, retiming, moving one part's keys (dope sheet rows),
//! copying and pasting keys, and planting a foot (the contact strip).

use serde_json::{Map, Value, json};

use crate::command::{self as cmd, CommandSpec, Kind, ParamSpec};
use crate::document::Document;
use crate::{markers, retime};

const MARKER_FRAME: ParamSpec = ParamSpec::new("frame", Kind::Integer, "the marker's frame")
    .required()
    .range(0.0, 1e7);
const NAME: ParamSpec = ParamSpec::new("name", Kind::Text, "the marker's name (one line)");
const FROM: ParamSpec = ParamSpec::new("from", Kind::Integer, "first frame")
    .required()
    .range(0.0, 1e7);
const TO: ParamSpec = ParamSpec::new("to", Kind::Integer, "last frame")
    .required()
    .range(0.0, 1e7);
const MARKERS_OUT: &[ParamSpec] = &[ParamSpec::new(
    "markers",
    Kind::Text,
    "every marker after the change: [{frame, name}] by frame",
)
.json()
.required()];

pub static SPECS: &[CommandSpec] = &[
    CommandSpec::new("marker.add", "Add marker", marker_add)
        .icon("marker")
        .undoable()
        .inputs(&[MARKER_FRAME, NAME])
        .outputs(MARKERS_OUT),
    CommandSpec::new("marker.rename", "Rename marker", marker_rename)
        .icon("marker")
        .undoable()
        .inputs(&[MARKER_FRAME, NAME.required()])
        .outputs(MARKERS_OUT),
    CommandSpec::new("marker.move", "Move marker", marker_move)
        .icon("marker")
        .undoable()
        .inputs(&[
            MARKER_FRAME,
            ParamSpec::new("to", Kind::Integer, "its new frame")
                .required()
                .range(0.0, 1e7),
        ])
        .outputs(MARKERS_OUT),
    CommandSpec::new("marker.delete", "Delete marker", marker_delete)
        .icon("marker")
        .undoable()
        .inputs(&[MARKER_FRAME])
        .outputs(MARKERS_OUT),
    CommandSpec::new("keys.retime", "Retime keys", keys_retime)
        .icon("retime")
        .undoable()
        .inputs(&[
            ParamSpec::new(
                "mode",
                Kind::Choice(&["insert", "delete", "scale"]),
                "insert or delete frames at `at` (everything after slides), or scale keys about `pivot`",
            )
            .required(),
            ParamSpec::new(
                "at",
                Kind::Integer,
                "insert/delete: the first frame inserted or deleted",
            )
            .range(0.0, 1e7),
            ParamSpec::new("count", Kind::Integer, "insert/delete: how many frames").range(1.0, 1e6),
            ParamSpec::new(
                "frames",
                Kind::IntList,
                "scale: the keys to scale (default: every key of the edited layer)",
            ),
            ParamSpec::new(
                "pivot",
                Kind::Number,
                "scale: the frame that stays (default: the first key)",
            )
            .range(0.0, 1e7),
            ParamSpec::new(
                "factor",
                Kind::Number,
                "scale: 2 doubles the spacing, 0.5 halves it",
            )
            .range(0.01, 100.0),
        ])
        .outputs(&[
            ParamSpec::new("keys", Kind::IntList, "the edited layer's key frames after"),
            ParamSpec::new("moved", Kind::Integer, "keys that changed frame"),
            ParamSpec::new(
                "removed",
                Kind::TextList,
                "keys deleted with their frames (layer: frame)",
            ),
            ParamSpec::new("notes", Kind::TextList, "fills to redo, markers removed"),
        ]),
    CommandSpec::new("key.move_parts", "Move part keys", key_move_parts)
        .icon("move-key")
        .undoable()
        .inputs(&[
            ParamSpec::new("from", Kind::Integer, "the key's frame")
                .required()
                .range(0.0, 1e7),
            ParamSpec::new("to", Kind::Integer, "where those parts' keys go")
                .required()
                .range(0.0, 1e7),
            ParamSpec::new("parts", Kind::TextList, "the body parts to move").required(),
        ])
        .outputs(&[ParamSpec::new("bones", Kind::Integer, "bones moved")]),
    CommandSpec::new("keys.copy", "Copy keys", keys_copy)
        .icon("copy-key")
        .inputs(&[ParamSpec::new("frames", Kind::IntList, "the keys' frames").required()])
        .outputs(&[ParamSpec::new("copied", Kind::Integer, "keys copied")]),
    CommandSpec::new("keys.paste", "Paste keys", keys_paste)
        .icon("paste")
        .undoable()
        .inputs(&[
            ParamSpec::new(
                "at",
                Kind::Integer,
                "where the first key goes (default: the playhead)",
            )
            .range(0.0, 1e7),
            ParamSpec::new("mirror", Kind::Bool, "swap left and right"),
        ])
        .outputs(&[
            ParamSpec::new("frames", Kind::IntList, "frames pasted"),
            ParamSpec::new("replaced", Kind::IntList, "frames whose keys were replaced"),
        ]),
    CommandSpec::new("feet.plant", "Plant foot", feet_plant)
        .icon("foot")
        .undoable()
        .inputs(&[
            ParamSpec::new("foot", Kind::Choice(&["L", "R"]), "which foot (this or hand)"),
            ParamSpec::new("hand", Kind::Choice(&["L", "R"]), "which hand (this or foot)"),
            FROM,
            TO,
            ParamSpec::new(
                "on",
                Kind::Bool,
                "plant (default) or take the plant's in-between keys out",
            ),
        ])
        .outputs(&[
            ParamSpec::new("frames", Kind::Integer, "frames changed (planted or freed)").required(),
            ParamSpec::new("notes", Kind::TextList, "what stays as it was").required(),
        ]),
];

type P = Map<String, Value>;
type Out = Result<Value, String>;

fn int(p: &P, name: &str) -> Result<i64, String> {
    p.get(name)
        .and_then(Value::as_f64)
        .map(|x| x as i64)
        .ok_or_else(|| format!("{name} missing"))
}

fn frames(p: &P, name: &str) -> Vec<i64> {
    p.get(name)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
        .collect()
}

fn listed(doc: &Document) -> Out {
    Ok(json!({ "markers": markers::json(&doc.clip) }))
}

fn marker_add(doc: &mut Document, p: &P) -> Out {
    let name = match cmd::text(p, "name") {
        Some(n) => n.to_string(),
        None => markers::default_name(&doc.clip),
    };
    markers::add(&mut doc.clip, int(p, "frame")?, &name)?;
    listed(doc)
}

fn marker_rename(doc: &mut Document, p: &P) -> Out {
    markers::rename(
        &mut doc.clip,
        int(p, "frame")?,
        cmd::text(p, "name").unwrap_or_default(),
    )?;
    listed(doc)
}

fn marker_move(doc: &mut Document, p: &P) -> Out {
    markers::move_to(&mut doc.clip, int(p, "frame")?, int(p, "to")?)?;
    listed(doc)
}

fn marker_delete(doc: &mut Document, p: &P) -> Out {
    markers::delete(&mut doc.clip, int(p, "frame")?)?;
    listed(doc)
}

fn keys_retime(doc: &mut Document, p: &P) -> Out {
    let mode = cmd::text(p, "mode").unwrap_or_default();
    let needs = |names: &[&str], not: &[&str]| -> Result<(), String> {
        if let Some(n) = names.iter().find(|n| !p.contains_key(**n)) {
            return Err(format!("{mode} needs `{n}`"));
        }
        if let Some(n) = not.iter().find(|n| p.contains_key(**n)) {
            return Err(format!("`{n}` is not used by {mode}"));
        }
        Ok(())
    };
    let done = match mode {
        "insert" | "delete" => {
            needs(&["at", "count"], &["frames", "pivot", "factor"])?;
            let (at, count) = (int(p, "at")?, int(p, "count")?);
            if mode == "insert" {
                retime::insert(&mut doc.clip, at, count)?
            } else {
                retime::delete(&mut doc.clip, at, count)?
            }
        }
        "scale" => {
            needs(&["factor"], &["at", "count"])?;
            let mut chosen = frames(p, "frames");
            if !p.contains_key("frames") {
                chosen = doc.clip.keys().iter().map(|k| k.frame).collect();
            }
            let pivot = match p.get("pivot").and_then(Value::as_f64) {
                Some(x) => x,
                None => chosen.iter().min().copied().unwrap_or(0) as f64,
            };
            let factor = p.get("factor").and_then(Value::as_f64).ok_or("factor missing")?;
            retime::scale(&mut doc.clip, &chosen, pivot, factor)?
        }
        other => return Err(format!("unknown mode {other:?}")),
    };
    doc.go_to(doc.frame)?;
    let keys: Vec<i64> = doc.clip.keys().iter().map(|k| k.frame).collect();
    Ok(json!({ "keys": keys, "moved": done.moved, "removed": done.removed, "notes": done.notes }))
}

fn key_move_parts(doc: &mut Document, p: &P) -> Out {
    let parts = cmd::texts(p, "parts");
    if parts.is_empty() {
        return Err("name at least one body part (key.move moves whole keys)".into());
    }
    let bones = doc
        .part_bones(&parts)?
        .unwrap_or_else(|| (0..doc.skeleton.len()).collect());
    let moved = doc.move_parts(int(p, "from")?, int(p, "to")?, &bones)?;
    Ok(json!({ "bones": moved.len() }))
}

fn keys_copy(doc: &mut Document, p: &P) -> Out {
    Ok(json!({ "copied": doc.copy_keys(&frames(p, "frames"))? }))
}

fn keys_paste(doc: &mut Document, p: &P) -> Out {
    let at = match p.get("at") {
        Some(_) => int(p, "at")?,
        None => doc.key_frame().max(0),
    };
    let mirror = p.get("mirror").and_then(Value::as_bool).unwrap_or(false);
    let (frames, replaced) = doc.paste_keys(at, mirror)?;
    Ok(json!({ "frames": frames, "replaced": replaced }))
}

fn feet_plant(doc: &mut Document, p: &P) -> Out {
    let (a, b) = (int(p, "from")?, int(p, "to")?);
    let (hand, side) = match (cmd::text(p, "foot"), cmd::text(p, "hand")) {
        (Some(side), None) => (false, side.to_string()),
        (None, Some(side)) => (true, side.to_string()),
        _ => return Err("give foot or hand, one of them".into()),
    };
    if p.get("on").and_then(Value::as_bool).unwrap_or(true) {
        let n = if hand {
            doc.plant_hand(&side, a, b)?
        } else {
            doc.plant_foot(&side, a, b)?
        };
        Ok(json!({ "frames": n, "notes": [] }))
    } else {
        let n = doc.unplant_limb(if hand { "wrist" } else { "foot" }, &side, a, b)?;
        Ok(json!({
            "frames": n,
            "notes": [format!(
                "removed the planted in-between keys on {a}–{b} (both feet's); keys of yours there keep their planted legs"
            )],
        }))
    }
}
