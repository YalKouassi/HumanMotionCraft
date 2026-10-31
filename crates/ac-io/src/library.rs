//! The two libraries, animations and poses, each a folder of JSON files:
//!
//! ```text
//! <root>/animations/<group>/<name>.clip.json
//! <root>/poses/<group>/<name>.pose.json
//! ```
//!
//! The folder is the group: a group exists while a folder does, and moving an item to another
//! group moves its file. An item's id is `<group>/<name>`.
//!
//! A library of thousands of clips is listed from `<root>/.index/<kind>.json` ([`Library::listed`]):
//! what each file said, kept against its size and time, so only a new or changed file is read.
//! Deleting that folder only makes the next listing read every file again.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
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
/// The index's folder in a library, and the version of its files (another version is read again).
pub const INDEX_FOLDER: &str = ".index";
const INDEX_VERSION: u32 = 1;
/// Fewest unread files worth starting threads for, and most threads reading them.
const THREADED_FROM: usize = 32;
const MAX_READERS: usize = 8;

/// One file found in a library, and its size and time when they could be read.
struct Found {
    group: String,
    name: String,
    path: PathBuf,
    stamp: Option<Stamp>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
struct Stamp {
    len: u64,
    secs: u64,
    nanos: u32,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct Indexed {
    stamp: Stamp,
    meta: Value,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Default)]
struct Index {
    version: u32,
    items: BTreeMap<String, Indexed>,
}

fn stamp_of(m: &std::fs::Metadata) -> Option<Stamp> {
    let t = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(Stamp {
        len: m.len(),
        secs: t.as_secs(),
        nanos: t.subsec_nanos(),
    })
}

/// Clips directly in `dir` or one visible folder down, when it has no `animations` folder: they
/// would not be listed, as a library's clips are `<dir>/animations/<group>/<name>.clip.json`.
pub fn clips_outside_layout(dir: &Path) -> usize {
    if dir.join(Kind::Animations.folder()).is_dir() {
        return 0;
    }
    let is_clip = |e: &std::fs::DirEntry| e.file_name().to_string_lossy().ends_with(Kind::Animations.ext());
    let mut n = 0;
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if is_clip(&e) {
            n += 1;
        } else if e.path().is_dir() && !e.file_name().to_string_lossy().starts_with('.') {
            n += std::fs::read_dir(e.path())
                .into_iter()
                .flatten()
                .flatten()
                .filter(|f| is_clip(f))
                .count();
        }
    }
    n
}

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
    natural_key(a).cmp(&natural_key(b))
}

/// What [`natural_cmp`] compares, to work out once per item when sorting thousands.
pub fn natural_key(s: &str) -> Vec<(u8, u128, String)> {
    fn part(s: &str, digits: bool) -> (u8, u128, String) {
        if digits {
            (0, s.parse().unwrap_or(u128::MAX), String::new())
        } else {
            (1, 0, s.to_string())
        }
    }
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

    /// Every file of this kind, sorted by group then name.
    fn found(&self) -> Vec<Found> {
        let mut files = Vec::new();
        let Ok(groups) = std::fs::read_dir(&self.root) else {
            return files;
        };
        for g in groups.flatten() {
            let gp = g.path();
            if !gp.is_dir() {
                continue;
            }
            let group = g.file_name().to_string_lossy().to_string();
            for f in std::fs::read_dir(&gp).into_iter().flatten().flatten() {
                let fname = f.file_name().to_string_lossy().to_string();
                if let Some(name) = fname.strip_suffix(self.kind.ext()) {
                    // the folder's own record of the file is far cheaper than opening it; a link
                    // is followed
                    let path = f.path();
                    let stamp = f
                        .metadata()
                        .ok()
                        .filter(|m| !m.file_type().is_symlink())
                        .or_else(|| std::fs::metadata(&path).ok())
                        .and_then(|m| stamp_of(&m));
                    files.push(Found {
                        group: group.clone(),
                        name: name.to_string(),
                        path,
                        stamp,
                    });
                }
                if files.len() >= MAX_ITEMS {
                    break;
                }
            }
        }
        files.sort_by_cached_key(|f| (natural_key(&f.group), natural_key(&f.name)));
        files
    }

    /// Every id of this kind, without opening a file.
    pub fn ids(&self) -> Vec<String> {
        self.found()
            .iter()
            .map(|f| format!("{}/{}", f.group, f.name))
            .collect()
    }

    /// One file's item, read from the file.
    fn describe(&self, f: &Found) -> Value {
        let mut meta = json!({ "id": format!("{}/{}", f.group, f.name), "name": f.name, "group": f.group });
        if let Some(s) = f.stamp {
            meta["modified"] = json!(s.secs);
        }
        match std::fs::read_to_string(&f.path)
            .map_err(|e| e.to_string())
            .and_then(|t| serde_json::from_str::<Value>(&t).map_err(|e| e.to_string()))
        {
            Ok(d) => {
                meta["origin"] = d.get("origin").cloned().unwrap_or(json!(""));
                if d.get("evaluation_only").and_then(Value::as_bool) == Some(true) {
                    meta["evaluation_only"] = json!(true);
                }
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
        meta
    }

    /// Every item: `{id, name, group, origin, modified}` (+ `frames`, `fps`, `seconds` and `loop`
    /// for animations, `only` for part poses; `modified`: the file's time, Unix seconds), sorted
    /// by group then name. Unreadable files are listed with an `error`, never skipped. Reads every
    /// file and writes nothing; [`Library::listed`] is the same list from the index.
    pub fn items(&self) -> Vec<Value> {
        self.found().iter().map(|f| self.describe(f)).collect()
    }

    /// The items of `todo`, in order, on a few threads when there are many.
    fn describe_all(&self, todo: &[&Found]) -> Vec<Value> {
        let threads = std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .min(MAX_READERS);
        if todo.len() < THREADED_FROM || threads < 2 {
            return todo.iter().map(|f| self.describe(f)).collect();
        }
        let chunks: Vec<&[&Found]> = todo.chunks(todo.len().div_ceil(threads)).collect();
        std::thread::scope(|s| {
            let readers: Vec<_> = chunks
                .iter()
                .map(|c| s.spawn(move || c.iter().map(|f| self.describe(f)).collect::<Vec<Value>>()))
                .collect();
            let mut out = Vec::with_capacity(todo.len());
            for (reader, chunk) in readers.into_iter().zip(&chunks) {
                match reader.join() {
                    Ok(items) => out.extend(items),
                    // read again here: this says what is wrong with the file, if anything is
                    Err(_) => out.extend(chunk.iter().map(|f| self.describe(f))),
                }
            }
            out
        })
    }

    fn index_path(&self) -> Option<PathBuf> {
        let file = format!("{}.json", self.kind.folder());
        Some(self.root.parent()?.join(INDEX_FOLDER).join(file))
    }

    /// The index as saved; one that can't be read is only read again from the files.
    fn load_index(&self) -> Index {
        let Some(text) = self.index_path().and_then(|p| std::fs::read_to_string(p).ok()) else {
            return Index::default();
        };
        match serde_json::from_str::<Index>(&text) {
            Ok(i) if i.version == INDEX_VERSION => i,
            Ok(_) => Index::default(),
            Err(e) => {
                tracing::warn!("the library index can't be read, its files are read again: {e}");
                Index::default()
            }
        }
    }

    /// [`Library::items`] for a library in use: a file whose size and time are as the index has
    /// them isn't opened, and the index is brought up to date. An index that can't be written is
    /// only slower next time (logged). The files themselves are never changed.
    pub fn listed(&self) -> Vec<Value> {
        let files = self.found();
        let old = self.load_index();
        let id_of = |f: &Found| format!("{}/{}", f.group, f.name);
        let known = |f: &Found| {
            let i = old.items.get(&id_of(f))?;
            (f.stamp? == i.stamp).then_some(i)
        };
        let todo: Vec<&Found> = files.iter().filter(|f| known(f).is_none()).collect();
        let mut read = self.describe_all(&todo).into_iter();
        let mut new = Index {
            version: INDEX_VERSION,
            items: BTreeMap::new(),
        };
        let mut out = Vec::with_capacity(files.len());
        for f in &files {
            let item = match known(f) {
                Some(i) => {
                    let mut meta = i.meta.clone();
                    // worked out again: a long fraction may not come back from text to the last bit
                    if let (Some(n), Some(fps)) = (
                        meta.get("frames").and_then(Value::as_i64),
                        meta.get("fps").and_then(Value::as_f64),
                    ) {
                        meta["seconds"] = json!(n.max(0) as f64 / fps);
                    }
                    meta
                }
                None => read.next().unwrap_or(Value::Null),
            };
            // an unreadable file is tried again next time
            if let Some(stamp) = f.stamp
                && item.get("error").is_none()
            {
                new.items.insert(
                    id_of(f),
                    Indexed {
                        stamp,
                        meta: item.clone(),
                    },
                );
            }
            out.push(item);
        }
        // an empty list leaves no file: a folder that isn't a library yet gets nothing put in it
        if new.items != old.items && !(new.items.is_empty() && !self.index_path().is_some_and(|p| p.exists()))
        {
            self.save_index(&new);
        }
        out
    }

    fn save_index(&self, index: &Index) {
        let Some(path) = self.index_path() else { return };
        let saved = serde_json::to_string(index)
            .map_err(|e| e.to_string())
            .and_then(|text| {
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                }
                craft_atomic::atomic_write(&path, text.as_bytes()).map_err(|e| e.to_string())
            });
        if let Err(e) = saved {
            tracing::warn!("the library index wasn't saved at {}: {e}", path.display());
        }
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
            .listed()
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
