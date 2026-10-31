//! Secondary motion as a layer (`layer.secondary`, roadmap 119800): a separate command, never part
//! of AI Fill. The layer is additive and has a key on each frame of the clip, so muting it gives
//! the clip back and no key of the person's is touched.

use serde_json::{Map, Value, json};

use crate::clip::{Interp, Key, Layer, LayerMode, LayerSource};
use crate::command::{self as cmd, CommandSpec, Kind, ParamSpec};
use crate::document::Document;
use crate::secondary::{self, Below, Options};

/// The layer's name; running the command again replaces the layer with this name.
const LAYER_NAME: &str = "Secondary motion";
/// Most frames it works on: every frame gets a key.
const MAX_FRAMES: i64 = 6000;
/// An ankle further than this (m) from its place is said in the notes.
const DRIFT_NOTE: f64 = 0.01;
/// A largest turn under this (radians) means the clip gave it nothing to work from.
const STILL: f64 = 1e-3;

pub static SPECS: &[CommandSpec] = &[CommandSpec::new("layer.secondary", "Secondary motion", secondary_motion)
    .icon("layer-add")
    .undoable()
    .inputs(&[
        ParamSpec::new("amount", Kind::Number, "how strong, 1 is an ordinary walk (default 1)").range(0.05, 2.0),
        ParamSpec::new(
            "parts",
            Kind::TextList,
            "which of arms, shoulders, pelvis, spine, head, hands, follow (default: all)",
        ),
    ])
    .outputs(&[
        ParamSpec::new("i", Kind::Integer, "the layer").required(),
        ParamSpec::new("name", Kind::Text, "its name").required(),
        ParamSpec::new("keys", Kind::Integer, "keys on it, one a frame").required(),
        ParamSpec::new("bones", Kind::Integer, "bones it turns").required(),
        ParamSpec::new("largest", Kind::Number, "the largest turn of any bone (radians)").required(),
        ParamSpec::new("foot_drift", Kind::Number, "how far an ankle ends up from its place (m)").required(),
        ParamSpec::new("replaced", Kind::Bool, "it replaced the layer from an earlier run").required(),
        ParamSpec::new("notes", Kind::TextList, "what the user should know").required(),
    ])];

fn secondary_motion(doc: &mut Document, p: &Map<String, Value>) -> Result<Value, String> {
    let amount = p.get("amount").and_then(Value::as_f64).unwrap_or(1.0);
    if !amount.is_finite() || amount <= 0.0 {
        return Err(format!("amount must be above 0, got {amount}"));
    }
    let mut parts = cmd::texts(p, "parts");
    if let Some(bad) = parts.iter().find(|x| !secondary::PARTS.contains(&x.as_str())) {
        return Err(format!("no part {bad:?}: the parts are {:?}", secondary::PARTS));
    }
    if parts.is_empty() {
        parts = secondary::PARTS.iter().map(|s| s.to_string()).collect();
    }

    let own = doc.clip.layers.iter().enumerate().skip(1).find_map(|(i, l)| {
        (l.name == LAYER_NAME && l.source == LayerSource::Keys && l.mode == LayerMode::Additive).then_some(i)
    });
    // the clip's length without the layer being replaced, which must not hold the length up
    let mut without = doc.clip.clone();
    if let Some(l) = own.and_then(|i| without.layers.get_mut(i)) {
        l.keys.clear();
    }
    let length = without.length_with(&doc.links);
    let Some(first) = doc.clip.base().keys.first().map(|k| k.frame) else {
        return Err("the animation has no keys: key a walk first".into());
    };
    let frames = length - first;
    if frames < 3 {
        return Err("secondary motion needs an animation of at least 3 frames".into());
    }
    if frames > MAX_FRAMES {
        return Err(format!(
            "the animation has {frames} frames: secondary motion works on at most {MAX_FRAMES}"
        ));
    }

    let top = own.unwrap_or(doc.clip.layers.len());
    let at: Vec<f64> = (first..length).map(|f| f as f64).collect();
    let (rotations, roots) = doc.clip.sample_below(&at, top, &doc.links)?;
    let motion = secondary::motion(
        &doc.skeleton,
        &Below {
            rotations: &rotations,
            roots: &roots,
            fps: doc.clip.fps,
            looping: doc.clip.loop_,
        },
        &Options { amount, parts },
    )?;

    let keys: Vec<Key> = motion
        .turns
        .into_iter()
        .zip(first..)
        .map(|(turns, f)| Key::new(f, turns, [0.0; 3], Interp::Linear))
        .collect();
    let count = keys.len();
    let bones = motion.bones.len();
    let i = match own {
        Some(i) => i,
        None => {
            let mut lay = Layer::keys(LAYER_NAME, Vec::new());
            lay.mode = LayerMode::Additive;
            doc.clip.layers.push(lay);
            doc.clip.layers.len() - 1
        }
    };
    let lay = doc.clip.layers.get_mut(i).ok_or("the layer is missing")?;
    lay.keys = keys;
    lay.only = Some(motion.bones);
    lay.muted = false;

    let mut notes =
        vec!["this layer doesn't follow later edits: run it again after changing your keys".to_string()];
    if motion.largest < STILL {
        notes.push("the body hardly moves here, so there is little to add: it needs travel or steps".into());
    }
    if motion.foot_drift > DRIFT_NOTE {
        notes.push(format!(
            "a straight leg can't reach everywhere: an ankle moves up to {:.1} cm; leave pelvis out of the parts to keep it",
            motion.foot_drift * 100.0
        ));
    }
    doc.go_to(doc.frame)?;
    Ok(json!({
        "i": i,
        "name": LAYER_NAME,
        "keys": count,
        "bones": bones,
        "largest": motion.largest,
        "foot_drift": motion.foot_drift,
        "replaced": own.is_some(),
        "notes": notes,
    }))
}
