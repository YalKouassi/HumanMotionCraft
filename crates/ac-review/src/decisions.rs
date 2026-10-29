//! What a reviewer decided, take by take: `training/review/decisions.json`, checked in, read by
//! training and the library cutter (`training/ac_train/review.py`).
//!
//! A take with no entry gets the scan's verdict. An entry overrides it:
//! - `ok`: the whole take is good (the scan was wrong);
//! - `cut`: use the take without `cuts` (frame ranges, first and last included);
//! - `reject`: leave the whole take out.
//!
//! Frames count from the take's first frame at the set's rate (30 fps). `frames` is the take's
//! length when it was reviewed: a rebuild that changes it (a different trim) makes the pipeline say
//! so rather than cut the wrong frames. `by` says who decided (`person`, or an agent's name), so an
//! agent's first pass can be checked.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

type R<T> = Result<T, String>;

pub const VERSION: u32 = 1;
/// Notes, types and tags are capped (input is hostile; the file is checked in).
pub const MAX_TEXT: usize = 2000;
pub const MAX_CUTS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Cut,
    Reject,
}

impl Status {
    pub fn parse(s: &str) -> R<Status> {
        match s {
            "ok" => Ok(Status::Ok),
            "cut" => Ok(Status::Cut),
            "reject" => Ok(Status::Reject),
            _ => Err(format!("status {s:?}: ok, cut or reject")),
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Cut => "cut",
            Status::Reject => "reject",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub status: Status,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cuts: Vec<[usize; 2]>,
    /// The motion type, when the take name's is wrong (`walk`, `fight`...). Training's styles
    /// are the labels the data has (`training/ac_train/labels.py`): this replaces the name's type.
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Style words (`tired`, `sneaky`...), added to the take's labels: each one with enough
    /// motion across the takes becomes a style the model learns. Notes go in `note`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    #[serde(default)]
    pub frames: usize,
    #[serde(default)]
    pub by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decisions {
    pub format: String,
    pub version: u32,
    /// By `<set>/<take>`.
    #[serde(default)]
    pub takes: BTreeMap<String, Decision>,
}

impl Default for Decisions {
    fn default() -> Self {
        Decisions {
            format: format!("{}.review", ac_core::product::ID),
            version: VERSION,
            takes: BTreeMap::new(),
        }
    }
}

/// Ranges sorted, inside `0..frames`, overlapping or touching ones joined.
pub fn tidy(cuts: &[[usize; 2]], frames: usize) -> R<Vec<[usize; 2]>> {
    if cuts.len() > MAX_CUTS {
        return Err(format!("{} cuts: at most {MAX_CUTS}", cuts.len()));
    }
    let mut v: Vec<[usize; 2]> = Vec::with_capacity(cuts.len());
    for &[a, b] in cuts {
        if a > b || b >= frames {
            return Err(format!(
                "cut {a}..{b}: a take of {frames} frames has 0..{}",
                frames.saturating_sub(1)
            ));
        }
        v.push([a, b]);
    }
    v.sort();
    let mut out: Vec<[usize; 2]> = Vec::with_capacity(v.len());
    for [a, b] in v {
        match out.last_mut() {
            Some(last) if a <= last[1] + 1 => last[1] = last[1].max(b),
            _ => out.push([a, b]),
        }
    }
    Ok(out)
}

impl Decisions {
    /// The file, or none yet when it doesn't exist. A file that won't read is an error: it is
    /// never overwritten with an empty one.
    pub fn load(path: &Path) -> R<Decisions> {
        if !path.exists() {
            return Ok(Decisions::default());
        }
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let d: Decisions = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        if !ac_core::product::is_format(&d.format, "review") {
            return Err(format!(
                "{}: not a review file (format {:?})",
                path.display(),
                d.format
            ));
        }
        if d.version != VERSION {
            return Err(format!(
                "{}: review file version {} (this reads {VERSION})",
                path.display(),
                d.version
            ));
        }
        Ok(d)
    }

    /// Written whole, atomically, keys sorted (a readable diff).
    pub fn save(&self, path: &Path) -> R<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let mut text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        text.push('\n');
        craft_atomic::atomic_write(path, text.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn cuts_are_tidied_and_checked() {
        assert_eq!(
            tidy(&[[10, 20], [0, 4], [18, 30], [31, 31]], 40).unwrap(),
            vec![[0, 4], [10, 31]]
        );
        assert!(tidy(&[[5, 2]], 40).is_err());
        assert!(tidy(&[[0, 40]], 40).is_err());
    }

    #[test]
    fn the_file_round_trips() {
        let dir = std::env::temp_dir().join(format!("ac-review-test-{}", std::process::id()));
        let path = dir.join("decisions.json");
        let mut d = Decisions::default();
        d.takes.insert(
            "cmu/walk_cmu_01_01".into(),
            Decision {
                status: Status::Cut,
                cuts: vec![[3, 9]],
                kind: Some("walk".into()),
                tags: vec!["tired".into()],
                note: "foot pops".into(),
                frames: 120,
                by: "person".into(),
            },
        );
        d.save(&path).unwrap();
        assert_eq!(Decisions::load(&path).unwrap(), d);
        std::fs::write(&path, "{ not json").unwrap();
        assert!(Decisions::load(&path).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
