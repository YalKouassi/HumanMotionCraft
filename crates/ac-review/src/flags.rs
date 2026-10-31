//! The model's flags (`<data>/flags.json`, written by `python -m ac_train.flag`, roadmap 137000):
//! the takes the in-betweener fails on, worst first, each with its loss and a line saying why.
//! They are for a person to look at, never a rejection: nothing here reads or writes a decision.
//!
//! The file names each set's takes and lengths by a CRC-32 (`digest`), the same one
//! `ac_train.flag.digest` computes, so flags made on a set since rebuilt are told apart (stale)
//! and left out, and said.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::store::Set;

type R<T> = Result<T, String>;

pub const VERSION: u64 = 1;
pub const FILE: &str = "flags.json";
/// More flags than this is not a flags file (the worst 1 % of a few thousand takes).
pub const MAX_FLAGS: usize = 100_000;

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct Flag {
    pub id: String,
    #[serde(default)]
    pub rank: usize,
    /// The take's mean window loss (training's loss, the card's weights).
    #[serde(default)]
    pub loss: f64,
    /// Its worst window's loss, and that window's key A (a frame of the take).
    #[serde(default)]
    pub worst: f64,
    #[serde(default)]
    pub worst_at: usize,
    #[serde(default)]
    pub windows: usize,
    #[serde(default)]
    pub why: String,
    /// Each loss term, weighted as training weighs it.
    #[serde(default)]
    pub weighted: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SetDigest {
    #[serde(default)]
    pub takes: usize,
    #[serde(default)]
    pub frames: usize,
    #[serde(default)]
    pub crc32: u32,
}

#[derive(Debug, Default, Deserialize)]
struct Model {
    #[serde(default)]
    tag: String,
    #[serde(default)]
    checkpoint: String,
}

#[derive(Debug, Default, Deserialize)]
struct File {
    #[serde(default)]
    format: String,
    #[serde(default)]
    version: u64,
    #[serde(default)]
    model: Model,
    #[serde(default)]
    gap: usize,
    #[serde(default)]
    stride: usize,
    #[serde(default)]
    takes: usize,
    #[serde(default)]
    median_loss: f64,
    #[serde(default)]
    sets: HashMap<String, SetDigest>,
    #[serde(default)]
    flagged: Vec<Flag>,
}

/// What `python -m ac_train.flag` found, or that it has not run.
#[derive(Debug, Default)]
pub struct Flags {
    pub path: PathBuf,
    /// Whether the file is there and read.
    pub present: bool,
    /// One line on the run: the model, the gap, the takes evaluated; or how to make the file.
    pub note: String,
    /// `<tag>/<checkpoint>`.
    pub model: String,
    pub gap: usize,
    pub sets: HashMap<String, SetDigest>,
    /// By `<set>/<take>`.
    pub takes: HashMap<String, Flag>,
}

/// CRC-32 over `"<take>\t<frames>\n"` for each take in order (`ac_train.flag.digest`).
pub fn digest(takes: impl IntoIterator<Item = (String, usize)>) -> u32 {
    let mut h = crc32fast::Hasher::new();
    for (name, frames) in takes {
        h.update(format!("{name}\t{frames}\n").as_bytes());
    }
    h.finalize()
}

impl Flags {
    pub fn none(path: PathBuf) -> Flags {
        Flags {
            note: format!(
                "no model flags ({}): python -m ac_train.flag --tag <tag> --checkpoint best writes the takes the model fails on",
                path.display()
            ),
            path,
            ..Flags::default()
        }
    }

    /// Read `<dir>/flags.json`. No file is no flags (said in `note`); a file that won't read, is
    /// not a flags file or is newer than this reader is an error.
    pub fn load(dir: &Path) -> R<Flags> {
        let path = dir.join(FILE);
        if !path.is_file() {
            return Ok(Flags::none(path));
        }
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let f: File = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        if !f.format.ends_with(".flags") {
            return Err(format!(
                "{}: not a flags file (format {:?})",
                path.display(),
                f.format
            ));
        }
        if f.version > VERSION {
            return Err(format!(
                "{}: version {} is newer than this review reads ({VERSION}): rebuild ac-review",
                path.display(),
                f.version
            ));
        }
        if f.flagged.len() > MAX_FLAGS {
            return Err(format!(
                "{}: {} flags is more than a flags file holds",
                path.display(),
                f.flagged.len()
            ));
        }
        let model = format!("{}/{}", f.model.tag, f.model.checkpoint);
        let note = format!(
            "{} of {} takes flagged by {model} (gap {}, {}, median loss {:.3})",
            f.flagged.len(),
            f.takes,
            f.gap,
            if f.stride <= 1 {
                "every window".to_string()
            } else {
                format!("a window every {} frames", f.stride)
            },
            f.median_loss
        );
        let takes = f
            .flagged
            .into_iter()
            .filter(|x| !x.id.is_empty())
            .map(|x| (x.id.clone(), x))
            .collect();
        Ok(Flags {
            path,
            present: true,
            note,
            model,
            gap: f.gap,
            sets: f.sets,
            takes,
        })
    }

    /// Leave out the flags of every set that was rebuilt since they were made (its takes or
    /// lengths differ from the digest), and say so. Flags of a set not loaded here stay (they
    /// never show).
    pub fn check(&mut self, sets: &[Set]) -> Vec<String> {
        let mut problems = Vec::new();
        for s in sets {
            let Some(d) = self.sets.get(&s.name) else {
                if self.present {
                    problems.push(format!(
                        "{} has no model flags (it was not there when python -m ac_train.flag ran): run it again",
                        s.name
                    ));
                }
                continue;
            };
            let now = digest(s.takes.iter().map(|t| (t.name.clone(), t.frames)));
            if d.crc32 != now || d.takes != s.takes.len() {
                let prefix = format!("{}/", s.name);
                let before = self.takes.len();
                self.takes.retain(|id, _| !id.starts_with(&prefix));
                problems.push(format!(
                    "{}'s model flags are stale ({} left out: the set was rebuilt since python -m ac_train.flag ran, {} takes then, {} now): run it again",
                    s.name,
                    before - self.takes.len(),
                    d.takes,
                    s.takes.len()
                ));
            }
        }
        problems
    }

    pub fn get(&self, id: &str) -> Option<&Flag> {
        self.takes.get(id)
    }
}
