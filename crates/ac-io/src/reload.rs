//! Reloading what the app read from disk at start-up, after it changed while the app runs (the
//! window watches the files and runs this): the joint limits, the library (part sets, the open
//! animation's linked clips, the item list) and the sample animations and their recipes.
//!
//! A file that no longer reads refuses the command and the old data stays in use: a front end
//! says so, and the person can fix the file and save it again.

use std::path::Path;
use std::sync::Arc;

use ac_core::Document;
use ac_core::clipfile;
use ac_core::command::{self as cmd, CommandSpec, Kind as K, ParamSpec};
use serde_json::{Map, Value, json};

use crate::commands::{anims, load_links, load_parts, poses};

type P = Map<String, Value>;
type Out = Result<Value, String>;

/// What can be reloaded.
pub const WHAT: &[&str] = &["limits", "library", "samples"];

pub static SPECS: &[CommandSpec] = &[CommandSpec::new("assets.reload", "Reload assets", assets_reload)
    .icon("restore")
    .inputs(&[ParamSpec::new(
        "what",
        K::TextList,
        "any of limits, library, samples (default: all of them)",
    )])
    .outputs(crate::returns::ASSETS_RELOAD)];

fn assets(doc: &Document) -> Result<&Path, String> {
    doc.assets
        .as_deref()
        .ok_or_else(|| "no assets folder is set (start the app with one)".to_string())
}

fn reload_limits(doc: &mut Document) -> Out {
    let path = assets(doc)?.join("rig/limits.json");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
    let limits = ac_core::fullbody::Limits::from_json(&text, &doc.skeleton)
        .map_err(|e| format!("{}: {e} (the joint limits in use are kept)", path.display()))?;
    let bones = limits.lo.len();
    doc.limits = Some(Arc::new(limits));
    let ranges = crate::setup::load_ranges(assets(doc)?, &doc.skeleton)
        .map_err(|e| format!("{e} (the tables in use are kept)"))?;
    doc.ranges = Some(Arc::new(ranges));
    Ok(json!({ "bones": bones }))
}

fn reload_library(doc: &mut Document) -> Out {
    load_parts(doc)?;
    let lib = anims(doc)?;
    doc.links = load_links(&lib, &doc.clip)?;
    ac_core::evaluation::mark_from_links(doc);
    if doc.clip.layers.iter().any(|l| !l.link.is_empty()) {
        doc.go_to(doc.frame)?;
    }
    Ok(json!({
        "animations": lib.items().len(),
        "poses": poses(doc)?.items().len(),
        "part_sets": doc.custom_parts.len(),
    }))
}

/// The shipped samples (`<assets>/samples/*.clip.json`) and their recipes (`recipes/*.json`)
/// still read; a recipe is `{name, script}`.
fn reload_samples(doc: &mut Document) -> Out {
    let dir = assets(doc)?.join(crate::import::SAMPLES);
    let files = |d: &Path, ext: &str| -> Vec<std::path::PathBuf> {
        let mut v: Vec<_> = std::fs::read_dir(d)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.to_string_lossy().ends_with(ext))
            .collect();
        v.sort();
        v
    };
    let read = |p: &Path| std::fs::read_to_string(p).map_err(|e| format!("can't read {}: {e}", p.display()));
    let samples = files(&dir, ".clip.json");
    for s in &samples {
        let clip = clipfile::load_clip(&read(s)?).map_err(|e| format!("{}: {e}", s.display()))?;
        if clip.bones != doc.skeleton.names {
            return Err(format!("{}: made for a different rig", s.display()));
        }
    }
    let recipes = files(&dir.join("recipes"), ".json");
    for r in &recipes {
        let v: Value = serde_json::from_str(&read(r)?).map_err(|e| format!("{}: {e}", r.display()))?;
        if v.get("name").and_then(Value::as_str).is_none()
            || v.get("script").and_then(Value::as_array).is_none()
        {
            return Err(format!("{}: a recipe is {{name, script}}", r.display()));
        }
    }
    Ok(json!({ "samples": samples.len(), "recipes": recipes.len() }))
}

/// Reload what's asked (default all) and say what was reloaded, with counts.
fn assets_reload(doc: &mut Document, p: &P) -> Out {
    let mut what = cmd::texts(p, "what");
    if what.is_empty() {
        what = WHAT.iter().map(|w| w.to_string()).collect();
    }
    if let Some(bad) = what.iter().find(|w| !WHAT.contains(&w.as_str())) {
        return Err(format!("{bad:?}: reload one of {WHAT:?}"));
    }
    let mut out = json!({ "reloaded": [] });
    let mut done = Vec::new();
    for w in WHAT.iter().filter(|w| what.iter().any(|x| x == *w)) {
        let r = match *w {
            "limits" => reload_limits(doc),
            "library" => reload_library(doc),
            _ => reload_samples(doc),
        }
        .map_err(|e| format!("reloading {w}: {e}"))?;
        out[*w] = r;
        done.push(*w);
    }
    out["reloaded"] = json!(done);
    Ok(out)
}
