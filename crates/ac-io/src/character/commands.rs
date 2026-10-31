//! The `retarget.*` commands: load a character (or a saved mapping), correct the mapping, check
//! how the open animation drives it, save the mapping, unload. The export is `clip.export`
//! with `target`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use ac_core::Document;
use ac_core::character::check::{self, Report};
use ac_core::character::plan::Plan;
use ac_core::command::{self as cmd, CommandSpec, Kind as K, ParamSpec};
use ac_core::contact;
use ac_core::physics::Body;
use serde_json::{Map, Value, json};

use crate::fbx::commands::joints_param;

type P = Map<String, Value>;
type Out = Result<Value, String>;

const JOINTS: ParamSpec = ParamSpec::new(
    "joints",
    K::Text,
    "landmark -> node name, correcting the guess ({\"l_knee\": \"L_Calf\"}; \"none\" unmaps one)",
)
.json();
const HANDS: ParamSpec = ParamSpec::new(
    "hands",
    K::Bool,
    "solve the arms onto the rig's wrists too (default: legs only)",
);

pub static SPECS: &[CommandSpec] = &[
    CommandSpec::new("retarget.load", "Load character", retarget_load)
        .icon("import")
        .undoable()
        .inputs(&[
            ParamSpec::new(
                "path",
                K::Text,
                "a character .glb (the mapping is guessed from its node names) or a saved mapping file (.retarget.json)",
            )
            .required(),
            JOINTS,
            HANDS,
        ])
        .outputs(crate::returns::RETARGET_LOAD),
    CommandSpec::new("retarget.map", "Map character joints", retarget_map)
        .icon("edit")
        .undoable()
        .inputs(&[JOINTS, HANDS])
        .outputs(crate::returns::RETARGET_LOAD),
    CommandSpec::new("retarget.check", "Check character retarget", retarget_check)
        .icon("graph")
        .outputs(crate::returns::RETARGET_CHECK),
    CommandSpec::new("retarget.save", "Save character mapping", retarget_save)
        .icon("save")
        .inputs(&[ParamSpec::new("path", K::Text, "the mapping file to write (.retarget.json)").required()])
        .writes()
        .outputs(crate::returns::RETARGET_SAVE),
    CommandSpec::new("retarget.unload", "Unload character", retarget_unload)
        .icon("close")
        .undoable()
        .outputs(crate::returns::RETARGET_UNLOAD),
];

fn hands_param(p: &P) -> Result<Option<bool>, String> {
    match p.get("hands") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_bool().map(Some).ok_or("hands: true or false".into()),
    }
}

fn retarget_load(doc: &mut Document, p: &P) -> Out {
    let path = PathBuf::from(cmd::text(p, "path").unwrap_or_default());
    if path.as_os_str().is_empty() {
        return Err("give the character file (path)".into());
    }
    let joints = joints_param(p)?;
    let hands = hands_param(p)?;
    let t = super::load(&path, &joints, hands)?;
    let out = super::report(&doc.skeleton, &t.loaded, &t.given, Some(&t.info));
    doc.character = Some(Arc::new(t.loaded));
    Ok(Value::Object(out))
}

fn loaded(doc: &Document) -> Result<Arc<ac_core::character::Loaded>, String> {
    doc.character.clone().ok_or_else(|| {
        ac_core::error::hinted(
            "no character is loaded",
            "retarget.load {path} loads a character .glb or a saved mapping",
        )
    })
}

fn retarget_map(doc: &mut Document, p: &P) -> Out {
    let cur = loaded(doc)?;
    let joints = joints_param(p)?;
    let hands = hands_param(p)?;
    if joints.is_empty() && hands.is_none() {
        return Err("give joints to correct, or hands".into());
    }
    let mut l = (*cur).clone();
    let given = super::correct(&l.character, &mut l.mapping, &joints)?;
    if let Some(h) = hands {
        l.mapping.hands = h;
    }
    l.notes.retain(|n| !n.starts_with("no node found for"));
    let missing = l.mapping.missing();
    if !missing.is_empty() {
        l.notes.push(format!(
            "no node found for {}: name them with joints before checking or exporting",
            missing.join(", ")
        ));
    }
    let out = super::report(&doc.skeleton, &l, &given, None);
    doc.character = Some(Arc::new(l));
    Ok(Value::Object(out))
}

/// The open animation as shown (physics where it plays) through the character, with the plan.
pub(crate) fn drive_document(doc: &mut Document) -> Result<(Plan, Report), String> {
    let l = loaded(doc)?;
    if doc.clip.base().keys.is_empty() {
        return Err(ac_core::error::hinted(
            "the animation has no keys to check",
            "open one with clip.open, or key poses",
        ));
    }
    let plan = Plan::new(&doc.skeleton, &l.character, &l.mapping)?;
    let n = doc.clip.length_with(&doc.links).max(1) as usize;
    if n > check::MAX_FRAMES {
        return Err(format!("{n} frames (most {})", check::MAX_FRAMES));
    }
    if doc.physics_shown() {
        doc.physics()?;
    }
    let frames: Vec<f64> = (0..n).map(|f| f as f64).collect();
    let poses = doc.shown_poses(&frames)?;
    let rots: Vec<Vec<ac_core::quat::Quat>> = poses.iter().map(|p| p.rotations.clone()).collect();
    let roots: Vec<ac_core::quat::Vec3> = poses.iter().map(|p| p.root).collect();
    let body = Body::new(&doc.skeleton)?;
    let phases = contact::clip_phases(
        &body,
        &doc.skeleton,
        &rots,
        &roots,
        doc.clip.fps,
        &doc.clip.surfaces,
    )?;
    let report = check::drive_clip(&plan, &l.character, &doc.skeleton, &rots, &roots, &phases)?;
    Ok((plan, report))
}

/// The check's result from a report.
pub(crate) fn check_json(doc: &Document, plan: &Plan, r: &Report) -> Value {
    let c = doc.character.as_ref().map(|l| &l.character);
    let names = |ix: &[usize]| -> Vec<String> {
        ix.iter()
            .filter_map(|&i| c.and_then(|c| c.nodes.get(i)).map(|n| n.name.clone()))
            .collect()
    };
    let foot = |f: &check::FootReport| {
        json!({
            "planted_frames": f.planted_frames,
            "spans": f.spans.iter().map(|(a, b, t)| json!({ "from": a, "to": b, "slide_cm": r4(t * 100.0) })).collect::<Vec<_>>(),
            "slide_cm": r4(f.slide * 100.0),
            "travel_cm": r4(f.travel * 100.0),
            "lowest_cm": r4(if f.lowest.is_finite() { f.lowest * 100.0 } else { 0.0 }),
            "unreached_frames": f.unreached_frames,
            "unreached_cm": r4(f.unreached_max * 100.0),
            "holds": holds(f),
        })
    };
    json!({
        "frames": r.frames.len(),
        "scale": r4(plan.scale),
        "driven": plan.drives.len(),
        "undriven": names(&plan.undriven),
        "feet": { "L": foot(&r.feet[0]), "R": foot(&r.feet[1]) },
        "problems": r.problems.iter().map(|p| json!({ "from": p.from, "to": p.to, "kind": p.kind, "label": p.text })).collect::<Vec<_>>(),
        "notes": plan.notes,
    })
}

fn r4(x: f64) -> f64 {
    (x * 1e4).round() / 1e4
}

/// A foot holds when, planted, it drifts under the limit, never sinks under the floor past
/// the limit, and its leg always reaches.
pub(crate) fn holds(f: &check::FootReport) -> bool {
    f.slide <= check::SLIDE_LIMIT && f.lowest >= -check::BELOW_LIMIT && f.unreached_frames == 0
}

fn retarget_check(doc: &mut Document, _: &P) -> Out {
    let (plan, report) = drive_document(doc)?;
    Ok(check_json(doc, &plan, &report))
}

fn retarget_save(doc: &mut Document, p: &P) -> Out {
    let l = loaded(doc)?;
    let path = PathBuf::from(cmd::text(p, "path").unwrap_or_default());
    if path.as_os_str().is_empty() {
        return Err("give the mapping file to write (path)".into());
    }
    if !super::is_mapping_file(&path) {
        return Err(format!("the mapping file must end in .json ({})", super::EXT));
    }
    let text = super::save_text(&l)?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    }
    craft_atomic::atomic_write(&path, text.as_bytes())
        .map_err(|e| format!("can't write {}: {e}", path.display()))?;
    Ok(json!({
        "path": path.to_string_lossy(),
        "bytes": text.len(),
        "hash": ac_core::hash::of_bytes(text.as_bytes()),
        "character": l.path.to_string_lossy(),
    }))
}

fn retarget_unload(doc: &mut Document, _: &P) -> Out {
    let was = loaded(doc)?;
    doc.character = None;
    Ok(json!({ "unloaded": true, "character": was.path.to_string_lossy() }))
}

/// The `target` of an export: a character or mapping file path, loaded fresh (the document's
/// loaded character is not used, so a batch is the same from any session).
pub fn target_param(p: &P) -> Result<Option<super::Target>, String> {
    match cmd::text(p, "target").filter(|t| !t.is_empty()) {
        None => Ok(None),
        Some(t) => Ok(Some(super::load(&PathBuf::from(t), &BTreeMap::new(), None)?)),
    }
}
