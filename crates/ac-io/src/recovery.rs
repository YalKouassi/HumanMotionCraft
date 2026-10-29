//! Crash recovery: the open animation's unsaved work, kept beside the library so a crash (or a
//! quit without saving) never loses it.
//!
//! ```text
//! <library>/.recovery/<group>~<name>.recovery.json   an animation from the library
//! <library>/.recovery/untitled.recovery.json         one never saved
//! ```
//!
//! Each file is `{"recovery": 1, "id": <library id or null>, "clip": <the clip file>}`, written
//! atomically. The front end writes one after edits (debounced) and clears it on save; at start-up
//! `recovery.list` says which hold work the library doesn't (`offer`), and `recovery.restore`
//! opens one as the open animation with its changes unsaved, so Save puts them in the library.
//! Paths never come from parameters: a recovery file is named, and must be one of the folder's.

use std::path::{Path, PathBuf};

use ac_core::Document;
use ac_core::clip::Clip;
use ac_core::clipfile;
use ac_core::command::{self as cmd, CommandSpec, Kind as K, ParamSpec};
use serde_json::{Map, Value, json};

use crate::commands::{anims, load_links, open, root};
use crate::import::MAX_FILE_BYTES;
use crate::returns;

type P = Map<String, Value>;
type Out = Result<Value, String>;

/// The folder under the library.
pub const FOLDER: &str = ".recovery";
const EXT: &str = ".recovery.json";
const UNTITLED: &str = "untitled";
/// The file format's version.
const VERSION: u64 = 1;
/// Most recovery files listed.
pub const MAX_FILES: usize = 1000;

const FILE: ParamSpec = ParamSpec::new(
    "file",
    K::Text,
    "a recovery file's name, as recovery.list gives it",
);

pub static SPECS: &[CommandSpec] = &[
    CommandSpec::new("recovery.write", "Keep unsaved work", recovery_write)
        .icon("save")
        .writes()
        .outputs(returns::RECOVERY_WRITE),
    CommandSpec::new("recovery.list", "Unsaved work", recovery_list)
        .icon("restore")
        .outputs(returns::RECOVERY_LIST),
    CommandSpec::new("recovery.restore", "Restore unsaved work", recovery_restore)
        .icon("restore")
        .undoable()
        .inputs(&[FILE.required()])
        .outputs(returns::RECOVERY_RESTORE),
    CommandSpec::new("recovery.clear", "Discard unsaved work", recovery_clear)
        .icon("delete")
        .inputs(&[
            FILE,
            ParamSpec::new(
                "all",
                K::Bool,
                "every recovery file (default: the open animation's)",
            ),
        ])
        .writes()
        .outputs(returns::RECOVERY_CLEAR),
];

fn folder(doc: &Document) -> Result<PathBuf, String> {
    Ok(root(doc)?.join(FOLDER))
}

/// The recovery file's name for a library id (`None`: never saved).
pub fn file_name(id: Option<&str>) -> String {
    match id.and_then(|i| i.split_once('/')) {
        // ids are made of safe names, which never hold `~`, so names can't collide
        Some((g, n)) => format!("{g}~{n}{EXT}"),
        None => format!("{UNTITLED}{EXT}"),
    }
}

/// A recovery file named by a parameter: a plain file name in the folder, nothing else.
fn named(doc: &Document, name: &str) -> Result<PathBuf, String> {
    if name.contains(['/', '\\', ':']) || name.starts_with('.') || !name.ends_with(EXT) {
        return Err(format!(
            "{name:?} is not a recovery file name (recovery.list gives them)"
        ));
    }
    let p = folder(doc)?.join(name);
    if !p.is_file() {
        return Err(format!("no recovery file {name:?}"));
    }
    Ok(p)
}

/// A recovery file's library id and clip.
fn read(path: &Path) -> Result<(Option<String>, Clip), String> {
    let size = std::fs::metadata(path)
        .map_err(|e| format!("can't read: {e}"))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(format!("{size} bytes is larger than {MAX_FILE_BYTES}"));
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("can't read: {e}"))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("not a recovery file: {e}"))?;
    if v.get("recovery").and_then(Value::as_u64) != Some(VERSION) {
        return Err(format!("not a version {VERSION} recovery file"));
    }
    let clip = v.get("clip").ok_or("a recovery file without a clip")?;
    let clip = clipfile::load_clip(&clip.to_string())?;
    Ok((v.get("id").and_then(Value::as_str).map(str::to_string), clip))
}

fn modified(p: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// Write the open animation to its recovery file.
fn recovery_write(doc: &mut Document, _: &P) -> Out {
    let dir = folder(doc)?;
    let name = file_name(doc.clip_id.as_deref());
    let clip: Value = serde_json::from_str(&clipfile::save_clip(&doc.clip)).map_err(|e| e.to_string())?;
    let text = serde_json::to_string(&json!({ "recovery": VERSION, "id": doc.clip_id, "clip": clip }))
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    let path = dir.join(&name);
    craft_atomic::atomic_write(&path, text.as_bytes())
        .map_err(|e| format!("can't write {}: {e}", path.display()))?;
    Ok(json!({ "file": name, "id": doc.clip_id }))
}

/// Every recovery file, and whether it holds work the library doesn't (`offer`): the library has
/// no such animation, or has another version that is older than the recovery file. A file that
/// can't be read is listed with its `error` (and not offered).
fn recovery_list(doc: &mut Document, _: &P) -> Out {
    let dir = folder(doc)?;
    let lib = anims(doc)?;
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(EXT))
        .take(MAX_FILES)
        .collect();
    names.sort();
    let mut items = Vec::new();
    for name in names {
        let path = dir.join(&name);
        let mut it = json!({ "file": name });
        match read(&path) {
            Err(e) => {
                it["error"] = json!(e);
                it["offer"] = json!(false);
            }
            Ok((id, clip)) => {
                it["id"] = json!(id);
                it["name"] = json!(
                    id.as_deref()
                        .and_then(|i| i.split_once('/'))
                        .map_or(clip.name.as_str(), |x| x.1)
                );
                it["keys"] = json!(clip.keys().len());
                let saved = id
                    .as_deref()
                    .and_then(|i| lib.path(i).ok())
                    .filter(|p| p.is_file());
                let (library, offer) = match &saved {
                    None => {
                        let work = clip.layers.len() > 1 || clip.layers.iter().any(|l| !l.keys.is_empty());
                        ("missing", work)
                    }
                    Some(p) => {
                        let same = std::fs::read_to_string(p)
                            .ok()
                            .and_then(|t| clipfile::load_clip(&t).ok())
                            .is_some_and(|c| c == clip);
                        let newer = match (modified(&path), modified(p)) {
                            (Some(r), Some(l)) => r >= l,
                            _ => true,
                        };
                        match (same, newer) {
                            (true, _) => ("same", false),
                            (false, true) => ("older", true),
                            (false, false) => ("newer", false),
                        }
                    }
                };
                // library: what the library holds beside it (missing, the same, an older or a
                // newer version)
                it["library"] = json!(library);
                it["offer"] = json!(offer);
            }
        }
        items.push(it);
    }
    let offered = items.iter().filter(|i| i["offer"] == json!(true)).count();
    Ok(json!({ "items": items, "offered": offered }))
}

/// Open a recovery file as the open animation, its changes unsaved: an animation still in the
/// library keeps its id (Save writes over it); one no longer there opens as a new animation.
fn recovery_restore(doc: &mut Document, p: &P) -> Out {
    let name = cmd::text(p, "file").unwrap_or_default().to_string();
    let path = named(doc, &name)?;
    let (id, clip) = read(&path).map_err(|e| format!("{name}: {e}"))?;
    if clip.bones != doc.skeleton.names {
        return Err(format!("{name}: made for a different rig"));
    }
    let lib = anims(doc)?;
    let saved = id
        .as_deref()
        .filter(|i| lib.exists(i))
        .and_then(|i| lib.read(i).ok().map(|t| (i.to_string(), t)));
    let kept_id = match saved {
        Some((i, text)) => {
            let library_clip = clipfile::load_clip(&text).map_err(|e| format!("{i}: {e}"))?;
            open(doc, library_clip, Some(i.clone()))?;
            Some(i)
        }
        None => {
            open(doc, clip.clone(), None)?;
            None
        }
    };
    doc.links = load_links(&lib, &clip)?;
    doc.clip = clip;
    doc.go_to(0.0)?;
    Ok(json!({ "id": kept_id, "unsaved": doc.unsaved(), "length": doc.clip.length() }))
}

/// Remove recovery files: one (`file`), all (`all`), or by default the open animation's (and the
/// never-saved one's, which it may have come from).
fn recovery_clear(doc: &mut Document, p: &P) -> Out {
    let dir = folder(doc)?;
    let targets: Vec<PathBuf> = if let Some(name) = cmd::text(p, "file") {
        vec![named(doc, name)?]
    } else if p.get("all").and_then(Value::as_bool).unwrap_or(false) {
        std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|f| f.to_string_lossy().ends_with(EXT))
            .take(MAX_FILES)
            .collect()
    } else {
        let mut v = vec![dir.join(file_name(None))];
        if doc.clip_id.is_some() {
            v.push(dir.join(file_name(doc.clip_id.as_deref())));
        }
        v
    };
    let mut removed = Vec::new();
    for t in targets.into_iter().filter(|t| t.is_file()) {
        std::fs::remove_file(&t).map_err(|e| format!("can't remove {}: {e}", t.display()))?;
        removed.push(
            t.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
        );
    }
    removed.sort();
    Ok(json!({ "removed": removed }))
}
