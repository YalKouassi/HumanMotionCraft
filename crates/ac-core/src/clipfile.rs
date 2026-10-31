//! Clip and pose files: `<id>.clip` / `<id>.pose` (version 1, `<id>` is `product::ID`), and cf-animate's
//! `cf-animate.clip` / `cf-animate.pose` read as the same schema (they are imported, never written).
//!
//! Text in, text out: the callers own the file system, so this stays wasm-safe and testable.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::clip::{Clip, Interp, Key, KeySource, Layer, LayerMode, LayerSource, Surface};
use crate::product;
use crate::quat::{Quat, Vec3};

pub const CLIP_FORMAT: &str = concat!(crate::product_id!(), ".clip");
pub const POSE_FORMAT: &str = concat!(crate::product_id!(), ".pose");
const LEGACY_CLIP: &str = "cf-animate.clip";
const LEGACY_POSE: &str = "cf-animate.pose";
pub const VERSION: u64 = 1;

/// Most keys in one layer and frames a key may sit on; a file asking for more is refused.
pub const MAX_KEYS: usize = 100_000;
pub const MAX_FRAME: i64 = 10_000_000;

type R<T> = Result<T, String>;

fn num(v: &Value, what: &str) -> R<f64> {
    v.as_f64()
        .filter(|x| x.is_finite())
        .ok_or_else(|| format!("{what}: expected a finite number"))
}

fn arr<const N: usize>(v: &Value, what: &str) -> R<[f64; N]> {
    let a = v
        .as_array()
        .ok_or_else(|| format!("{what}: expected {N} numbers"))?;
    if a.len() != N {
        return Err(format!("{what}: expected {N} numbers, got {}", a.len()));
    }
    let mut out = [0.0; N];
    for (o, x) in out.iter_mut().zip(a) {
        *o = num(x, what)?;
    }
    Ok(out)
}

fn text(v: &Map<String, Value>, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn bone_list(names: &[String], v: Option<&Value>, what: &str) -> R<Option<Vec<usize>>> {
    let Some(v) = v else { return Ok(None) };
    let a = v
        .as_array()
        .ok_or_else(|| format!("{what}: expected a list of bone names"))?;
    let mut out = Vec::with_capacity(a.len());
    for b in a {
        let n = b.as_str().ok_or_else(|| format!("{what}: bone names are text"))?;
        out.push(
            names
                .iter()
                .position(|x| x == n)
                .ok_or_else(|| format!("{what}: unknown bone {n:?}"))?,
        );
    }
    Ok(Some(out))
}

fn keys_of(names: &[String], v: Option<&Value>) -> R<Vec<Key>> {
    let items = match v {
        None => return Ok(Vec::new()),
        Some(v) => v.as_array().ok_or("keys: expected a list")?,
    };
    if items.len() > MAX_KEYS {
        return Err(format!("{} keys in one layer (most {MAX_KEYS})", items.len()));
    }
    let mut out: Vec<Key> = Vec::with_capacity(items.len());
    for (i, k) in items.iter().enumerate() {
        let k = k
            .as_object()
            .ok_or_else(|| format!("key {i}: expected an object"))?;
        let frame = k
            .get("frame")
            .and_then(Value::as_i64)
            .ok_or_else(|| format!("key {i}: frame"))?;
        if !(0..=MAX_FRAME).contains(&frame) {
            return Err(format!("key {i}: frame {frame} outside 0..={MAX_FRAME}"));
        }
        let rv = k
            .get("rotations")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("key {i}: rotations"))?;
        if rv.len() != names.len() {
            return Err(format!(
                "key {i}: {} rotations for {} bones",
                rv.len(),
                names.len()
            ));
        }
        let rots: Vec<Quat> = rv
            .iter()
            .map(|q| arr::<4>(q, &format!("key {i} rotation")))
            .collect::<R<_>>()?;
        let root = arr::<3>(k.get("root").unwrap_or(&Value::Null), &format!("key {i} root"))?;
        let interp_name = text(k, "interp");
        let interp = Interp::parse(&interp_name)
            .ok_or_else(|| format!("key {i}: unknown interpolation {interp_name:?}"))?;
        let mut key =
            Key::new(frame, rots, root, interp).with_only(bone_list(names, k.get("only"), "key only")?);
        if key.only.as_ref().is_some_and(Vec::is_empty) {
            return Err(format!("key {i}: a partial key needs at least one bone"));
        }
        match k.get("source").and_then(Value::as_str).unwrap_or("manual") {
            "manual" => {}
            "ai" => {
                key.source = KeySource::Ai;
                key.model = text(k, "model");
            }
            other => return Err(format!("key {i}: unknown source {other:?}")),
        }
        out.retain(|o| o.frame != key.frame);
        out.push(key);
    }
    out.sort_by_key(|k| k.frame);
    Ok(out)
}

/// A clip from a file's text.
pub fn load_clip(text_in: &str) -> R<Clip> {
    let v: Value = serde_json::from_str(text_in).map_err(|e| format!("not JSON: {e}"))?;
    let d = v.as_object().ok_or("a clip file is a JSON object")?;
    let format = text(d, "format");
    if !product::is_format(&format, "clip") && format != LEGACY_CLIP {
        return Err(format!("not a clip file (format {format:?})"));
    }
    if d.get("version").and_then(Value::as_u64) != Some(VERSION) {
        return Err(format!("unsupported clip version {:?}", d.get("version")));
    }
    let names: Vec<String> = d
        .get("bones")
        .and_then(Value::as_array)
        .ok_or("bones missing")?
        .iter()
        .map(|b| b.as_str().map(str::to_string).ok_or("bone names are text"))
        .collect::<Result<_, _>>()?;
    let fps = num(d.get("fps").unwrap_or(&Value::Null), "fps")?;
    if !(1.0..=1000.0).contains(&fps) {
        return Err(format!("fps {fps} outside 1..=1000"));
    }
    let mut clip = Clip::new(names.clone(), fps);
    clip.loop_ = d.get("loop").and_then(Value::as_bool).unwrap_or(false);
    clip.name = text(d, "name");
    clip.group = text(d, "group");
    clip.origin = text(d, "origin");
    clip.physics = d.get("physics").and_then(Value::as_bool).unwrap_or(true);
    for s in d.get("surfaces").and_then(Value::as_array).into_iter().flatten() {
        let s = s.as_object().ok_or("surface: expected an object")?;
        clip.surfaces.push(Surface {
            name: text(s, "name"),
            min: arr::<2>(s.get("min").unwrap_or(&Value::Null), "surface min")?,
            max: arr::<2>(s.get("max").unwrap_or(&Value::Null), "surface max")?,
            top: num(s.get("top").unwrap_or(&Value::Null), "surface top")?,
            yaw: match s.get("yaw") {
                None => 0.0,
                Some(v) => num(v, "surface yaw")?,
            },
        });
    }
    clip.markers = markers_of(d.get("markers"))?;
    clip.contacts = contacts_of(&names, d.get("contacts"))?;
    clip.evaluation_only = d.get("evaluation_only").and_then(Value::as_bool).unwrap_or(false);
    if let Some(base) = clip.layers.first_mut() {
        base.keys = keys_of(&names, d.get("keys"))?;
    }
    for (i, l) in d
        .get("layers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let l = l
            .as_object()
            .ok_or_else(|| format!("layer {i}: expected an object"))?;
        let mut lay = Layer::keys(&text(l, "name"), keys_of(&names, l.get("keys"))?);
        lay.source = match text(l, "source").as_str() {
            "keys" | "" => LayerSource::Keys,
            "link" => LayerSource::Link,
            "travel" => LayerSource::Travel,
            other => return Err(format!("layer {i}: unknown source {other:?}")),
        };
        lay.mode = match text(l, "mode").as_str() {
            "override" | "" => LayerMode::Override,
            "additive" => LayerMode::Additive,
            other => return Err(format!("layer {i}: unknown mode {other:?}")),
        };
        lay.link = text(l, "link");
        lay.only = bone_list(&names, l.get("only"), "layer only")?;
        lay.parts = l
            .get("parts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|p| p.as_str().map(str::to_string))
            .collect();
        lay.weight = l
            .get("weight")
            .and_then(Value::as_f64)
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        lay.offset = l
            .get("offset")
            .and_then(Value::as_f64)
            .filter(|x| x.is_finite())
            .unwrap_or(0.0);
        lay.speed = l.get("speed").and_then(Value::as_f64).unwrap_or(1.0);
        if !(lay.speed.is_finite() && lay.speed > 0.0) {
            return Err(format!("layer {i}: speed must be above 0"));
        }
        lay.loop_ = l.get("loop").and_then(Value::as_bool).unwrap_or(true);
        lay.muted = l.get("muted").and_then(Value::as_bool).unwrap_or(false);
        lay.start = l.get("start").and_then(Value::as_f64);
        lay.end = l.get("end").and_then(Value::as_f64);
        lay.fade = l.get("fade").and_then(Value::as_i64).unwrap_or(0).max(0);
        lay.gait = l
            .get("gait")
            .and_then(Value::as_str)
            .unwrap_or("auto")
            .to_string();
        for w in l.get("waypoints").and_then(Value::as_array).into_iter().flatten() {
            lay.waypoints.push(arr::<2>(w, "waypoint")?);
        }
        if let Some(m) = l.get("moves").and_then(Value::as_object) {
            for (k, v) in m {
                lay.moves.insert(k.clone(), arr::<2>(v, "footstep move")?);
            }
        }
        lay.steps = l
            .get("steps")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        clip.layers.push(lay);
    }
    Ok(clip)
}

/// The held hands and feet (absent in older files: none).
fn contacts_of(names: &[String], v: Option<&Value>) -> R<Vec<crate::clip::Contact>> {
    let mut out = Vec::new();
    for (i, c) in v.and_then(Value::as_array).into_iter().flatten().enumerate() {
        let c = c
            .as_object()
            .ok_or_else(|| format!("contact {i}: expected an object"))?;
        let bone = text(c, "bone");
        if !names.contains(&bone) || crate::physics::held_limb(&bone).is_none() {
            return Err(format!("contact {i}: {bone:?} is not a hand or foot of the rig"));
        }
        let frame = |name: &str| {
            c.get(name)
                .and_then(Value::as_i64)
                .ok_or_else(|| format!("contact {i}: {name}"))
        };
        let (from, to) = (frame("from")?, frame("to")?);
        if to < from {
            return Err(format!("contact {i}: frames {from}–{to}"));
        }
        let at = arr::<3>(c.get("at").unwrap_or(&Value::Null), "contact at")?;
        out.push(crate::clip::Contact { bone, from, to, at });
    }
    Ok(out)
}

/// The timeline markers (absent in older files: none). A file with two on one frame, or a bad
/// name, was not written by us and is refused.
fn markers_of(v: Option<&Value>) -> R<Vec<crate::markers::Marker>> {
    let mut clip = Clip::new(Vec::new(), 30.0);
    let Some(v) = v else { return Ok(clip.markers) };
    let items = v.as_array().ok_or("markers: expected a list")?;
    for (i, m) in items.iter().enumerate() {
        let m = m
            .as_object()
            .ok_or_else(|| format!("marker {i}: expected an object"))?;
        let frame = m
            .get("frame")
            .and_then(Value::as_i64)
            .ok_or_else(|| format!("marker {i}: frame"))?;
        crate::markers::add(&mut clip, frame, &text(m, "name")).map_err(|e| format!("marker {i}: {e}"))?;
    }
    Ok(clip.markers)
}

fn keys_json(bones: &[String], keys: &[Key]) -> Value {
    keys.iter()
        .map(|k| {
            let mut m = Map::new();
            m.insert("frame".into(), json!(k.frame));
            m.insert("interp".into(), json!(k.interp.name()));
            if k.source == KeySource::Ai {
                m.insert("source".into(), json!("ai"));
                m.insert("model".into(), json!(k.model));
            }
            if let Some(only) = &k.only {
                m.insert(
                    "only".into(),
                    json!(only.iter().filter_map(|&i| bones.get(i)).collect::<Vec<_>>()),
                );
            }
            m.insert("root".into(), json!(k.root));
            m.insert("rotations".into(), json!(k.rotations));
            Value::Object(m)
        })
        .collect()
}

/// A clip as `CLIP_FORMAT` JSON (keys sorted, one space indent).
pub fn save_clip(clip: &Clip) -> String {
    let mut d = Map::new();
    d.insert("format".into(), json!(CLIP_FORMAT));
    d.insert("version".into(), json!(VERSION));
    d.insert("name".into(), json!(clip.name));
    if !clip.group.is_empty() {
        d.insert("group".into(), json!(clip.group));
    }
    if !clip.origin.is_empty() {
        d.insert("origin".into(), json!(clip.origin));
    }
    if !clip.physics {
        d.insert("physics".into(), json!(false));
    }
    if !clip.surfaces.is_empty() {
        d.insert(
            "surfaces".into(),
            clip.surfaces
                .iter()
                .map(|s| {
                    let mut o = json!({"name": s.name, "min": s.min, "max": s.max, "top": s.top});
                    if s.yaw != 0.0 {
                        o["yaw"] = json!(s.yaw);
                    }
                    o
                })
                .collect(),
        );
    }
    if !clip.markers.is_empty() {
        d.insert("markers".into(), crate::markers::json(clip));
    }
    if !clip.contacts.is_empty() {
        d.insert(
            "contacts".into(),
            clip.contacts
                .iter()
                .map(|c| json!({"bone": c.bone, "from": c.from, "to": c.to, "at": c.at}))
                .collect(),
        );
    }
    if clip.evaluation_only {
        d.insert("evaluation_only".into(), json!(true));
    }
    d.insert("fps".into(), json!(clip.fps));
    d.insert("loop".into(), json!(clip.loop_));
    d.insert("bones".into(), json!(clip.bones));
    d.insert("keys".into(), keys_json(&clip.bones, &clip.base().keys));
    if clip.layers.len() > 1 {
        let layers: Vec<Value> = clip
            .layers
            .iter()
            .skip(1)
            .map(|l| layer_json(&clip.bones, l))
            .collect();
        d.insert("layers".into(), Value::Array(layers));
    }
    pretty(&Value::Object(d))
}

fn layer_json(bones: &[String], l: &Layer) -> Value {
    let mut m = Map::new();
    m.insert("name".into(), json!(l.name));
    let source = match l.source {
        LayerSource::Keys => "keys",
        LayerSource::Link => "link",
        LayerSource::Travel => "travel",
    };
    m.insert("source".into(), json!(source));
    if l.source == LayerSource::Link {
        m.insert("link".into(), json!(l.link));
    } else {
        m.insert("keys".into(), keys_json(bones, &l.keys));
    }
    if let Some(only) = &l.only {
        m.insert(
            "only".into(),
            json!(only.iter().filter_map(|&i| bones.get(i)).collect::<Vec<_>>()),
        );
    }
    if !l.parts.is_empty() {
        m.insert("parts".into(), json!(l.parts));
    }
    let mode = if l.mode == LayerMode::Additive {
        "additive"
    } else {
        "override"
    };
    m.insert("mode".into(), json!(mode));
    m.insert("weight".into(), json!(l.weight));
    m.insert("offset".into(), json!(l.offset));
    m.insert("speed".into(), json!(l.speed));
    m.insert("loop".into(), json!(l.loop_));
    m.insert("muted".into(), json!(l.muted));
    if let (Some(s), Some(e)) = (l.start, l.end) {
        m.insert("start".into(), json!(s));
        m.insert("end".into(), json!(e));
        m.insert("fade".into(), json!(l.fade));
    }
    if l.source == LayerSource::Travel {
        m.insert("gait".into(), json!(l.gait));
    }
    if !l.waypoints.is_empty() {
        m.insert("waypoints".into(), json!(l.waypoints));
    }
    if !l.moves.is_empty() {
        m.insert("moves".into(), json!(l.moves));
    }
    if !l.steps.is_empty() {
        m.insert("steps".into(), json!(l.steps));
    }
    Value::Object(m)
}

fn pretty(v: &Value) -> String {
    // serde_json maps are sorted (BTreeMap), so the output is stable
    serde_json::to_string_pretty(v).unwrap_or_default()
}

/// A saved pose: one rotation per bone and the root, optionally only some bones.
#[derive(Debug, Clone, PartialEq)]
pub struct PoseFile {
    pub name: String,
    pub group: String,
    pub bones: Vec<String>,
    pub rotations: Vec<Quat>,
    pub root: Vec3,
    pub only: Option<Vec<usize>>,
    /// See [`crate::clip::Clip::evaluation_only`].
    pub evaluation_only: bool,
}

pub fn load_pose(text_in: &str) -> R<PoseFile> {
    let v: Value = serde_json::from_str(text_in).map_err(|e| format!("not JSON: {e}"))?;
    let d = v.as_object().ok_or("a pose file is a JSON object")?;
    let format = text(d, "format");
    if !product::is_format(&format, "pose") && format != LEGACY_POSE {
        return Err(format!("not a pose file (format {format:?})"));
    }
    let bones: Vec<String> = d
        .get("bones")
        .and_then(Value::as_array)
        .ok_or("bones missing")?
        .iter()
        .map(|b| b.as_str().map(str::to_string).ok_or("bone names are text"))
        .collect::<Result<_, _>>()?;
    let rv = d
        .get("rotations")
        .and_then(Value::as_array)
        .ok_or("rotations missing")?;
    if rv.len() != bones.len() {
        return Err(format!("{} rotations for {} bones", rv.len(), bones.len()));
    }
    let rotations = rv.iter().map(|q| arr::<4>(q, "rotation")).collect::<R<_>>()?;
    let root = arr::<3>(d.get("root").unwrap_or(&Value::Null), "root")?;
    let only = bone_list(&bones, d.get("only"), "only")?;
    Ok(PoseFile {
        name: text(d, "name"),
        group: text(d, "group"),
        bones,
        rotations,
        root,
        only,
        evaluation_only: d.get("evaluation_only").and_then(Value::as_bool).unwrap_or(false),
    })
}

pub fn save_pose(p: &PoseFile) -> String {
    let mut d = BTreeMap::new();
    d.insert("format", json!(POSE_FORMAT));
    d.insert("version", json!(VERSION));
    d.insert("name", json!(p.name));
    d.insert("group", json!(p.group));
    d.insert("bones", json!(p.bones));
    d.insert("rotations", json!(p.rotations));
    d.insert("root", json!(p.root));
    if p.evaluation_only {
        d.insert("evaluation_only", json!(true));
    }
    if let Some(only) = &p.only {
        d.insert(
            "only",
            json!(only.iter().filter_map(|&i| p.bones.get(i)).collect::<Vec<_>>()),
        );
    }
    pretty(&json!(d))
}
