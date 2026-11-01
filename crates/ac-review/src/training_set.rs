//! A training set: a named, versioned list of clips and labels, one file a set,
//! `<folder>/<name>.set.json`. `ac_train.train --set` reads the same file (roadmap 177000).
//!
//! A clip's id is `<set>/<take>`, or `library:<group>/<name>` for a clip of a person's own library.
//! A library clip is private: a set holding one is refused inside a git checkout, read or written,
//! so its names can't reach a repository. `set_version` goes up by one with every saved change, so
//! a name and a version always mean the same clips.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::decisions::{self, Status};

type R<T> = Result<T, String>;

pub const VERSION: u32 = 1;
pub const EXT: &str = ".set.json";
/// What starts a private clip's id.
pub const LIBRARY: &str = "library:";
pub const MAX_CLIPS: usize = 200_000;
pub const MAX_LABELS: usize = 64;
pub const MAX_LABEL: usize = 64;
const MAX_NAME: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClipStatus {
    Keep,
    Reject,
}

impl ClipStatus {
    pub fn parse(s: &str) -> R<ClipStatus> {
        match s {
            "keep" => Ok(ClipStatus::Keep),
            "reject" => Ok(ClipStatus::Reject),
            _ => Err(format!("status {s:?}: keep or reject")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetClip {
    pub id: String,
    pub status: ClipStatus,
    #[serde(default)]
    pub labels: Vec<String>,
    /// Frame ranges left out, first and last included.
    #[serde(default)]
    pub cuts: Vec<[usize; 2]>,
}

impl SetClip {
    pub fn new(id: &str) -> SetClip {
        SetClip {
            id: id.to_string(),
            status: ClipStatus::Keep,
            labels: Vec::new(),
            cuts: Vec::new(),
        }
    }

    /// The same clip in the review's words, for its badge and queue.
    pub fn review_status(&self) -> Status {
        match self.status {
            ClipStatus::Reject => Status::Reject,
            ClipStatus::Keep if self.cuts.is_empty() => Status::Ok,
            ClipStatus::Keep => Status::Cut,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct File {
    format: String,
    version: u32,
    name: String,
    set_version: u64,
    #[serde(default)]
    public: bool,
    #[serde(default)]
    clips: Vec<SetClip>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrainingSet {
    pub name: String,
    pub set_version: u64,
    pub public: bool,
    /// By id.
    pub clips: BTreeMap<String, SetClip>,
}

pub fn is_private(id: &str) -> bool {
    id.starts_with(LIBRARY)
}

/// A set's name becomes its file's: letters, digits, `-`, `_` and inner dots only.
pub fn check_name(name: &str) -> R<()> {
    let ok = !name.is_empty()
        && name.chars().count() <= MAX_NAME
        && !name.starts_with('.')
        && !name.ends_with('.')
        && name.chars().all(|c| c.is_alphanumeric() || "-_.".contains(c));
    if ok {
        Ok(())
    } else {
        Err(format!(
            "set name {name:?}: up to {MAX_NAME} letters, digits, - _ and inner dots"
        ))
    }
}

/// `path` absolute with its `..` worked out; the part that exists has its links followed.
fn resolved(path: &Path) -> R<PathBuf> {
    let abs = std::path::absolute(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut plain = PathBuf::new();
    for c in abs.components() {
        match c {
            Component::ParentDir => {
                plain.pop();
            }
            Component::CurDir => {}
            other => plain.push(other),
        }
    }
    let mut rest = Vec::new();
    let mut at = plain.as_path();
    loop {
        if let Ok(real) = at.canonicalize() {
            let mut out = real;
            out.extend(rest.iter().rev());
            return Ok(out);
        }
        let Some(up) = at.parent() else {
            return Ok(plain);
        };
        if let Some(n) = at.file_name() {
            rest.push(n.to_os_string());
        }
        at = up;
    }
}

/// The git checkout `path` is in (a `.git` folder, or a worktree's `.git` file, in it or above).
pub fn git_checkout(path: &Path) -> R<Option<PathBuf>> {
    let real = resolved(path)?;
    Ok(real
        .ancestors()
        .find(|d| d.join(".git").exists())
        .map(Path::to_path_buf))
}

impl TrainingSet {
    pub fn new(name: &str, public: bool) -> R<TrainingSet> {
        check_name(name)?;
        Ok(TrainingSet {
            name: name.to_string(),
            set_version: 1,
            public,
            clips: BTreeMap::new(),
        })
    }

    pub fn path(folder: &Path, name: &str) -> PathBuf {
        folder.join(format!("{name}{EXT}"))
    }

    pub fn private_clips(&self) -> usize {
        self.clips.keys().filter(|id| is_private(id)).count()
    }

    fn check_place(&self, path: &Path) -> R<()> {
        let n = self.private_clips();
        if n > 0
            && let Some(repo) = git_checkout(path)?
        {
            return Err(format!(
                "set {} holds {n} library clips, which are private: it can't be kept inside the git checkout {}; keep it outside any repository",
                self.name,
                repo.display()
            ));
        }
        Ok(())
    }

    /// Read a set, whose file must say the name it is saved under. A set with library clips inside
    /// a git checkout is refused here too.
    pub fn load(folder: &Path, name: &str) -> R<TrainingSet> {
        check_name(name)?;
        let path = TrainingSet::path(folder, name);
        let at = |e: String| format!("{}: {e}", path.display());
        let text = std::fs::read_to_string(&path).map_err(|e| at(e.to_string()))?;
        let f: File = serde_json::from_str(&text).map_err(|e| at(e.to_string()))?;
        if !ac_core::product::is_format(&f.format, "training-set") {
            return Err(at(format!("not a training set (format {:?})", f.format)));
        }
        if f.version != VERSION {
            return Err(at(format!(
                "training set version {} (this reads {VERSION})",
                f.version
            )));
        }
        if f.name != name {
            return Err(at(format!("the file says its name is {:?}", f.name)));
        }
        if f.clips.len() > MAX_CLIPS {
            return Err(at(format!("{} clips: at most {MAX_CLIPS}", f.clips.len())));
        }
        let mut clips = BTreeMap::new();
        for c in f.clips {
            if c.cuts.len() > decisions::MAX_CUTS || c.cuts.iter().any(|[a, b]| a > b) {
                return Err(at(format!("clip {}: bad cuts", c.id)));
            }
            check_labels(&c.labels).map_err(|e| at(format!("clip {}: {e}", c.id)))?;
            let id = c.id.clone();
            if clips.insert(id.clone(), c).is_some() {
                return Err(at(format!("clip {id} is listed twice")));
            }
        }
        let set = TrainingSet {
            name: f.name,
            set_version: f.set_version,
            public: f.public,
            clips,
        };
        set.check_place(&path)?;
        Ok(set)
    }

    /// Written whole, atomically, clips sorted by id (a readable diff). Refused before anything is
    /// made on disk when the set has library clips and the place is inside a git checkout.
    pub fn save(&self, folder: &Path) -> R<()> {
        check_name(&self.name)?;
        let path = TrainingSet::path(folder, &self.name);
        self.check_place(&path)?;
        std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        let file = File {
            format: format!("{}.training-set", ac_core::product::ID),
            version: VERSION,
            name: self.name.clone(),
            set_version: self.set_version,
            public: self.public,
            clips: self.clips.values().cloned().collect(),
        };
        let mut text = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
        text.push('\n');
        craft_atomic::atomic_write(&path, text.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))
    }
}

pub fn check_labels(labels: &[String]) -> R<()> {
    if labels.len() > MAX_LABELS || labels.iter().any(|l| l.chars().count() > MAX_LABEL) {
        return Err(format!("at most {MAX_LABELS} labels of {MAX_LABEL} characters"));
    }
    Ok(())
}

/// The sets in a folder, by name, read; a file that won't read is said, the rest still list.
pub fn list(folder: &Path) -> (Vec<TrainingSet>, Vec<String>) {
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            e.file_name()
                .to_string_lossy()
                .strip_suffix(EXT)
                .map(str::to_string)
        })
        .collect();
    names.sort();
    let (mut sets, mut problems) = (Vec::new(), Vec::new());
    for n in names {
        match TrainingSet::load(folder, &n) {
            Ok(s) => sets.push(s),
            Err(e) => problems.push(e),
        }
    }
    (sets, problems)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ac-review-set-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_file_has_the_shared_shape_and_round_trips() {
        let dir = scratch("shape");
        let mut s = TrainingSet::new("base-natural", false).unwrap();
        s.set_version = 2;
        let mut c = SetClip::new("cmu/walk_01");
        c.labels = vec!["walk".into()];
        c.cuts = vec![[3, 9]];
        s.clips.insert(c.id.clone(), c);
        s.save(&dir).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("base-natural.set.json")).unwrap())
                .unwrap();
        assert_eq!(v["format"], format!("{}.training-set", ac_core::product::ID));
        assert_eq!(v["version"], 1);
        assert_eq!(v["name"], "base-natural");
        assert_eq!(v["set_version"], 2);
        assert_eq!(v["public"], false);
        assert_eq!(
            v["clips"][0],
            serde_json::json!({"id": "cmu/walk_01", "status": "keep", "labels": ["walk"], "cuts": [[3, 9]]})
        );
        assert_eq!(TrainingSet::load(&dir, "base-natural").unwrap(), s);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_library_clip_is_refused_inside_a_checkout_read_or_written() {
        let repo = scratch("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        // a folder that doesn't exist yet, and a way round through `..`
        let inside = repo.join("not").join("yet");
        let round = repo.join("a").join("..").join("sets");
        let mut s = TrainingSet::new("mine", false).unwrap();
        // the free clips may live in a repository
        s.clips.insert("cmu/a".into(), SetClip::new("cmu/a"));
        s.save(&inside).unwrap();
        s.clips.insert("library:G/n".into(), SetClip::new("library:G/n"));
        for at in [&inside, &round] {
            let e = s.save(at).unwrap_err();
            assert!(e.contains("private"), "{e}");
        }
        assert!(!round.exists(), "a refused save makes nothing");
        // a worktree's .git is a file
        let tree = scratch("tree");
        std::fs::write(tree.join(".git"), "gitdir: elsewhere").unwrap();
        assert!(s.save(&tree).is_err());
        // a file put there by hand is refused when read
        let outside = scratch("outside");
        s.save(&outside).unwrap();
        std::fs::copy(outside.join("mine.set.json"), inside.join("mine.set.json")).unwrap();
        assert!(
            TrainingSet::load(&inside, "mine")
                .unwrap_err()
                .contains("private")
        );
        for d in [repo, tree, outside] {
            std::fs::remove_dir_all(&d).ok();
        }
    }

    #[test]
    fn bad_files_and_names_are_refused() {
        let dir = scratch("bad");
        for name in ["", "../up", "a/b", "a\\b", ".hidden", "with space"] {
            assert!(TrainingSet::new(name, false).is_err(), "{name:?}");
        }
        let write = |body: &str| std::fs::write(dir.join("x.set.json"), body).unwrap();
        let head = format!(
            r#""format":"{}.training-set","version":1,"set_version":1"#,
            ac_core::product::ID
        );
        write(&format!(r#"{{{head},"name":"other"}}"#));
        assert!(TrainingSet::load(&dir, "x").is_err());
        let clip = r#"{"id":"a/b","status":"keep"}"#;
        write(&format!(r#"{{{head},"name":"x","clips":[{clip},{clip}]}}"#));
        assert!(TrainingSet::load(&dir, "x").unwrap_err().contains("twice"));
        write(&format!(
            r#"{{{head},"name":"x","clips":[{{"id":"a/b","status":"maybe"}}]}}"#
        ));
        assert!(TrainingSet::load(&dir, "x").is_err());
        write(&format!(
            r#"{{{head},"name":"x","clips":[{{"id":"a/b","status":"keep","cuts":[[5,2]]}}]}}"#
        ));
        assert!(TrainingSet::load(&dir, "x").is_err());
        let (sets, problems) = list(&dir);
        assert!(sets.is_empty());
        assert_eq!(problems.len(), 1, "a bad file is said, not hidden");
        std::fs::remove_dir_all(&dir).ok();
    }
}
