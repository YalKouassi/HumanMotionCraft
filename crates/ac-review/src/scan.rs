//! The quality scan's findings (`<data>/<set>.quality.json`, written by
//! `python -m ac_train.quality`): per take the frame ranges it would cut and why, and whether it
//! rejects the take.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

type R<T> = Result<T, String>;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ScanTake {
    pub take: String,
    #[serde(default)]
    pub frames: usize,
    /// `[first, last, why]`: `why` is one reason or several joined by `+`.
    #[serde(default)]
    pub cuts: Vec<(usize, usize, String)>,
    /// Why the whole take is rejected; empty when it isn't.
    #[serde(default)]
    pub reject: String,
    #[serde(default)]
    pub cut_frames: usize,
    #[serde(default)]
    pub floor_m: f64,
    #[serde(default)]
    pub glitches: usize,
    #[serde(default)]
    pub skate_share: f64,
    #[serde(default)]
    pub float_s: f64,
}

impl ScanTake {
    /// Every reason in its cuts, once each, sorted.
    pub fn reasons(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .cuts
            .iter()
            .flat_map(|(_, _, w)| w.split('+').map(str::to_string))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// How much of the take the scan would cut (a rejected take: all of it).
    pub fn share(&self) -> f64 {
        if !self.reject.is_empty() {
            return 1.0;
        }
        self.cut_frames as f64 / self.frames.max(1) as f64
    }
}

#[derive(Debug, Default, Deserialize)]
struct File {
    #[serde(default)]
    takes: Vec<ScanTake>,
}

/// A set's scan by take name (empty when the set was never scanned).
pub fn load(dir: &Path, set: &str) -> R<HashMap<String, ScanTake>> {
    let path = dir.join(format!("{set}.quality.json"));
    if !path.is_file() {
        return Ok(HashMap::new());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let f: File = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(f.takes.into_iter().map(|t| (t.take.clone(), t)).collect())
}
