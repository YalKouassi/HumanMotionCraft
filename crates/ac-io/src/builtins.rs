//! Keeping the installed built-ins in step with the ones the app ships: `library.sync`, run at
//! start-up on a library that already exists (an empty one gets `library.install`).
//!
//! The library remembers what it was given in `<library>/.builtins.json`: each built-in's id and
//! a hash of its file as installed. A sync then:
//! - copies in a shipped item the library was never given (new built-ins reach old libraries);
//! - replaces one it was given and still holds unchanged when the shipped file changed (a fix);
//! - moves one no longer shipped to `<library>/.retired/` (out of the library, not destroyed),
//!   unless it moved to the community (`moved.json` beside the shipped items says where): that one
//!   stays where it is as a community install (its `origin` and `.community.json` say which clip);
//! - leaves one the person changed (its file no longer matches) and one they deleted.
//!
//! A library from before the record treats its items with an `origin` as installed built-ins: the
//! person's own work has none (new, save as and duplicate leave or clear it).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::community_record::{Install, ORIGIN, Record as CommunityRecord};
use crate::library::Kind;

type R<T> = Result<T, String>;

/// The record of what the library was given, beside `animations/` and `poses/`.
pub const RECORD: &str = ".builtins.json";
/// Where built-ins the app no longer ships are moved to.
pub const RETIRED: &str = ".retired";
/// Beside the shipped items: the built-ins that moved to the community's Official collection,
/// `{animations: {<id>: {clip, title, author, licence, credit}}}` (`services/community/scripts/official.ts`).
pub const MOVED: &str = "moved.json";
const KINDS: [Kind; 2] = [Kind::Animations, Kind::Poses];

/// FNV-1a, 64 bits, as hex: tells a changed file from the one installed (not a security hash).
pub fn hash(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// `<root>/<kind folder>/<group>/<name><ext>` by id `group/name`; folders starting with a dot
/// (thumbnails, retired items) are not items.
fn files(root: &Path, kind: Kind) -> BTreeMap<String, PathBuf> {
    let mut out = BTreeMap::new();
    for g in std::fs::read_dir(root.join(kind.folder()))
        .into_iter()
        .flatten()
        .flatten()
    {
        let group = g.file_name().to_string_lossy().to_string();
        if group.starts_with('.') || !g.path().is_dir() {
            continue;
        }
        for f in std::fs::read_dir(g.path()).into_iter().flatten().flatten() {
            let name = f.file_name().to_string_lossy().to_string();
            if let Some(stem) = name.strip_suffix(kind.ext()) {
                out.insert(format!("{group}/{stem}"), f.path());
            }
        }
    }
    out
}

type Record = BTreeMap<&'static str, BTreeMap<String, String>>;

/// The record, or `None` when the library has none yet (made before it existed).
fn read_record(lib: &Path) -> R<Option<Record>> {
    let path = lib.join(RECORD);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut rec = Record::new();
    for kind in KINDS {
        let mut ids = BTreeMap::new();
        if let Some(m) = v.get(kind.folder()).and_then(Value::as_object) {
            for (id, h) in m {
                if let Some(h) = h.as_str() {
                    ids.insert(id.clone(), h.to_string());
                }
            }
        }
        rec.insert(kind.folder(), ids);
    }
    Ok(Some(rec))
}

fn write_record(lib: &Path, rec: &Record) -> R<()> {
    let mut v = json!({ "version": 1 });
    for (k, ids) in rec {
        v[*k] = json!(ids);
    }
    let text = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
    let path = lib.join(RECORD);
    craft_atomic::atomic_write(&path, text.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))
}

fn read(path: &Path) -> R<Vec<u8>> {
    std::fs::read(path).map_err(|e| format!("can't read {}: {e}", path.display()))
}

/// A library file, or `None` when it went between listing and reading (another session on the
/// same library synced it first).
fn read_mine(path: &Path) -> R<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("can't read {}: {e}", path.display())),
    }
}

fn copy(bytes: &[u8], to: &Path) -> R<()> {
    if let Some(d) = to.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("can't create {}: {e}", d.display()))?;
    }
    craft_atomic::atomic_write(to, bytes).map_err(|e| format!("can't write {}: {e}", to.display()))
}

/// The shipped built-ins that moved to the community, by library id (none when `from` has no list).
fn read_moved(from: &Path) -> R<BTreeMap<String, Install>> {
    let path = from.join(MOVED);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(format!("can't read {}: {e}", path.display())),
    };
    #[derive(serde::Deserialize, Default)]
    #[serde(default)]
    struct Moved {
        animations: BTreeMap<String, Install>,
    }
    let m: Moved = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(m.animations)
}

/// Keep a built-in that moved to the community as an install of its clip: its `origin` names the
/// clip, and the community record keeps its credit.
fn keep_moved(path: &Path, bytes: &[u8], id: &str, to: &Install, rec: &mut CommunityRecord) -> R<()> {
    let mut v: Value = serde_json::from_slice(bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let Some(o) = v.as_object_mut() else {
        return Err(format!("{} is not a clip", path.display()));
    };
    o.insert("origin".into(), json!(format!("{ORIGIN}{}", to.clip)));
    let text = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
    copy(text.as_bytes(), path)?;
    rec.installs.insert(id.to_string(), to.clone());
    Ok(())
}

/// Move a retired built-in to `<library>/.retired/<kind>/<group>/`; its group folder goes when
/// it was the last item (as `Library::delete` does).
fn retire(lib: &Path, kind: Kind, id: &str, path: &Path) -> R<()> {
    let (group, _) = id
        .split_once('/')
        .ok_or_else(|| format!("{id:?} is not <group>/<name>"))?;
    let name = path
        .file_name()
        .ok_or_else(|| format!("{} has no file name", path.display()))?;
    let dir = lib.join(RETIRED).join(kind.folder()).join(group);
    std::fs::create_dir_all(&dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    let to = dir.join(name);
    match std::fs::rename(path, &to) {
        // another session on the same library retired it first
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        r => r,
    }
    .map_err(|e| format!("can't move {} to {}: {e}", path.display(), to.display()))?;
    if let Some(g) = path.parent()
        && std::fs::read_dir(g).is_ok_and(|mut d| d.next().is_none())
    {
        std::fs::remove_dir(g).map_err(|e| format!("can't remove {}: {e}", g.display()))?;
    }
    Ok(())
}

/// Whether a file with no record was installed as a built-in: it has an `origin`.
fn has_origin(bytes: &[u8]) -> bool {
    serde_json::from_slice::<Value>(bytes)
        .ok()
        .and_then(|d| d.get("origin").and_then(Value::as_str).map(|o| !o.is_empty()))
        .unwrap_or(false)
}

/// Record the items of `lib` that are byte-for-byte the shipped ones in `from` (after
/// `library.install`), keeping what the record already holds.
pub fn record_installed(from: &Path, lib: &Path) -> R<()> {
    let mut rec = read_record(lib)?.unwrap_or_default();
    for kind in KINDS {
        let ids = rec.entry(kind.folder()).or_default();
        let mine = files(lib, kind);
        for (id, src) in files(from, kind) {
            if let Some(dst) = mine.get(&id) {
                let Some(b) = read_mine(dst)? else { continue };
                if b == read(&src)? {
                    ids.insert(id, hash(&b));
                }
            }
        }
    }
    write_record(lib, &rec)
}

/// Bring the built-ins in `lib` in step with the shipped ones in `from` (module docs).
/// `{animations: {added, updated, retired, moved, kept: [{id, why}]}, poses: {...}, changed}`.
pub fn sync(from: &Path, lib: &Path) -> R<Value> {
    if !from.join(Kind::Animations.folder()).is_dir() {
        return Err(format!("{} is not a built-in library folder", from.display()));
    }
    let old = read_record(lib)?;
    let moved_out = read_moved(from)?;
    let mut community: Option<CommunityRecord> = None;
    let mut rec = Record::new();
    let mut out = json!({ "changed": 0 });
    let mut changed = 0;
    for kind in KINDS {
        let given = old.as_ref().and_then(|r| r.get(kind.folder()));
        let shipped = files(from, kind);
        let mine = files(lib, kind);
        let (mut added, mut updated, mut retired, mut kept) = (vec![], vec![], vec![], vec![]);
        let mut moved = vec![];
        let mut ids = BTreeMap::new();
        for (id, src) in &shipped {
            let new = read(src)?;
            let new_hash = hash(&new);
            let Some(dst) = mine.get(id) else {
                // deleted by the person: left deleted (still recorded, so not added again)
                if let Some(h) = given.and_then(|g| g.get(id)) {
                    ids.insert(id.clone(), h.clone());
                } else {
                    copy(
                        &new,
                        &lib.join(kind.folder()).join(
                            src.strip_prefix(from.join(kind.folder()))
                                .map_err(|e| e.to_string())?,
                        ),
                    )?;
                    ids.insert(id.clone(), new_hash);
                    added.push(id.clone());
                }
                continue;
            };
            let Some(cur) = read_mine(dst)? else { continue };
            let cur_hash = hash(&cur);
            if cur == new {
                ids.insert(id.clone(), new_hash);
                continue;
            }
            let installed = match given {
                Some(g) => g.get(id).is_some_and(|h| *h == cur_hash),
                None => has_origin(&cur),
            };
            if installed {
                copy(&new, dst)?;
                ids.insert(id.clone(), new_hash);
                updated.push(id.clone());
            } else {
                let why = if given.is_some_and(|g| g.contains_key(id)) {
                    "changed in your library: kept"
                } else {
                    "yours, under a built-in's name: kept"
                };
                kept.push(json!({ "id": id, "why": why }));
            }
        }
        for (id, dst) in &mine {
            if shipped.contains_key(id) {
                continue;
            }
            let Some(cur) = read_mine(dst)? else { continue };
            let installed = match given {
                Some(g) => g.get(id).is_some_and(|h| *h == hash(&cur)),
                None => has_origin(&cur),
            };
            let to_community = moved_out.get(id).filter(|_| kind == Kind::Animations);
            if let (true, Some(to)) = (installed, to_community) {
                let rec = match &mut community {
                    Some(r) => r,
                    None => community.insert(CommunityRecord::load(lib)?),
                };
                keep_moved(dst, &cur, id, to, rec)?;
                moved.push(id.clone());
            } else if installed {
                retire(lib, kind, id, dst)?;
                retired.push(id.clone());
            } else if given.is_some_and(|g| g.contains_key(id)) {
                kept.push(json!({ "id": id, "why": "no longer shipped, changed in your library: kept" }));
            }
        }
        changed += added.len() + updated.len() + retired.len() + moved.len();
        out[kind.folder()] = json!({
            "added": added, "updated": updated, "retired": retired, "moved": moved, "kept": kept,
        });
        rec.insert(kind.folder(), ids);
    }
    if let Some(c) = community {
        c.save(lib)?;
    }
    write_record(lib, &rec)?;
    out["changed"] = json!(changed);
    Ok(out)
}

/// One line for the status bar: what a sync changed, or `None` when nothing did.
pub fn summary(r: &Value) -> Option<String> {
    let count = |what: &str| -> usize {
        KINDS
            .iter()
            .map(|k| r[k.folder()][what].as_array().map_or(0, Vec::len))
            .sum()
    };
    let (added, updated, retired, kept) = (count("added"), count("updated"), count("retired"), count("kept"));
    let moved = count("moved");
    if added + updated + retired + moved == 0 {
        return None;
    }
    let mut parts = Vec::new();
    for (n, what) in [
        (added, "new"),
        (updated, "fixed"),
        (retired, "retired (moved to .retired)"),
        (moved, "now in the community (kept in your library)"),
    ] {
        if n > 0 {
            parts.push(format!("{n} {what}"));
        }
    }
    let mut s = format!("updated the built-in library: {}", parts.join(", "));
    if kept > 0 {
        s.push_str(&format!("; {kept} you changed kept as they are"));
    }
    Some(s)
}
