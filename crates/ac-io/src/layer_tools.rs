//! Layer tools for the layers panel: solo one layer, and see where a travel would walk before
//! adding it.

use ac_core::Document;
use ac_core::clip::LayerSource;
use ac_core::command::{CommandSpec, Kind as K, ParamSpec};
use serde_json::{Map, Value, json};

use crate::layers::{layer_at, layer_travel, refresh};

type P = Map<String, Value>;
type Out = Result<Value, String>;

pub static SPECS: &[CommandSpec] = &[
    CommandSpec::new("layer.solo", "Solo layer", layer_solo)
        .icon("layer")
        .undoable()
        .inputs(&[
            ParamSpec::new("i", K::Integer, "the layer (0: the base alone)").required(),
            ParamSpec::new(
                "on",
                K::Bool,
                "true (default): mute every other layer above the base; false: unmute them all",
            ),
        ])
        .outputs(crate::returns::LAYERS_LIST),
    CommandSpec::new("layer.travel_preview", "Preview travel", travel_preview)
        .icon("travel")
        .inputs(&[
            ParamSpec::new(
                "from",
                K::Integer,
                "first frame (with `to`; default: the keys around the playhead)",
            )
            .range(0.0, 1e6),
            ParamSpec::new("to", K::Integer, "last frame").range(0.0, 1e6),
            ParamSpec::new("parts", K::TextList, "body parts (empty: the whole body)"),
            ParamSpec::new("fade", K::Integer, "frames the layer fades in and out over").range(0.0, 600.0),
            ParamSpec::new(
                "gait",
                K::Choice(&["auto", "walk", "run", "crouch"]),
                "which Locomotion clips to walk with",
            ),
        ])
        .outputs(crate::returns::LAYER_TRAVEL_PREVIEW),
];

/// Hear one layer: every other layer above the base is muted (the base always plays). `on: false`
/// unmutes every layer.
fn layer_solo(doc: &mut Document, p: &P) -> Out {
    let i = layer_at(doc, p)?;
    let on = p.get("on").and_then(Value::as_bool).unwrap_or(true);
    for (j, lay) in doc.clip.layers.iter_mut().enumerate().skip(1) {
        lay.muted = on && j != i;
    }
    refresh(doc)
}

/// Work out the travel `layer.travel` would add, on a copy: where the hips go on every frame of
/// it (`path`, metres) and its footsteps. The animation is not changed.
fn travel_preview(doc: &mut Document, p: &P) -> Out {
    let mut trial = doc.clone();
    // the path is the keyed walk: the physics pass would only slow the preview down
    trial.clip.physics = false;
    let made = layer_travel(&mut trial, p)?;
    let i = made["i"]
        .as_u64()
        .and_then(|i| usize::try_from(i).ok())
        .ok_or("the travel layer was not made")?;
    let lay = trial.clip.layers.get(i).ok_or("the travel layer was not made")?;
    if lay.source != LayerSource::Travel {
        return Err("the travel layer was not made".into());
    }
    let (a, b) = (
        lay.start.unwrap_or(0.0).round() as i64,
        lay.end.unwrap_or(0.0).round() as i64,
    );
    let steps = lay.steps.clone();
    let gait = lay.gait.clone();
    let frames: Vec<f64> = (a..=b).map(|f| f as f64).collect();
    let (rots, roots) = trial.clip.sample_with(&frames, &trial.links)?;
    let path = rots
        .iter()
        .zip(&roots)
        .map(|(r, o)| {
            let (_, pos) = trial.skeleton.fk(r, *o).map_err(|e| e.to_string())?;
            pos.first()
                .copied()
                .ok_or_else(|| "the rig has no root".to_string())
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(json!({ "from": a, "to": b, "gait": gait, "path": path, "steps": steps }))
}
