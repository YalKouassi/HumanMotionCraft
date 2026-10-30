//! Importing animations and poses into the library: a folder of the person's own, or the sample
//! animations the app ships.
//!
//! A folder is read as `*.clip.json` (animations) and `*.pose.json` (poses), in it and in its
//! subfolders. The folder a file is in is its group, except for files directly in the chosen
//! folder, which take the `group` given (default: the folder's name). So both a loose folder of
//! clips and a whole library (`animations/<group>/...`, `poses/<group>/...`) import as expected.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ac_core::Document;
use ac_core::clipfile;
use ac_core::command::{self as cmd, CommandSpec, Kind as K, ParamSpec};
use serde_json::{Map, Value, json};

use crate::commands::{anims, builtin, poses};
use crate::library::{self, Kind, Library};

type P = Map<String, Value>;
type Out = Result<Value, String>;

/// Where the shipped samples live in the assets, and the group they land in.
pub const SAMPLES: &str = "samples";
pub const SAMPLES_GROUP: &str = "Samples";
/// How deep into subfolders an import looks.
pub const MAX_DEPTH: usize = 8;
/// Largest file an import reads (a long mocap clip is a few MB).
pub const MAX_FILE_BYTES: u64 = 256 << 20;
/// The group of files directly in a folder that has no name (a drive root), and of single files.
pub const UNNAMED_GROUP: &str = "Imported";
/// How many numbered copies (`<name> 2`, ...) an imported file is looked for under.
const MAX_SAME_NAME: usize = 100;

const OVERWRITE: &str = "overwrite";

pub static SPECS: &[CommandSpec] = &[
    CommandSpec::new("library.import", "Import library", library_import)
        .icon("import")
        .inputs(&[
            ParamSpec::new(
                "from",
                K::Text,
                "a folder of *.clip.json and *.pose.json files (subfolders are groups)",
            )
            .required(),
            ParamSpec::new(
                "group",
                K::Text,
                "the group for files directly in the folder (default: the folder's name)",
            ),
            ParamSpec::new(
                OVERWRITE,
                K::Bool,
                "replace items already in the library (your changes to them are lost); built-ins are always kept",
            ),
        ])
        .writes()
        .outputs(crate::returns::LIBRARY_IMPORT),
    CommandSpec::new("library.add_file", "Import file", library_add_file)
        .icon("import")
        .inputs(&[
            ParamSpec::new("path", K::Text, "one animation (*.clip.json) or pose (*.pose.json) file").required(),
            ParamSpec::new("group", K::Text, "the group it goes in (default Imported)"),
        ])
        .writes()
        .outputs(crate::returns::LIBRARY_ADD_FILE),
    CommandSpec::new("library.samples", "Import samples", library_samples)
        .icon("import")
        .inputs(&[ParamSpec::new(
            OVERWRITE,
            K::Bool,
            "replace samples already in the library (your changes to them are lost)",
        )])
        .writes()
        .outputs(crate::returns::LIBRARY_SAMPLES),
];

/// One file found to import.
struct Item {
    kind: Kind,
    id: String,
    text: String,
    /// Where it came from, relative to the chosen folder, with `/` between folders.
    file: String,
}

/// What a folder holds: the items to import and the files that can't be, with why.
#[derive(Default)]
struct Found {
    items: Vec<Item>,
    skipped: Vec<(String, String)>,
    /// A subfolder deeper than `MAX_DEPTH`, or more files than `library::MAX_ITEMS`.
    cut: Option<String>,
}

fn kind_of(file_name: &str) -> Option<(Kind, &str)> {
    [Kind::Animations, Kind::Poses]
        .into_iter()
        .find_map(|k| file_name.strip_suffix(k.ext()).map(|n| (k, n)))
}

fn rel(parts: &[String], name: &str) -> String {
    parts
        .iter()
        .map(String::as_str)
        .chain([name])
        .collect::<Vec<_>>()
        .join("/")
}

/// Read one file as an item for `doc`'s rig.
fn read_item(
    doc: &Document,
    path: &Path,
    kind: Kind,
    group: &str,
    name: &str,
) -> Result<(String, String), String> {
    let size = std::fs::metadata(path)
        .map_err(|e| format!("can't read: {e}"))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(format!("{size} bytes is larger than {MAX_FILE_BYTES}"));
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("can't read: {e}"))?;
    let bones = match kind {
        Kind::Animations => clipfile::load_clip(&text)?.bones,
        Kind::Poses => clipfile::load_pose(&text)?.bones,
    };
    if bones != doc.skeleton.names {
        return Err(format!(
            "made for a different rig ({} bones; this rig has {})",
            bones.len(),
            doc.skeleton.len()
        ));
    }
    Ok((Library::make_id(group, name)?, text))
}

/// Every importable file under `from`, sorted (folders and files in natural order), each read and
/// checked against the rig. Folders starting with `.` (thumbnails, tool data) are passed over.
fn find(doc: &Document, from: &Path, group: &str) -> Found {
    let mut found = Found::default();
    let mut seen: BTreeMap<(&'static str, String), String> = BTreeMap::new();
    // (folder, its path below `from`, the group of its files)
    let mut stack = vec![(from.to_path_buf(), Vec::<String>::new(), group.to_string())];
    while let Some((dir, parts, group)) = stack.pop() {
        let mut files = Vec::new();
        let mut dirs = Vec::new();
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            match e.file_type() {
                // symlinked folders are not followed: they could loop
                Ok(t) if t.is_dir() => {
                    if !name.starts_with('.') {
                        dirs.push(name);
                    }
                }
                Ok(t) if t.is_file() || t.is_symlink() => files.push(name),
                _ => {}
            }
        }
        files.sort_by(|a, b| library::natural_cmp(a, b));
        dirs.sort_by(|a, b| library::natural_cmp(a, b));
        for name in files {
            let Some((kind, stem)) = kind_of(&name) else {
                continue;
            };
            if found.items.len() + found.skipped.len() >= library::MAX_ITEMS {
                found.cut = Some(format!("stopped after {} files", library::MAX_ITEMS));
                return found;
            }
            let file = rel(&parts, &name);
            match read_item(doc, &dir.join(&name), kind, &group, stem) {
                Ok((id, text)) => {
                    let key = (kind.folder(), id.to_lowercase());
                    if let Some(first) = seen.get(&key) {
                        found
                            .skipped
                            .push((file, format!("the same library id {id:?} as {first}")));
                    } else {
                        seen.insert(key, file.clone());
                        found.items.push(Item { kind, id, text, file });
                    }
                }
                Err(e) => found.skipped.push((file, e)),
            }
        }
        if parts.len() >= MAX_DEPTH {
            if !dirs.is_empty() {
                found.cut = Some(format!(
                    "{} is {MAX_DEPTH} folders deep: its subfolders were not read",
                    rel(&parts, "")
                ));
            }
            continue;
        }
        // reversed so the stack pops them in order
        for d in dirs.into_iter().rev() {
            let mut sub = parts.clone();
            sub.push(d.clone());
            stack.push((dir.join(&d), sub, d));
        }
    }
    found
}

/// Ids written and ids already there (kept), per kind.
#[derive(Default)]
struct Copied {
    copied: Vec<String>,
    kept: Vec<String>,
}

/// Write `items` into the library: missing ones always, existing ones only with `overwrite`, and
/// never over a built-in.
fn copy(doc: &Document, items: &[Item], overwrite: bool) -> Result<(Copied, Copied), String> {
    let (alib, plib) = (anims(doc)?, poses(doc)?);
    let (mut a, mut p) = (Copied::default(), Copied::default());
    for it in items {
        let (lib, out) = match it.kind {
            Kind::Animations => (&alib, &mut a),
            Kind::Poses => (&plib, &mut p),
        };
        if lib.exists(&it.id) && (!overwrite || builtin(doc, it.kind, &it.id).is_some()) {
            out.kept.push(it.id.clone());
        } else {
            lib.write(&it.id, &it.text)
                .map_err(|e| format!("{e} (from {})", it.file))?;
            out.copied.push(it.id.clone());
        }
    }
    Ok((a, p))
}

/// Import a folder of the person's own: everything that reads as an animation or pose for this
/// rig is copied; files that don't are skipped and listed with why. Nothing importable is refused.
fn library_import(doc: &mut Document, p: &P) -> Out {
    anims(doc)?;
    let from = PathBuf::from(cmd::text(p, "from").unwrap_or_default());
    if !from.is_dir() {
        return Err(format!("{} is not a folder", from.display()));
    }
    let overwrite = p.get(OVERWRITE).and_then(Value::as_bool).unwrap_or(false);
    let group = match cmd::text(p, "group").map(str::trim).filter(|g| !g.is_empty()) {
        Some(g) => g.to_string(),
        None => from
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| UNNAMED_GROUP.to_string()),
    };
    library::safe(&group)?;
    let found = find(doc, &from, &group);
    if found.items.is_empty() {
        let why = match found.skipped.first() {
            Some((file, e)) => format!(
                "; {} file(s) could not be read, the first: {file}: {e}",
                found.skipped.len()
            ),
            None => String::new(),
        };
        return Err(format!(
            "nothing to import from {}: no animations or poses (*.clip.json, *.pose.json){why}",
            from.display()
        ));
    }
    let (a, ps) = copy(doc, &found.items, overwrite)?;
    let mut out = json!({
        "group": group,
        "animations": { "copied": a.copied, "kept": a.kept },
        "poses": { "copied": ps.copied, "kept": ps.kept },
        "skipped": found
            .skipped
            .iter()
            .map(|(file, error)| json!({ "file": file, "error": error }))
            .collect::<Vec<_>>(),
    });
    if let Some(cut) = found.cut {
        out["cut"] = json!(cut);
    }
    Ok(out)
}

/// Copy the sample animations (`<assets>/samples/*.clip.json`: a few keys each, the rest filled by
/// the AI) into the library's Samples group, as the person's own animations to open, play and
/// change. Samples already there are kept unless `overwrite`. The samples are ours, so one that
/// won't read is an error and nothing is copied.
fn library_samples(doc: &mut Document, p: &P) -> Out {
    anims(doc)?;
    let overwrite = p.get(OVERWRITE).and_then(Value::as_bool).unwrap_or(false);
    let from = doc
        .assets
        .as_ref()
        .ok_or("no assets folder is set")?
        .join(SAMPLES);
    if !from.is_dir() {
        return Err(format!("no samples folder at {}", from.display()));
    }
    let found = find(doc, &from, SAMPLES_GROUP);
    // only the folder's own files are samples (recipes/ holds the scripts that make them)
    let items: Vec<Item> = found
        .items
        .into_iter()
        .filter(|it| it.kind == Kind::Animations && !it.file.contains('/'))
        .collect();
    if let Some((file, e)) = found.skipped.iter().find(|(f, _)| !f.contains('/')) {
        return Err(format!("sample {file}: {e}"));
    }
    if items.is_empty() {
        return Err(format!("no samples (*.clip.json) in {}", from.display()));
    }
    let (a, _) = copy(doc, &items, overwrite)?;
    Ok(json!({ "copied": a.copied, "kept": a.kept, "group": SAMPLES_GROUP }))
}

/// Import one animation or pose file into the library (a drop on the window, File → Import
/// file): it goes in `group` (default Imported) under its own name, or `<name> 2`, ... when that
/// name is taken by something else. The same file imported again is not copied twice.
fn library_add_file(doc: &mut Document, p: &P) -> Out {
    let path = PathBuf::from(cmd::text(p, "path").unwrap_or_default());
    let fname = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let (kind, stem) = kind_of(&fname).ok_or_else(|| {
        format!(
            "{} is not an animation (*.clip.json) or pose (*.pose.json) file",
            path.display()
        )
    })?;
    if !path.is_file() {
        return Err(format!("{} is not a file", path.display()));
    }
    let group = cmd::text(p, "group")
        .map(str::trim)
        .filter(|g| !g.is_empty())
        .unwrap_or(UNNAMED_GROUP);
    let (_, text) =
        read_item(doc, &path, kind, group, stem).map_err(|e| format!("{}: {e}", path.display()))?;
    let lib = match kind {
        Kind::Animations => anims(doc)?,
        Kind::Poses => poses(doc)?,
    };
    let what = match kind {
        Kind::Animations => "animation",
        Kind::Poses => "pose",
    };
    // the file as it is kept under `id`: its own name and group
    let as_item = |id: &str| -> Result<String, String> {
        let mut d: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        if let Some((g, n)) = id.split_once('/')
            && let Some(o) = d.as_object_mut()
        {
            if o.contains_key("name") {
                o.insert("name".into(), json!(n));
            }
            if o.contains_key("group") {
                o.insert("group".into(), json!(g));
            }
        }
        serde_json::to_string_pretty(&d).map_err(|e| e.to_string())
    };
    // imported before: the same item in the group, under its name or a numbered one
    for n in 1..=MAX_SAME_NAME {
        let name = if n == 1 {
            stem.to_string()
        } else {
            format!("{stem} {n}")
        };
        let Ok(cand) = Library::make_id(group, &name) else {
            break;
        };
        if let Ok(t) = lib.read(&cand)
            && (t == text || as_item(&cand).is_ok_and(|x| x == t))
        {
            return Ok(json!({ "kind": what, "id": cand, "already": true }));
        }
    }
    let free = lib.free_id(group, stem)?;
    lib.write(&free, &as_item(&free)?)?;
    Ok(json!({ "kind": what, "id": free, "already": false }))
}

/// Copy a built-in library folder into the library (only missing files, unless `overwrite`).
pub(crate) fn library_install(doc: &mut Document, p: &P) -> Out {
    let to = crate::commands::root(doc)?.to_path_buf();
    let from = PathBuf::from(cmd::text(p, "from").unwrap_or_default());
    let overwrite = p.get("overwrite").and_then(Value::as_bool).unwrap_or(false);
    if !from.is_dir() {
        return Err(format!("{} is not a folder", from.display()));
    }
    let mut copied = 0;
    for kind in [Kind::Animations, Kind::Poses] {
        let src = from.join(kind.folder());
        for g in std::fs::read_dir(&src).into_iter().flatten().flatten() {
            for f in std::fs::read_dir(g.path()).into_iter().flatten().flatten() {
                let dst = to.join(kind.folder()).join(g.file_name()).join(f.file_name());
                if dst.exists() && !overwrite {
                    continue;
                }
                let bytes = std::fs::read(f.path()).map_err(|e| format!("{}: {e}", f.path().display()))?;
                if let Some(d) = dst.parent() {
                    std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
                }
                craft_atomic::atomic_write(&dst, &bytes).map_err(|e| format!("{}: {e}", dst.display()))?;
                copied += 1;
            }
        }
    }
    // what was given is recorded, so a later `library.sync` knows the person's changes from it
    crate::builtins::record_installed(&from, &to)?;
    Ok(json!({ "copied": copied }))
}

/// Bring the installed built-ins in step with the shipped ones (`crate::builtins`).
pub(crate) fn library_sync(doc: &mut Document, p: &P) -> Out {
    let to = crate::commands::root(doc)?.to_path_buf();
    let from = PathBuf::from(cmd::text(p, "from").unwrap_or_default());
    crate::builtins::sync(&from, &to)
}
