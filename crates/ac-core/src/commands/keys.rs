//! Frames, keys and posing: the timeline and the viewport's tools.

use serde_json::{Map, Value, json};

use super::returns;
use crate::clip::Interp;
use crate::command::{self as cmd, CommandSpec, Kind, ParamSpec};
use crate::document::{Clipboard, Document};
use crate::edits;
use crate::parts;

const PARTS: ParamSpec = ParamSpec::new("parts", Kind::TextList, "body parts (empty: the whole body)");
const FRAME: ParamSpec =
    ParamSpec::new("frame", Kind::Number, "a frame (fractional while scrubbing)").range(0.0, 1e7);
const KEY_FRAME: ParamSpec = ParamSpec::new("frame", Kind::Integer, "a key's frame").range(0.0, 1e7);
const BONE: ParamSpec = ParamSpec::new("bone", Kind::Bone, "a bone").required();
const INTERP: ParamSpec = ParamSpec::new(
    "interp",
    Kind::Choice(Interp::NAMES),
    "interpolation into the next key",
);

pub static SPECS: &[CommandSpec] = &[
    CommandSpec::new("frame.set", "Go to frame", frame_set)
        .icon("playhead")
        .inputs(&[FRAME.required()])
        .outputs(returns::FRAME_SET),
    CommandSpec::new("key.set", "Set key", key_set)
        .icon("key")
        .shortcut("K")
        .undoable()
        .inputs(&[PARTS, INTERP])
        .outputs(returns::KEY_SET),
    CommandSpec::new("key.delete", "Delete key", key_delete)
        .icon("unkey")
        .shortcut("Shift+K")
        .undoable()
        .inputs(&[PARTS]),
    CommandSpec::new("keys.delete", "Delete keys", keys_delete)
        .icon("unkey")
        .undoable()
        .inputs(&[ParamSpec::new("frames", Kind::IntList, "the keys' frames").required()]),
    CommandSpec::new("key.interp", "Set interpolation", key_interp)
        .icon("curve")
        .undoable()
        .inputs(&[INTERP.required()]),
    CommandSpec::new("key.move", "Move key", key_move)
        .icon("move-key")
        .undoable()
        .undo_label(move_label)
        .inputs(&[
            ParamSpec::new("from", Kind::Integer, "the key's frame")
                .required()
                .range(0.0, 1e7),
            ParamSpec::new("to", Kind::Integer, "its new frame")
                .required()
                .range(0.0, 1e7),
        ])
        .outputs(returns::KEY_MOVE),
    CommandSpec::new("key.copy", "Copy pose to frame", key_copy)
        .icon("copy-key")
        .undoable()
        .inputs(&[
            ParamSpec::new("from", Kind::Integer, "the frame to copy")
                .required()
                .range(0.0, 1e7),
            ParamSpec::new("to", Kind::Integer, "the frame to key")
                .required()
                .range(0.0, 1e7),
        ])
        .outputs(returns::KEY_COPY),
    CommandSpec::new("key.remove_parts", "Remove parts from key", key_remove_parts)
        .icon("unkey")
        .undoable()
        .inputs(&[KEY_FRAME.required(), PARTS]),
    CommandSpec::new("key.hold", "Hold parts", key_hold)
        .icon("hold")
        .undoable()
        .inputs(&[
            ParamSpec::new("from", Kind::Integer, "first frame")
                .required()
                .range(0.0, 1e7),
            ParamSpec::new("to", Kind::Integer, "last frame")
                .required()
                .range(0.0, 1e7),
            PARTS,
        ]),
    CommandSpec::new("key.channel", "Set key channel", key_channel)
        .icon("curve")
        .undoable()
        .inputs(&[
            BONE,
            KEY_FRAME.required(),
            ParamSpec::new("to", Kind::Integer, "move the joint's key to this frame").range(0.0, 1e7),
            ParamSpec::new(
                "axis",
                Kind::Integer,
                "0-2: angle about X/Y/Z (degrees); 3-5: hips position",
            )
            .required()
            .range(0.0, 5.0),
            ParamSpec::new("value", Kind::Number, "the new value")
                .required()
                .range(-1e6, 1e6),
        ]),
    CommandSpec::new("pose.drag", "Drag joint", pose_drag)
        .icon("drag")
        .tool()
        .undoable()
        .inputs(&[
            BONE,
            ParamSpec::new("to", Kind::Vec3, "where the bone's head goes (metres)"),
            ParamSpec::new("by", Kind::Vec3, "or: an offset from where it is now (metres)"),
        ])
        .outputs(returns::POSE_DRAG),
    CommandSpec::new("pose.reach", "Reach (full body)", pose_reach)
        .icon("reach")
        .tool()
        .undoable()
        .inputs(&[
            BONE,
            ParamSpec::new("to", Kind::Vec3, "where the bone's head goes (metres)"),
            ParamSpec::new("by", Kind::Vec3, "or: an offset from where it is now (metres)"),
        ])
        .outputs(returns::POSE_REACH),
    CommandSpec::new("pose.rotate", "Rotate joint", pose_rotate)
        .icon("rotate")
        .tool()
        .undoable()
        .inputs(&[
            BONE,
            ParamSpec::new("axis", Kind::Vec3, "world axis").required(),
            ParamSpec::new("angle", Kind::Number, "radians")
                .required()
                .range(-100.0, 100.0),
        ]),
    CommandSpec::new("pose.spread_bend", "Spread bend into the back", pose_spread_bend)
        .icon("spine")
        .undoable()
        .outputs(returns::POSE_SPREAD_BEND),
    CommandSpec::new("pose.mirror", "Mirror pose", pose_mirror)
        .icon("mirror")
        .undoable()
        .inputs(&[ParamSpec::new(
            "mode",
            Kind::Choice(&["flip", "l2r", "r2l"]),
            "flip, or copy one side onto the other",
        )]),
    CommandSpec::new("pose.copy", "Copy pose", pose_copy)
        .icon("copy")
        .shortcut("Ctrl+C")
        .inputs(&[PARTS])
        .outputs(returns::POSE_COPY),
    CommandSpec::new("pose.paste", "Paste pose", pose_paste)
        .icon("paste")
        .shortcut("Ctrl+V")
        .undoable()
        .inputs(&[PARTS, ParamSpec::new("mirror", Kind::Bool, "paste mirrored")]),
    CommandSpec::new("pin.set", "Pin joint", pin_set)
        .icon("pin")
        .undoable()
        .inputs(&[BONE, ParamSpec::new("on", Kind::Bool, "pin (true) or unpin")])
        .outputs(returns::PIN_SET),
    CommandSpec::new("pins.clear", "Clear pins", pins_clear)
        .icon("unpin")
        .undoable(),
    CommandSpec::new("parts.lock", "Lock parts", parts_lock)
        .icon("lock")
        .undoable()
        .inputs(&[PARTS])
        .outputs(returns::PARTS_LOCK),
    CommandSpec::new("clip.loop", "Loop", clip_loop)
        .icon("loop")
        .undoable()
        .inputs(&[ParamSpec::new("on", Kind::Bool, "loop the animation").required()]),
    CommandSpec::new("feet.lock", "Lock feet", feet_lock)
        .icon("foot")
        .undoable()
        .outputs(returns::FEET_LOCK),
    CommandSpec::new("feet.roll", "Roll feet", feet_roll)
        .icon("foot")
        .undoable()
        .outputs(returns::FEET_ROLL),
    CommandSpec::new("hips.transfer", "Shift weight over the feet", hips_transfer)
        .icon("foot")
        .undoable()
        .outputs(returns::HIPS_TRANSFER),
    CommandSpec::new("ai.keep", "Keep AI keys", ai_keep)
        .icon("keep")
        .undoable()
        .inputs(&[
            ParamSpec::new("from", Kind::Integer, "first frame")
                .required()
                .range(0.0, 1e7),
            ParamSpec::new("to", Kind::Integer, "last frame")
                .required()
                .range(0.0, 1e7),
        ])
        .outputs(returns::AI_KEEP),
    CommandSpec::new("ai.clear", "Clear AI keys", ai_clear)
        .icon("clear")
        .undoable()
        .inputs(&[
            ParamSpec::new("from", Kind::Integer, "key before the span")
                .required()
                .range(0.0, 1e7),
            ParamSpec::new("to", Kind::Integer, "key after the span")
                .required()
                .range(0.0, 1e7),
        ])
        .outputs(returns::AI_CLEAR),
    CommandSpec::new("ai.fill", "AI fill", ai_fill)
        .icon("ai")
        .undoable()
        .undo_label(fill_label)
        .inputs(&[
            ParamSpec::new(
                "from",
                Kind::Integer,
                "your key before the gap (default: around the playhead)",
            )
            .range(0.0, 1e7),
            ParamSpec::new("to", Kind::Integer, "your key after the gap").range(0.0, 1e7),
            ParamSpec::new(
                "anchors",
                Kind::Choice(&["auto", "none"]),
                "a gap over the model's limit: \"auto\" (default) places anchor poses from the library inside it and fills each span between them; \"none\" refuses it",
            ),
        ])
        .outputs(returns::AI_FILL),
    CommandSpec::new("ai.settings", "AI fill settings", ai_settings)
        .inputs(&[
            ParamSpec::new(
                "preset",
                Kind::Text,
                "a named preset: \"preserve\" | \"natural\" | \"follow_style\"",
            ),
            ParamSpec::new("style", Kind::Text, "a style the model knows (\"any\": none)"),
            ParamSpec::new(
                "style_force",
                Kind::Number,
                "style guidance: 0 none, 1 the style, more is stronger",
            )
            .range(0.0, 3.0),
            ParamSpec::new(
                "ai_force",
                Kind::Number,
                "how much of the network's change to keep (0: plain interpolation)",
            )
            .range(0.0, 1.5),
            ParamSpec::new(
                "trajectory",
                Kind::Number,
                "pull the root onto the keys' own path",
            )
            .range(0.0, 1.0),
            ParamSpec::new("head", Kind::Number, "keep the neck and head on the keys' spline")
                .range(0.0, 1.0),
        ])
        .outputs(returns::AI_SETTINGS),
    CommandSpec::new("ai.info", "AI model", ai_info).outputs(returns::AI_INFO),
    CommandSpec::new("ai.preview", "Preview AI fill", ai_preview)
        .icon("ai")
        .inputs(&[
            ParamSpec::new(
                "from",
                Kind::Integer,
                "your key before the gap (default: around the playhead)",
            )
            .range(0.0, 1e7),
            ParamSpec::new("to", Kind::Integer, "your key after the gap").range(0.0, 1e7),
            ParamSpec::new(
                "mode",
                Kind::Text,
                "\"ai\" (the model, default) or \"plain\" (plain interpolation)",
            ),
        ])
        .outputs(returns::AI_PREVIEW),
    CommandSpec::new("ai.accept", "Keep AI fill", ai_accept)
        .icon("ai")
        .undoable()
        .undo_label(fill_label)
        .outputs(returns::AI_PREVIEW),
    CommandSpec::new("ai.discard", "Discard AI preview", ai_discard)
        .icon("ai")
        .outputs(returns::AI_DISCARD),
];

type P = Map<String, Value>;
type Out = Result<Value, String>;

/// A fill's undo step names the frames filled: those between the keys it reports ("AI fill 1–29").
fn fill_label(label: &'static str, _: &P, r: &Value) -> String {
    match (r["from"].as_i64(), r["to"].as_i64()) {
        (Some(a), Some(b)) => format!("{label} {}–{}", a + 1, b - 1),
        _ => label.to_string(),
    }
}

/// "Move key 20→30".
fn move_label(label: &'static str, p: &P, _: &Value) -> String {
    match (
        p.get("from").and_then(Value::as_i64),
        p.get("to").and_then(Value::as_i64),
    ) {
        (Some(a), Some(b)) => format!("{label} {a}→{b}"),
        _ => label.to_string(),
    }
}

fn int(p: &P, name: &str) -> Result<i64, String> {
    p.get(name)
        .and_then(Value::as_f64)
        .map(|x| x as i64)
        .ok_or_else(|| format!("{name} missing"))
}

fn num(p: &P, name: &str) -> Result<f64, String> {
    p.get(name)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("{name} missing"))
}

fn parts_of(doc: &Document, p: &P) -> Result<Option<Vec<usize>>, String> {
    doc.part_bones(&cmd::texts(p, "parts"))
}

fn frame_set(doc: &mut Document, p: &P) -> Out {
    doc.go_to(num(p, "frame")?)?;
    Ok(json!({ "frame": doc.frame }))
}

fn key_set(doc: &mut Document, p: &P) -> Out {
    let interp = cmd::text(p, "interp").and_then(Interp::parse);
    let only = parts_of(doc, p)?;
    let f = doc.key_frame();
    let was = doc.clip.keys().iter().find(|k| k.frame == f).cloned();
    doc.set_key(interp, only)?;
    let same = doc.clip.keys().iter().find(|k| k.frame == f) == was.as_ref();
    // the AI frames on both sides were made for the old key
    let notes = doc.resync_ai()?;
    let mut out = json!({ "frame": f });
    // a key set again as it was leaves nothing to make again
    if !notes.is_empty() && !same {
        out["notes"] = json!(notes);
    }
    Ok(out)
}

fn key_delete(doc: &mut Document, p: &P) -> Out {
    let only = parts_of(doc, p)?;
    doc.delete_key(only)?;
    Ok(json!({}))
}

fn keys_delete(doc: &mut Document, p: &P) -> Out {
    let frames: Vec<i64> = p
        .get("frames")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
        .collect();
    doc.delete_keys(&frames)?;
    Ok(json!({}))
}

fn key_interp(doc: &mut Document, p: &P) -> Out {
    let interp = cmd::text(p, "interp")
        .and_then(Interp::parse)
        .ok_or("interp missing")?;
    let f = doc.key_frame();
    let e = doc.clip.edit;
    let k = doc
        .clip
        .layers
        .get_mut(e)
        .and_then(|l| l.keys.iter_mut().find(|k| k.frame == f))
        .ok_or_else(|| {
            crate::error::hinted(
                format!("no key on frame {f}"),
                "frame.set to a keyed frame (editor.state lists the keys), or key.set first",
            )
        })?;
    k.interp = interp;
    Ok(json!({}))
}

fn key_move(doc: &mut Document, p: &P) -> Out {
    let filler = doc.ai_filler();
    let notes = doc.move_key(
        int(p, "from")?,
        int(p, "to")?,
        filler.as_ref().map(|f| f as &dyn edits::Filler),
    )?;
    Ok(json!({ "notes": notes }))
}

fn key_copy(doc: &mut Document, p: &P) -> Out {
    let filler = doc.ai_filler();
    let notes = doc.copy_key(
        int(p, "from")?,
        int(p, "to")?,
        filler.as_ref().map(|f| f as &dyn edits::Filler),
    )?;
    Ok(json!({ "notes": notes }))
}

fn key_remove_parts(doc: &mut Document, p: &P) -> Out {
    let only = parts_of(doc, p)?;
    doc.remove_parts(int(p, "frame")?, only)?;
    doc.go_to(doc.frame)?;
    Ok(json!({}))
}

fn key_hold(doc: &mut Document, p: &P) -> Out {
    let only = parts_of(doc, p)?;
    doc.hold_part(int(p, "from")?, int(p, "to")?, only)?;
    Ok(json!({}))
}

fn key_channel(doc: &mut Document, p: &P) -> Out {
    let bone = doc.bone(cmd::text(p, "bone").unwrap_or_default())?;
    let frame = int(p, "frame")?;
    let to = p.get("to").and_then(Value::as_i64).unwrap_or(frame);
    doc.set_channel(bone, frame, to, int(p, "axis")? as usize, num(p, "value")?)?;
    Ok(json!({}))
}

fn pose_drag(doc: &mut Document, p: &P) -> Out {
    let bone = cmd::text(p, "bone").unwrap_or_default().to_string();
    let i = doc.bone(&bone)?;
    let target = match (cmd::vec3(p, "to"), cmd::vec3(p, "by")) {
        (Some(t), None) => t,
        (None, Some(d)) => crate::quat::add3(*doc.joints()?.get(i).ok_or("joint")?, d),
        _ => return Err("give exactly one of `to` and `by`".into()),
    };
    if target.iter().any(|x| x.abs() > 1e3) {
        return Err(format!("target {target:?} is more than a kilometre away"));
    }
    doc.drag(&bone, target)?;
    let at = doc.joints()?.get(i).copied().ok_or("joint")?;
    Ok(json!({ "position": at }))
}

/// The whole body moves the bone toward its target; pinned joints stay, limits hold.
fn pose_reach(doc: &mut Document, p: &P) -> Out {
    let bone = cmd::text(p, "bone").unwrap_or_default().to_string();
    let i = doc.bone(&bone)?;
    let target = match (cmd::vec3(p, "to"), cmd::vec3(p, "by")) {
        (Some(t), None) => t,
        (None, Some(d)) => crate::quat::add3(*doc.joints()?.get(i).ok_or("joint")?, d),
        _ => return Err("give exactly one of `to` and `by`".into()),
    };
    if target.iter().any(|x| x.abs() > 1e3) {
        return Err(format!("target {target:?} is more than a kilometre away"));
    }
    let before = doc.pose.clone();
    let left = doc.reach(&bone, target)?;
    doc.keep_locked_pub(&before)?;
    let at = doc.joints()?.get(i).copied().ok_or("joint")?;
    Ok(json!({ "position": at, "error": left, "reached": left < 1e-4 }))
}

fn pose_rotate(doc: &mut Document, p: &P) -> Out {
    let axis = cmd::vec3(p, "axis").ok_or("axis missing")?;
    if crate::quat::len3(axis) < 1e-12 {
        return Err("the axis must not be zero".into());
    }
    let base = doc.pose.rotations.clone();
    doc.rotate(
        &base,
        cmd::text(p, "bone").unwrap_or_default(),
        axis,
        num(p, "angle")?,
    )?;
    Ok(json!({}))
}

fn pose_spread_bend(doc: &mut Document, _: &P) -> Out {
    let deg = doc.spread_bend(edits::SPINE_SHARE)?;
    Ok(json!({ "degrees": deg }))
}

fn pose_mirror(doc: &mut Document, p: &P) -> Out {
    doc.mirror(cmd::text(p, "mode").unwrap_or("flip"))?;
    Ok(json!({}))
}

fn pose_copy(doc: &mut Document, p: &P) -> Out {
    let only = parts_of(doc, p)?;
    let label = parts::label(&doc.skeleton, &doc.custom_parts, only.as_deref());
    doc.clipboard = Some(Clipboard {
        pose: doc.pose.clone(),
        only,
    });
    Ok(json!({ "copied": label }))
}

fn pose_paste(doc: &mut Document, p: &P) -> Out {
    let cb = doc
        .clipboard
        .clone()
        .ok_or_else(|| crate::error::hinted("nothing copied yet", "run pose.copy first"))?;
    let (mut pose, mut only) = (cb.pose, cb.only);
    if let Some(ask) = parts_of(doc, p)? {
        only = Some(match only {
            None => ask,
            Some(o) => o.into_iter().filter(|b| ask.contains(b)).collect(),
        });
        if only.as_ref().is_some_and(Vec::is_empty) {
            return Err("the copied pose has none of those parts".into());
        }
    }
    if p.get("mirror").and_then(Value::as_bool).unwrap_or(false) {
        pose = edits::mirrored(&doc.skeleton, &pose)?;
        only = edits::mirror_bones(&doc.skeleton, only.as_deref());
    }
    doc.paste_part(&pose, only.as_deref());
    Ok(json!({}))
}

fn pin_set(doc: &mut Document, p: &P) -> Out {
    let on = p.get("on").and_then(Value::as_bool).unwrap_or(true);
    doc.pin(cmd::text(p, "bone").unwrap_or_default(), on)?;
    Ok(json!({ "pins": doc.pins.keys().collect::<Vec<_>>() }))
}

fn pins_clear(doc: &mut Document, _: &P) -> Out {
    doc.pins.clear();
    Ok(json!({}))
}

fn parts_lock(doc: &mut Document, p: &P) -> Out {
    let parts = cmd::texts(p, "parts");
    doc.part_bones(&parts)?; // refuse unknown names
    doc.locked = parts;
    Ok(json!({ "locked": doc.locked }))
}

fn clip_loop(doc: &mut Document, p: &P) -> Out {
    doc.clip.loop_ = p.get("on").and_then(Value::as_bool).unwrap_or(false);
    doc.go_to(doc.frame)?;
    Ok(json!({}))
}

fn feet_lock(doc: &mut Document, _: &P) -> Out {
    Ok(json!({ "frames": doc.lock_feet()? }))
}

fn feet_roll(doc: &mut Document, _: &P) -> Out {
    let (frames, short) = doc.roll_feet()?;
    Ok(json!({ "frames": frames, "short": short }))
}

fn hips_transfer(doc: &mut Document, _: &P) -> Out {
    let (frames, left) = doc.transfer_weight()?;
    Ok(json!({ "frames": frames, "left_cm": left }))
}

/// A frame range from `from` and `to`; refuses one that runs backwards.
fn range(p: &P) -> Result<(i64, i64), String> {
    let (a, b) = (int(p, "from")?, int(p, "to")?);
    if b < a {
        return Err(format!("the range {a}..{b} runs backwards"));
    }
    Ok((a, b))
}

fn ai_keep(doc: &mut Document, p: &P) -> Out {
    let (a, b) = range(p)?;
    Ok(json!({ "kept": edits::keep_ai(&mut doc.clip, a, b)? }))
}

fn ai_clear(doc: &mut Document, p: &P) -> Out {
    let (a, b) = range(p)?;
    let n = edits::clear_ai(&mut doc.clip, a, b)?;
    doc.go_to(doc.frame)?;
    Ok(json!({ "cleared": n }))
}

/// Fill between two of your keys with the learned in-betweener.
fn ai_fill(doc: &mut Document, p: &P) -> Out {
    let span = match (p.get("from"), p.get("to")) {
        (None, None) => None,
        (Some(_), Some(_)) => Some(range(p)?),
        _ => return Err("give both `from` and `to`, or neither".into()),
    };
    let anchors = match cmd::text(p, "anchors") {
        None | Some("auto") => true,
        Some("none") => false,
        Some(x) => return Err(format!("anchors {x:?}: \"auto\" or \"none\"")),
    };
    let f = doc.ai_fill_with(span, anchors)?;
    let d = crate::filldiag::diagnose(doc, &f).json();
    Ok(json!({
        "from": f.from, "to": f.to, "frames": f.to - f.from - 1, "model": f.model, "notes": f.notes,
        "anchors": f.anchors, "anchor_segment": f.segment, "diagnostics": d,
    }))
}

/// A previewed fill: its span, label and the root head on every frame of it (the ghost path).
fn preview_json(doc: &Document, p: &crate::ai::Preview) -> Out {
    let frames: Vec<f64> = (p.from..=p.to).map(|f| f as f64).collect();
    let (rots, roots) = p.clip.sample_with(&frames, &p.links)?;
    let heads = rots
        .iter()
        .zip(&roots)
        .map(|(r, o)| Ok(doc.skeleton.fk(r, *o).map_err(|e| e.to_string())?.1[0]))
        .collect::<Result<Vec<_>, String>>()?;
    Ok(json!({
        "from": p.from, "to": p.to, "frames": p.to - p.from - 1, "model": p.label,
        "mode": if p.plain { "plain" } else { "ai" }, "root_heads": heads, "notes": p.notes,
    }))
}

/// Work out a fill and show it as a ghost without changing the animation.
fn ai_preview(doc: &mut Document, p: &P) -> Out {
    let span = match (p.get("from"), p.get("to")) {
        (None, None) => None,
        (Some(_), Some(_)) => Some(range(p)?),
        _ => return Err("give both `from` and `to`, or neither".into()),
    };
    let plain = match cmd::text(p, "mode") {
        None | Some("ai") => false,
        Some("plain") => true,
        Some(m) => return Err(format!("mode {m:?}: \"ai\" or \"plain\"")),
    };
    let pv = doc.ai_preview(span, plain)?;
    preview_json(doc, &pv)
}

/// Keep the previewed fill (one undo step).
fn ai_accept(doc: &mut Document, _: &P) -> Out {
    let pv = doc.ai_accept()?;
    preview_json(doc, &pv)
}

/// Drop the preview; says whether there was one.
fn ai_discard(doc: &mut Document, _: &P) -> Out {
    Ok(json!({ "discarded": doc.ai_preview.take().is_some() }))
}

/// Set any of the fill settings; returns them all.
fn ai_settings(doc: &mut Document, p: &P) -> Out {
    let mut s = doc.ai_settings.clone();
    if let Some(preset) = cmd::text(p, "preset") {
        match preset.to_ascii_lowercase().replace([' ', '_', '-'], "").as_str() {
            "preserve" | "preservemotion" | "preservemymotion" => {
                s.ai_force = 0.5;
                s.style_force = 0.0;
                s.trajectory = 0.8;
                s.head = 0.8;
            }
            "natural" | "naturalmotion" | "naturalmovement" => {
                s.ai_force = 1.0;
                s.style_force = 1.0;
                s.trajectory = 0.0;
                s.head = 0.0;
            }
            "followstyle" | "style" => {
                s.ai_force = 1.0;
                s.style_force = 1.5;
                s.trajectory = 0.2;
                s.head = 0.0;
            }
            _ => {
                return Err(format!(
                    "unknown preset {preset:?}; one of \"preserve\", \"natural\", \"follow_style\""
                ));
            }
        }
    }
    if let Some(st) = cmd::text(p, "style") {
        let known = doc
            .ai
            .as_ref()
            .map(|m| m.info().styles.clone())
            .unwrap_or_default();
        if st != "any" && !known.iter().any(|k| k == st) {
            return Err(format!("style {st:?}: the model knows {known:?}"));
        }
        s.style = st.to_string();
    }
    for (k, v) in [
        ("style_force", &mut s.style_force),
        ("ai_force", &mut s.ai_force),
        ("trajectory", &mut s.trajectory),
        ("head", &mut s.head),
    ] {
        if let Some(x) = p.get(k).and_then(Value::as_f64) {
            *v = x;
        }
    }
    doc.ai_settings = s;
    Ok(json!(doc.ai_settings))
}

/// The loaded model's card (`model: null` when none is loaded) and the settings.
fn ai_info(doc: &mut Document, _: &P) -> Out {
    Ok(json!({ "model": doc.ai.as_ref().map(|m| m.info().clone()), "settings": doc.ai_settings }))
}
