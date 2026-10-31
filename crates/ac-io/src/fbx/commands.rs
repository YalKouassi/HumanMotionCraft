//! `clip.import_batch`: every `.fbx` in a folder imported onto the rig and written as a
//! `.clip.json` each, into another folder (the training set reader in `training/` reads those,
//! so there is one FBX importer). A file a Unity `.meta` cuts into named clips gives one
//! `.clip.json` each, `<file> - <clip>`. The open animation is left alone.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ac_core::Document;
use ac_core::clipfile;
use ac_core::command::{self as cmd, CommandSpec, Kind as K, ParamSpec};
use serde_json::{Map, Value, json};

use super::import::{Imported, open_file};

type P = Map<String, Value>;
type Out = Result<Value, String>;

/// Most files one batch imports.
pub const MAX_BATCH: usize = 1000;
/// How deep into subfolders the batch looks.
pub const MAX_DEPTH: usize = 8;

pub static SPECS: &[CommandSpec] =
    &[
        CommandSpec::new("clip.import_batch", "Import a folder of FBX", import_batch)
            .icon("import")
            .inputs(&[
                ParamSpec::new("from", K::Text, "a folder of .fbx files (subfolders too)").required(),
                ParamSpec::new(
                    "to",
                    K::Text,
                    "the folder the .clip.json files go in (made if missing)",
                )
                .required(),
                ParamSpec::new(
                    "joints",
                    K::Text,
                    "landmark -> joint name, correcting the guess for every file ({\"l_knee\": \"LeftLeg\"})",
                )
                .json(),
            ])
            .writes()
            .outputs(crate::returns::CLIP_IMPORT_BATCH),
    ];

/// The landmark -> joint corrections a `joints` parameter gives.
pub fn joints_param(p: &P) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    let Some(v) = p.get("joints") else {
        return Ok(out);
    };
    if v.is_null() {
        return Ok(out);
    }
    let o = v
        .as_object()
        .ok_or("joints: expected an object of landmark -> joint name")?;
    for (k, v) in o {
        let name = v
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("joints: {k:?} must name a joint"))?;
        out.insert(k.clone(), name.to_string());
    }
    Ok(out)
}

/// Every `.fbx` under `from`, sorted, with its path below `from`.
fn find(from: &Path) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    let mut stack = vec![(from.to_path_buf(), Vec::<String>::new())];
    while let Some((dir, parts)) = stack.pop() {
        let mut files = Vec::new();
        let mut dirs = Vec::new();
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            match e.file_type() {
                Ok(t) if t.is_dir() && !name.starts_with('.') && parts.len() < MAX_DEPTH => dirs.push(name),
                Ok(t) if t.is_file() && name.to_lowercase().ends_with(".fbx") => files.push(name),
                _ => {}
            }
        }
        files.sort_by(|a, b| crate::library::natural_cmp(a, b));
        dirs.sort_by(|a, b| crate::library::natural_cmp(a, b));
        for f in files {
            let rel = parts
                .iter()
                .map(String::as_str)
                .chain([f.as_str()])
                .collect::<Vec<_>>()
                .join("/");
            out.push((dir.join(&f), rel));
            if out.len() >= MAX_BATCH {
                return out;
            }
        }
        for d in dirs.into_iter().rev() {
            let mut sub = parts.clone();
            sub.push(d.clone());
            stack.push((dir.join(&d), sub));
        }
    }
    out
}

/// A clip's name as part of a file name: what a file name can't hold (a `.meta` is input, and a
/// slash must not walk out of the folder) becomes `_`.
fn file_part(name: &str) -> String {
    let out: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let out = out.trim_matches([' ', '.']);
    if out.is_empty() {
        "clip".into()
    } else {
        out.to_string()
    }
}

/// What a batch has done so far.
struct Written<'a> {
    to: &'a Path,
    files: Vec<Value>,
    failed: Vec<Value>,
    /// Output file -> the file it came from.
    outputs: BTreeMap<String, String>,
}

impl Written<'_> {
    fn fail(&mut self, rel: &str, clip: Option<&str>, error: String) {
        let mut f = json!({ "file": rel, "error": error });
        if let Some(c) = clip {
            f["clip"] = json!(c);
        }
        self.failed.push(f);
    }

    /// Write one imported clip as `<stem>.clip.json`, never over one written before.
    fn write(&mut self, rel: &str, clip: Option<&str>, stem: &str, imported: Result<Imported, String>) {
        let out = self
            .to
            .join(format!("{stem}{}", crate::library::Kind::Animations.ext()));
        if let Some(first) = self.outputs.get(&stem.to_lowercase()) {
            let error = format!(
                "a file of the same name, {first}, was already written to {}: rename one of them",
                out.display()
            );
            self.fail(rel, clip, error);
            return;
        }
        let imported = match imported {
            Ok(i) => i,
            Err(e) => return self.fail(rel, clip, e),
        };
        self.outputs.insert(stem.to_lowercase(), rel.to_string());
        if let Err(e) = craft_atomic::atomic_write(&out, clipfile::save_clip(&imported.clip).as_bytes()) {
            self.fail(rel, clip, format!("can't write {}: {e}", out.display()));
            return;
        }
        let mut f = json!({
            "file": rel,
            "path": out.to_string_lossy(),
            "frames": imported.report.get("frames").cloned().unwrap_or(Value::Null),
            "fps": imported.report.get("fps").cloned().unwrap_or(Value::Null),
            "notes": imported.report.get("notes").cloned().unwrap_or(json!([])),
        });
        if let Some(c) = clip {
            f["clip"] = json!(c);
            f["loop"] = json!(imported.clip.loop_);
        }
        self.files.push(f);
    }
}

/// Import each file; one that can't be is listed in `failed` with why, and the rest are still
/// written, and likewise one named clip of a file. Two of one name would land on one `.clip.json`:
/// the second is listed in `failed` instead of overwriting the first. Refused only when none could be.
fn import_batch(doc: &mut Document, p: &P) -> Out {
    let from = PathBuf::from(cmd::text(p, "from").unwrap_or_default());
    if !from.is_dir() {
        return Err(format!("{} is not a folder", from.display()));
    }
    let to = PathBuf::from(cmd::text(p, "to").unwrap_or_default());
    if to.as_os_str().is_empty() {
        return Err("give the folder the clips go in (to)".into());
    }
    let joints = joints_param(p)?;
    let found = find(&from);
    if found.is_empty() {
        return Err(format!("no .fbx files under {}", from.display()));
    }
    std::fs::create_dir_all(&to).map_err(|e| format!("{}: {e}", to.display()))?;
    let mut w = Written {
        to: &to,
        files: Vec::new(),
        failed: Vec::new(),
        outputs: BTreeMap::new(),
    };
    for (path, rel) in &found {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "imported".into());
        let source = match open_file(path, &joints) {
            Ok(s) => s,
            Err(e) => {
                w.fail(rel, None, e);
                continue;
            }
        };
        if source.clips().is_empty() {
            w.write(rel, None, &stem, source.whole(&doc.skeleton));
        }
        for clip in source.clips() {
            let out = format!("{stem} - {}", file_part(&clip.name));
            w.write(rel, Some(&clip.name), &out, source.clip(&doc.skeleton, clip));
        }
    }
    let Written { files, failed, .. } = w;
    if files.is_empty() {
        let first = failed
            .first()
            .and_then(|f| f["error"].as_str())
            .unwrap_or("no files")
            .to_string();
        return Err(format!("nothing imported: {first}"));
    }
    let mut out = json!({ "folder": to.to_string_lossy(), "files": files, "failed": failed });
    if found.len() >= MAX_BATCH {
        out["cut"] = json!(format!("stopped after {MAX_BATCH} files"));
    }
    Ok(out)
}
