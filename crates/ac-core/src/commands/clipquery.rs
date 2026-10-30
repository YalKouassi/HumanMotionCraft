//! Read-only views of the clip: the editor's state, joint curves, foot contacts and samples.

use serde_json::{Map, Value, json};

use super::returns;
use crate::clip::KeySource;
use crate::command::{self as cmd, CommandSpec, Kind, ParamSpec};
use crate::document::Document;
use crate::{edits, parts};

type P = Map<String, Value>;
type Out = Result<Value, String>;

const BONE: ParamSpec = ParamSpec::new("bone", Kind::Bone, "a bone").required();

pub static SPECS: &[CommandSpec] = &[
    CommandSpec::new("editor.state", "Editor state", editor_state).outputs(returns::EDITOR_STATE),
    CommandSpec::new("clip.curves", "Joint curves", clip_curves)
        .icon("graph")
        .inputs(&[BONE])
        .outputs(returns::CLIP_CURVES),
    CommandSpec::new("clip.contacts", "Foot contacts", clip_contacts)
        .icon("foot")
        .outputs(returns::CLIP_CONTACTS),
    CommandSpec::new("clip.sample", "Sample clip", clip_sample)
        .inputs(&[
            ParamSpec::new("from", Kind::Number, "first frame").range(0.0, 1e7),
            ParamSpec::new("to", Kind::Number, "last frame").range(0.0, 1e7),
            ParamSpec::new("step", Kind::Number, "frames between samples").range(0.01, 1e4),
        ])
        .outputs(returns::CLIP_SAMPLE),
];

/// Everything a front end needs to draw the editor (cf-animate's `state` message).
fn editor_state(doc: &mut Document, _: &P) -> Out {
    let sk = doc.skeleton.clone();
    let keys: Vec<Value> = doc
        .clip
        .keys()
        .iter()
        .map(|k| {
            let mut m = json!({
                "frame": k.frame,
                "interp": k.interp.name(),
                "source": if k.source == KeySource::Ai { "ai" } else { "manual" },
                "model": k.model,
            });
            if let (Some(only), Some(obj)) = (&k.only, m.as_object_mut()) {
                obj.insert(
                    "part".into(),
                    json!(parts::label(&sk, &doc.custom_parts, Some(only))),
                );
                obj.insert("rows".into(), json!(parts::rows(&sk, Some(only))));
            }
            m
        })
        .collect();
    let layers: Vec<Value> = doc
        .clip
        .layers
        .iter()
        .map(|l| json!({ "name": l.name, "keys": l.keys.len(), "muted": l.muted, "weight": l.weight, "link": l.link }))
        .collect();
    Ok(json!({
        "frame": doc.frame,
        "length": doc.clip.length(),
        "fps": doc.clip.fps,
        "loop": doc.clip.loop_,
        "name": doc.clip.name,
        "group": doc.clip.group,
        "id": doc.clip_id,
        "project": doc.project.as_ref().map(|p| p.path.to_string_lossy().replace('\\', "/")),
        "unsaved": doc.unsaved(),
        "keys": keys,
        "layers": layers,
        "edit_layer": doc.clip.edit,
        "locked": doc.locked,
        "pins": doc.pins.keys().collect::<Vec<_>>(),
        "bands": edits::ai_bands(&doc.clip),
        "clipboard": doc.clipboard.as_ref().map(|c| parts::label(&sk, &doc.custom_parts, c.only.as_deref())),
        "part_presets": parts::PRESETS,
        "part_rows": parts::ROWS,
        "physics": doc.clip.physics,
        "markers": crate::markers::json(&doc.clip),
        "preview": doc.ai_preview.as_ref().map(|p| json!({
            "from": p.from, "to": p.to, "model": p.label, "plain": p.plain,
            "live": doc.live_preview().is_some(),
        })),
    }))
}

fn clip_curves(doc: &mut Document, p: &P) -> Out {
    let bone = doc.bone(cmd::text(p, "bone").unwrap_or_default())?;
    edits::curves(&doc.clip, bone, &doc.links)
}

fn clip_contacts(doc: &mut Document, _: &P) -> Out {
    Ok(json!(doc.contacts(0.04, 0.5)?))
}

/// Most samples one request may ask for.
const MAX_SAMPLES: f64 = 100_000.0;

fn clip_sample(doc: &mut Document, p: &P) -> Out {
    if doc.clip.base().keys.is_empty() {
        return Err(crate::error::hinted(
            "the animation has no keys",
            "pose the body and run key.set, or open one with clip.open",
        ));
    }
    let from = p.get("from").and_then(Value::as_f64).unwrap_or(0.0);
    let to = p
        .get("to")
        .and_then(Value::as_f64)
        .unwrap_or((doc.clip.length() - 1).max(0) as f64);
    let step = p.get("step").and_then(Value::as_f64).unwrap_or(1.0);
    if to < from || (to - from) / step > MAX_SAMPLES {
        return Err(format!(
            "{from}..{to} by {step} is empty or more than {MAX_SAMPLES} samples"
        ));
    }
    let n = ((to - from) / step).floor() as usize + 1;
    let frames: Vec<f64> = (0..n).map(|i| from + i as f64 * step).collect();
    let (r, o) = doc.clip.sample_with(&frames, &doc.links)?;
    Ok(json!({ "frames": frames, "rotations": r, "roots": o }))
}
