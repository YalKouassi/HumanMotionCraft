//! Project files: the whole working document in one self-contained JSON file, so it can be sent
//! to someone or kept outside the library.
//!
//! ```text
//! {
//!   "format": "<id>.project", "version": 1,
//!   "clip": <the open animation, as a clip file>,
//!   "links": { "<library id>": <a clip file>, ... },   every clip the link layers use
//!   "edit": 0, "frame": 4.0,                            the layer keys go on; the playhead
//!   "pins": { "<bone>": [x, y, z] }, "locked": ["<part>"],
//!   "ai": { "style", "style_force", "ai_force", "trajectory", "head" }
//! }
//! ```
//!
//! `project.save` writes it (`path` defaults to the file the document came from), `project.open`
//! replaces the document with it (one undo step; the document then belongs to the project:
//! `clip_id` is cleared, Save goes to the file, and link layers play the embedded clips, not the
//! library's), `project.info` reads one without opening it. Not in a project: the library, the
//! preferences, derived data. The file's keys are sorted and its clips are written by
//! `clipfile::save_clip`, so the same document always gives the same bytes.
//!
//! Versions: 0 was the format's first cut (links as a `[{id, clip}]` list, the playhead as
//! `playhead`, no `ai` block); [`migrate`] brings it to 1. A file from a newer version is refused.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ac_core::ai::FillSettings;
use ac_core::clip::{Clip, LayerSource};
use ac_core::clipfile;
use ac_core::command::{self as cmd, CommandSpec, Kind as K, ParamSpec};
use ac_core::{Document, ProjectFile, product};
use serde_json::{Map, Value, json};

use crate::import::MAX_FILE_BYTES;
use crate::returns;

type P = Map<String, Value>;
type Out = Result<Value, String>;
type R<T> = Result<T, String>;

/// The format's kind: files say `format: "<product id>.project"`.
pub const KIND: &str = "project";
/// The version this build writes.
pub const VERSION: u64 = 1;
/// The file name ending the app's dialogs use (any name reads: the `format` field decides).
pub const EXT: &str = ".project.json";
/// Most linked clips one project may embed.
pub const MAX_LINKS: usize = 256;

const PATH: ParamSpec = ParamSpec::new("path", K::Text, "a project file (.project.json)");

pub static SPECS: &[CommandSpec] = &[
    CommandSpec::new("project.open", "Open project", project_open)
        .icon("open")
        .shortcut("Ctrl+Shift+O")
        .undoable()
        .inputs(&[PATH.required()])
        .outputs(returns::PROJECT_OPEN),
    CommandSpec::new("project.save", "Save project", project_save)
        .icon("save")
        .shortcut("Ctrl+Shift+S")
        .inputs(&[ParamSpec::new(
            "path",
            K::Text,
            "where to write it (default: the file the project was opened from or saved to)",
        )])
        .writes()
        .outputs(returns::PROJECT_SAVE),
    CommandSpec::new("project.info", "Project file info", project_info)
        .icon("info")
        .inputs(&[PATH.required()])
        .outputs(returns::PROJECT_INFO),
];

/// What a project file holds, read and checked (bones against a rig are the caller's check).
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    pub clip: Clip,
    pub links: BTreeMap<String, Clip>,
    pub edit: usize,
    pub frame: f64,
    pub pins: BTreeMap<String, [f64; 3]>,
    pub locked: Vec<String>,
    pub ai: FillSettings,
    /// The version the file was written with.
    pub version: u64,
}

impl Loaded {
    /// Whether opening it migrates (the file is older than [`VERSION`]).
    pub fn migrated(&self) -> bool {
        self.version != VERSION
    }
}

/// The ids of the clips `clip`'s link layers use, and theirs, out of `links` (a clip a link names
/// but `links` lacks is simply not there: the layer says so in the UI).
pub fn reachable(clip: &Clip, links: &BTreeMap<String, Clip>) -> BTreeMap<String, Clip> {
    let mut out = BTreeMap::new();
    let mut todo: Vec<String> = link_ids(clip);
    while let Some(id) = todo.pop() {
        if out.contains_key(&id) {
            continue;
        }
        let Some(c) = links.get(&id) else { continue };
        todo.extend(link_ids(c));
        out.insert(id, c.clone());
    }
    out
}

fn link_ids(clip: &Clip) -> Vec<String> {
    clip.layers
        .iter()
        .filter(|l| l.source == LayerSource::Link && !l.link.is_empty())
        .map(|l| l.link.clone())
        .collect()
}

fn clip_value(clip: &Clip) -> R<Value> {
    serde_json::from_str(&clipfile::save_clip(clip)).map_err(|e| format!("the clip didn't serialise: {e}"))
}

/// The document as a project file's text: the open clip, the linked clips it reaches, and the
/// working state. Keys sorted, so the same document gives the same bytes.
pub fn save_text(doc: &Document) -> R<String> {
    let links = reachable(&doc.clip, &doc.links);
    let mut d = Map::new();
    d.insert("format".into(), json!(product::format(KIND)));
    d.insert("version".into(), json!(VERSION));
    d.insert("clip".into(), clip_value(&doc.clip)?);
    let mut embedded = Map::new();
    for (id, c) in &links {
        embedded.insert(id.clone(), clip_value(c)?);
    }
    d.insert("links".into(), Value::Object(embedded));
    d.insert("edit".into(), json!(doc.clip.edit));
    d.insert("frame".into(), json!(doc.frame));
    d.insert("pins".into(), json!(doc.pins));
    d.insert("locked".into(), json!(doc.locked));
    d.insert("ai".into(), json!(doc.ai_settings));
    serde_json::to_string_pretty(&Value::Object(d)).map_err(|e| format!("the project didn't serialise: {e}"))
}

fn text_of(m: &Map<String, Value>, key: &str) -> String {
    m.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn number(v: Option<&Value>, what: &str, lo: f64, hi: f64, default: f64) -> R<f64> {
    let Some(v) = v else { return Ok(default) };
    let x = v
        .as_f64()
        .filter(|x| x.is_finite())
        .ok_or_else(|| format!("{what}: expected a number"))?;
    if !(lo..=hi).contains(&x) {
        return Err(format!("{what}: {x} is outside {lo}..={hi}"));
    }
    Ok(x)
}

/// A version-0 file as version 1: `links` from a `[{id, clip}]` list to a map by id, `playhead`
/// to `frame`. Any other version is returned as it is.
pub fn migrate(mut v: Value) -> R<Value> {
    if v.get("version").and_then(Value::as_u64) != Some(0) {
        return Ok(v);
    }
    let d = v.as_object_mut().ok_or("a project file is a JSON object")?;
    if let Some(list) = d.remove("links") {
        let items = list.as_array().ok_or("version 0 links: expected a list")?;
        let mut map = Map::new();
        for (i, it) in items.iter().enumerate() {
            let it = it
                .as_object()
                .ok_or_else(|| format!("version 0 link {i}: expected {{id, clip}}"))?;
            let id = it
                .get("id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| format!("version 0 link {i}: id"))?;
            let clip = it
                .get("clip")
                .cloned()
                .ok_or_else(|| format!("version 0 link {i}: clip"))?;
            if map.insert(id.to_string(), clip).is_some() {
                return Err(format!("version 0 link {i}: {id:?} twice"));
            }
        }
        d.insert("links".into(), Value::Object(map));
    }
    if let Some(f) = d.remove("playhead") {
        d.insert("frame".into(), f);
    }
    d.insert("version".into(), json!(1));
    Ok(v)
}

/// A project from a file's text (checked; a version-0 file migrated).
pub fn load_text(text: &str) -> R<Loaded> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("not a project file: not JSON ({e})"))?;
    let d = v.as_object().ok_or("not a project file: not a JSON object")?;
    let format = text_of(d, "format");
    if !product::is_format(&format, KIND) {
        return Err(if product::is_format(&format, "clip") {
            "this is an animation file, not a project (clip.import opens it)".into()
        } else {
            format!("not a project file (format {format:?})")
        });
    }
    let version = d
        .get("version")
        .and_then(Value::as_u64)
        .ok_or("not a project file: no version")?;
    if version > VERSION {
        return Err(format!(
            "written by a newer version of {} (project version {version}; this one reads up to {VERSION})",
            product::NAME
        ));
    }
    let v = migrate(v)?;
    let d = v.as_object().ok_or("a project file is a JSON object")?;
    let clip_v = d.get("clip").ok_or("a project file without a clip")?;
    let clip = clipfile::load_clip(&clip_v.to_string()).map_err(|e| format!("clip: {e}"))?;
    let mut links = BTreeMap::new();
    if let Some(l) = d.get("links") {
        let items = l.as_object().ok_or("links: expected an object by id")?;
        if items.len() > MAX_LINKS {
            return Err(format!("{} linked clips (most {MAX_LINKS})", items.len()));
        }
        for (id, c) in items {
            let c = clipfile::load_clip(&c.to_string()).map_err(|e| format!("linked {id}: {e}"))?;
            if c.bones != clip.bones {
                return Err(format!("linked {id}: made for a different rig"));
            }
            links.insert(id.clone(), c);
        }
    }
    let edit = match d.get("edit") {
        None => 0,
        Some(e) => e
            .as_u64()
            .and_then(|e| usize::try_from(e).ok())
            .filter(|e| *e < clip.layers.len())
            .ok_or_else(|| format!("edit: no such layer (there are {})", clip.layers.len()))?,
    };
    let frame = number(d.get("frame"), "frame", 0.0, clipfile::MAX_FRAME as f64, 0.0)?;
    let mut pins = BTreeMap::new();
    if let Some(p) = d.get("pins") {
        for (bone, at) in p.as_object().ok_or("pins: expected an object by bone")? {
            let a = at
                .as_array()
                .filter(|a| a.len() == 3)
                .ok_or_else(|| format!("pin {bone}: expected 3 numbers"))?;
            let mut xyz = [0.0; 3];
            for (o, x) in xyz.iter_mut().zip(a) {
                *o = number(Some(x), &format!("pin {bone}"), -1e4, 1e4, 0.0)?;
            }
            pins.insert(bone.clone(), xyz);
        }
    }
    let locked: Vec<String> = d
        .get("locked")
        .map(|l| {
            l.as_array()
                .ok_or("locked: expected a list of parts")?
                .iter()
                .map(|x| {
                    x.as_str()
                        .map(str::to_string)
                        .ok_or("locked: part names are text")
                })
                .collect::<Result<Vec<_>, &str>>()
                .map_err(str::to_string)
        })
        .transpose()?
        .unwrap_or_default();
    let mut ai = FillSettings::default();
    if let Some(a) = d.get("ai") {
        let a = a.as_object().ok_or("ai: expected an object")?;
        if let Some(s) = a.get("style") {
            ai.style = s.as_str().ok_or("ai.style: expected text")?.to_string();
        }
        ai.style_force = number(a.get("style_force"), "ai.style_force", 0.0, 3.0, ai.style_force)?;
        ai.ai_force = number(a.get("ai_force"), "ai.ai_force", 0.0, 1.5, ai.ai_force)?;
        ai.trajectory = number(a.get("trajectory"), "ai.trajectory", 0.0, 1.0, ai.trajectory)?;
        ai.head = number(a.get("head"), "ai.head", 0.0, 1.0, ai.head)?;
    }
    Ok(Loaded {
        clip,
        links,
        edit,
        frame,
        pins,
        locked,
        ai,
        version,
    })
}

/// A file's bytes, size-capped.
fn read(path: &Path) -> R<String> {
    let size = std::fs::metadata(path)
        .map_err(|e| format!("can't read {}: {e}", path.display()))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(format!(
            "{}: {size} bytes is larger than {MAX_FILE_BYTES}",
            path.display()
        ));
    }
    std::fs::read_to_string(path).map_err(|e| format!("can't read {}: {e}", path.display()))
}

fn path_param(p: &P) -> R<PathBuf> {
    let path = cmd::text(p, "path").unwrap_or_default().trim();
    if path.is_empty() {
        return Err("path: give the project file".into());
    }
    Ok(PathBuf::from(path))
}

fn shown(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn ids(links: &BTreeMap<String, Clip>) -> Vec<&String> {
    links.keys().collect()
}

/// Write the document to `path` (default: its own project file). The document then belongs to
/// the project: `clip_id` is cleared and the links it doesn't reach are dropped.
fn project_save(doc: &mut Document, p: &P) -> Out {
    let path = match cmd::text(p, "path").map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => PathBuf::from(s),
        None => doc.project.as_ref().map(|pr| pr.path.clone()).ok_or_else(|| {
            ac_core::error::hinted(
                "this animation has no project file yet",
                "give path: where to write the project",
            )
        })?,
    };
    let text = save_text(doc)?;
    craft_atomic::atomic_write(&path, text.as_bytes())
        .map_err(|e| format!("can't write {}: {e}", shown(&path)))?;
    let links = reachable(&doc.clip, &doc.links);
    doc.links = links.clone();
    doc.clip_id = None;
    doc.project = Some(Arc::new(ProjectFile {
        path: path.clone(),
        clip: doc.clip.clone(),
        links,
        ai_settings: doc.ai_settings.clone(),
    }));
    Ok(json!({
        "path": shown(&path),
        "bytes": text.len(),
        "hash": ac_core::hash::of_bytes(text.as_bytes()),
        "links": ids(&doc.links),
        "clips": 1 + doc.links.len(),
    }))
}

/// Replace the document with a project file's (one undo step).
fn project_open(doc: &mut Document, p: &P) -> Out {
    let path = path_param(p)?;
    let text = read(&path)?;
    let loaded = load_text(&text).map_err(|e| format!("{}: {e}", shown(&path)))?;
    if loaded.clip.bones != doc.skeleton.names {
        return Err(format!(
            "{}: this project is for a different rig ({} bones; this rig has {})",
            shown(&path),
            loaded.clip.bones.len(),
            doc.skeleton.len()
        ));
    }
    for bone in loaded.pins.keys() {
        doc.bone(bone).map_err(|e| format!("pin: {e}"))?;
    }
    doc.part_bones(&loaded.locked)
        .map_err(|e| format!("locked: {e}"))?;
    let mut notes = Vec::new();
    if let Some(model) = &doc.ai
        && loaded.ai.style != "any"
        && !model.info().styles.contains(&loaded.ai.style)
    {
        notes.push(format!(
            "the AI style {:?} is not one the loaded model knows: fills use no style until ai.settings picks one",
            loaded.ai.style
        ));
    }
    let Loaded {
        mut clip,
        links,
        edit,
        frame,
        pins,
        locked,
        ai,
        version,
    } = loaded;
    clip.edit = edit;
    doc.project = Some(Arc::new(ProjectFile {
        path: path.clone(),
        clip: clip.clone(),
        links: links.clone(),
        ai_settings: ai.clone(),
    }));
    doc.clip = clip;
    doc.links = links;
    doc.clip_id = None;
    doc.pins = pins;
    doc.locked = locked;
    doc.ai_settings = ai;
    doc.ai_preview = None;
    doc.go_to(frame)?;
    Ok(json!({
        "path": shown(&path),
        "name": doc.clip.name,
        "length": doc.clip.length(),
        "frame": doc.frame,
        "links": ids(&doc.links),
        "layers": doc.clip.layers.len(),
        "boxes": doc.clip.surfaces.len(),
        "version": version,
        "migrated": version != VERSION,
        "notes": notes,
    }))
}

/// What a project file holds, without opening it.
fn project_info(_: &mut Document, p: &P) -> Out {
    let path = path_param(p)?;
    let text = read(&path)?;
    let loaded = load_text(&text).map_err(|e| format!("{}: {e}", shown(&path)))?;
    Ok(json!({
        "path": shown(&path),
        "version": loaded.version,
        "current": loaded.version == VERSION,
        "name": loaded.clip.name,
        "group": loaded.clip.group,
        "fps": loaded.clip.fps,
        "frames": loaded.clip.length(),
        "layers": loaded.clip.layers.len(),
        "boxes": loaded.clip.surfaces.len(),
        "links": ids(&loaded.links),
        "clips": 1 + loaded.links.len(),
        "bytes": text.len(),
        "hash": ac_core::hash::of_bytes(text.as_bytes()),
    }))
}
