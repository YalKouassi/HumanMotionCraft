//! The two libraries, animations and poses, each a folder of JSON files:
//!
//! ```text
//! <root>/animations/<group>/<name>.clip.json
//! <root>/poses/<group>/<name>.pose.json
//! ```
//!
//! The folder is the group: a group exists while a folder does, and moving an item to another
//! group moves its file. An item's id is `<group>/<name>`.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

type R<T> = Result<T, String>;

/// The app's own library: `%APPDATA%/<product name>/library` (the temp folder without `APPDATA`).
/// After a rename, a library still under a former name's folder is used until the new one exists.
pub fn app_library_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = |name: &str| base.join(name).join("library");
    let current = dir(ac_core::product::NAME);
    if current.exists() {
        return current;
    }
    ac_core::product::FORMER
        .iter()
        .map(|f| dir(f.0))
        .find(|d| d.exists())
        .unwrap_or(current)
}

/// The app's own models: `models` beside [`app_library_dir`]. A model here (`<tag>/weights.json`)
/// is used instead of the bundled one: one trained on data that may not be shipped (the training
/// README's `--noncommercial`) lives here, on its owner's machine only.
pub fn app_models_dir() -> PathBuf {
    app_library_dir().with_file_name("models")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Animations,
    Poses,
}

impl Kind {
    pub fn folder(self) -> &'static str {
        match self {
            Kind::Animations => "animations",
            Kind::Poses => "poses",
        }
    }

    pub fn ext(self) -> &'static str {
        match self {
            Kind::Animations => ".clip.json",
            Kind::Poses => ".pose.json",
        }
    }
}

/// Most items listed from one library (a folder with more is listed up to this).
pub const MAX_ITEMS: usize = 100_000;
const SAFE: &str = "-_ ()&,.";

/// A name made safe for a file name: letters, digits and `-_ ()&,.`; never empty.
pub fn safe(name: &str) -> R<String> {
    let keep: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || SAFE.contains(*c))
        .collect();
    let keep = keep.trim_matches(|c| c == ' ' || c == '.').to_string();
    if keep.is_empty() {
        Err(format!("{name:?} has no usable characters for a name"))
    } else {
        Ok(keep)
    }
}

/// Case-blind, numbers by value ("Walk" < "Walk 2" < "Walk 10").
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    fn parts(s: &str) -> Vec<(u8, u128, String)> {
        let mut out = Vec::new();
        let mut cur = String::new();
        let mut digits = false;
        for c in s.to_lowercase().chars() {
            if c.is_ascii_digit() != digits && !cur.is_empty() {
                out.push(part(&cur, digits));
                cur.clear();
            }
            digits = c.is_ascii_digit();
            cur.push(c);
        }
        if !cur.is_empty() {
            out.push(part(&cur, digits));
        }
        out
    }
    fn part(s: &str, digits: bool) -> (u8, u128, String) {
        if digits {
            (0, s.parse().unwrap_or(u128::MAX), String::new())
        } else {
            (1, 0, s.to_string())
        }
    }
    parts(a).cmp(&parts(b))
}

#[derive(Debug, Clone)]
pub struct Library {
    pub root: PathBuf,
    pub kind: Kind,
}

impl Library {
    pub fn new(root: &Path, kind: Kind) -> Library {
        Library {
            root: root.join(kind.folder()),
            kind,
        }
    }

    pub fn make_id(group: &str, name: &str) -> R<String> {
        let group = if group.trim().is_empty() {
            "Ungrouped"
        } else {
            group
        };
        Ok(format!("{}/{}", safe(group)?, safe(name)?))
    }

    /// The file of an id; refuses ids that would leave the library folder.
    pub fn path(&self, id: &str) -> R<PathBuf> {
        let (group, name) = id
            .split_once('/')
            .ok_or_else(|| format!("{id:?} is not <group>/<name>"))?;
        Ok(self
            .root
            .join(safe(group)?)
            .join(format!("{}{}", safe(name)?, self.kind.ext())))
    }

    pub fn exists(&self, id: &str) -> bool {
        self.path(id).is_ok_and(|p| p.is_file())
    }

    /// Every item: `{id, name, group, origin, modified}` (+ `frames`, `fps`, `seconds` and `loop`
    /// for animations, `only` for part poses; `modified`: the file's time, Unix seconds), sorted
    /// by group then name. Unreadable files are listed with an `error`, never skipped.
    pub fn items(&self) -> Vec<Value> {
        let mut out = Vec::new();
        let Ok(groups) = std::fs::read_dir(&self.root) else {
            return out;
        };
        let mut files: Vec<(String, String, PathBuf)> = Vec::new();
        for g in groups.flatten() {
            let gp = g.path();
            if !gp.is_dir() {
                continue;
            }
            let group = g.file_name().to_string_lossy().to_string();
            for f in std::fs::read_dir(&gp).into_iter().flatten().flatten() {
                let fname = f.file_name().to_string_lossy().to_string();
                if let Some(name) = fname.strip_suffix(self.kind.ext()) {
                    files.push((group.clone(), name.to_string(), f.path()));
                }
                if files.len() >= MAX_ITEMS {
                    break;
                }
            }
        }
        files.sort_by(|a, b| natural_cmp(&a.0, &b.0).then_with(|| natural_cmp(&a.1, &b.1)));
        for (group, name, path) in files {
            let mut meta = json!({ "id": format!("{group}/{name}"), "name": name, "group": group });
            if let Some(t) = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            {
                meta["modified"] = json!(t.as_secs());
            }
            match std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|t| serde_json::from_str::<Value>(&t).map_err(|e| e.to_string()))
            {
                Ok(d) => {
                    meta["origin"] = d.get("origin").cloned().unwrap_or(json!(""));
                    if self.kind == Kind::Animations {
                        let last = d
                            .get("keys")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .filter_map(|k| k.get("frame").and_then(Value::as_i64))
                            .max()
                            .unwrap_or(-1);
                        meta["frames"] = json!(last + 1);
                        let fps = d.get("fps").and_then(Value::as_f64).filter(|f| *f >= 1.0);
                        if let Some(fps) = fps {
                            meta["fps"] = json!(fps);
                            meta["seconds"] = json!((last + 1).max(0) as f64 / fps);
                        }
                        meta["loop"] = json!(d.get("loop").and_then(Value::as_bool).unwrap_or(false));
                    } else if let Some(only) = d.get("only") {
                        meta["only"] = only.clone();
                    }
                }
                Err(e) => meta["error"] = json!(format!("can't read: {e}")),
            }
            out.push(meta);
        }
        out
    }

    pub fn groups(&self) -> Vec<String> {
        let mut g: Vec<String> = std::fs::read_dir(&self.root)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        g.sort_by(|a, b| natural_cmp(a, b));
        g
    }

    /// `group/name`, or `group/name 2`, ... when the name is taken anywhere in the library.
    pub fn free_id(&self, group: &str, name: &str) -> R<String> {
        let base = safe(name)?;
        let taken: Vec<String> = self
            .items()
            .iter()
            .filter_map(|i| i.get("name").and_then(Value::as_str).map(str::to_lowercase))
            .collect();
        let mut cand = base.clone();
        let mut n = 2;
        while taken.contains(&cand.to_lowercase()) {
            cand = format!("{base} {n}");
            n += 1;
        }
        Library::make_id(group, &cand)
    }

    pub fn read(&self, id: &str) -> R<String> {
        let p = self.path(id)?;
        std::fs::read_to_string(&p).map_err(|e| format!("can't read {}: {e}", p.display()))
    }

    /// Write an item atomically (a crash never leaves half a file).
    pub fn write(&self, id: &str, text: &str) -> R<String> {
        let p = self.path(id)?;
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
        }
        craft_atomic::atomic_write(&p, text.as_bytes())
            .map_err(|e| format!("can't write {}: {e}", p.display()))?;
        Ok(id.to_string())
    }

    pub fn delete(&self, id: &str) -> R<()> {
        let p = self.path(id)?;
        if p.exists() {
            std::fs::remove_file(&p).map_err(|e| format!("can't delete {}: {e}", p.display()))?;
        }
        if let Some(dir) = p.parent()
            && std::fs::read_dir(dir).is_ok_and(|mut d| d.next().is_none())
        {
            // an empty group goes with its last item
            std::fs::remove_dir(dir).map_err(|e| format!("can't remove {}: {e}", dir.display()))?;
        }
        Ok(())
    }

    /// Rename and/or regroup; returns the new id (unchanged when nothing moves).
    pub fn move_item(&self, id: &str, group: Option<&str>, name: Option<&str>) -> R<String> {
        let (old_group, old_name) = id
            .split_once('/')
            .ok_or_else(|| format!("{id:?} is not <group>/<name>"))?;
        let group = group.unwrap_or(old_group);
        let mut new_name = match name {
            Some(n) => safe(n)?,
            None => old_name.to_string(),
        };
        if new_name.to_lowercase() != old_name.to_lowercase() {
            new_name = self
                .free_id(group, &new_name)?
                .split_once('/')
                .map(|x| x.1.to_string())
                .unwrap_or(new_name);
        }
        let new_id = Library::make_id(group, &new_name)?;
        if new_id == id {
            return Ok(new_id);
        }
        let mut d: Value = serde_json::from_str(&self.read(id)?).map_err(|e| format!("{id}: {e}"))?;
        if d.get("name").is_some() {
            d["name"] = json!(new_name);
        }
        if d.get("group").is_some() {
            d["group"] = json!(new_id.split_once('/').map(|x| x.0).unwrap_or(group));
        }
        self.write(
            &new_id,
            &serde_json::to_string_pretty(&d).map_err(|e| e.to_string())?,
        )?;
        self.delete(id)?;
        // a star follows its item
        crate::find::rename_favourite(self, id, &new_id)?;
        Ok(new_id)
    }

    pub fn duplicate(&self, id: &str) -> R<String> {
        let mut d: Value = serde_json::from_str(&self.read(id)?).map_err(|e| format!("{id}: {e}"))?;
        let (group, name) = id.split_once('/').ok_or("bad id")?;
        let new_id = self.free_id(group, &format!("{name} copy"))?;
        if let Some(o) = d.as_object_mut() {
            o.remove("origin");
            if o.contains_key("name") {
                o.insert(
                    "name".into(),
                    json!(new_id.split_once('/').map(|x| x.1).unwrap_or_default()),
                );
            }
        }
        self.write(
            &new_id,
            &serde_json::to_string_pretty(&d).map_err(|e| e.to_string())?,
        )
    }
}
