//! Hot reload: the joint limits (`assets/rig/limits.json`), the library folder and the sample
//! animations and recipes are watched while the window runs; what changed is reloaded through
//! `assets.reload` and the status bar says what was.
//!
//! Watching is a cheap poll of file sizes and times (no watcher thread). Changes the window makes
//! itself (a save, an import) are taken as seen when it refreshes its library list.

use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use serde_json::{Value, json};

use super::super::App;

/// How often the files are looked at (milliseconds).
pub const POLL_MS: u64 = 1500;
/// Most files looked at in one watched tree (a huge library is watched up to this).
const MAX_FILES: usize = 20_000;

/// A cheap summary of some files: how many, their total size, and the latest change.
type Print = (usize, u64, Option<SystemTime>);

#[derive(Default)]
pub struct Watch {
    assets: PathBuf,
    library: PathBuf,
    /// limits, library, samples
    prints: Option<[Print; 3]>,
    polled: Option<Instant>,
}

fn add(p: &mut Print, path: &Path) {
    if let Ok(m) = std::fs::metadata(path) {
        p.0 += 1;
        p.1 = p.1.wrapping_add(m.len());
        if let Ok(t) = m.modified() {
            p.2 = Some(p.2.map_or(t, |x| x.max(t)));
        }
    }
}

/// Every file under `dir` (folders starting with `.` passed over), `depth` levels down.
fn tree(p: &mut Print, dir: &Path, depth: usize) {
    // a folder's own time changes when a file in it is removed or renamed
    add(p, dir);
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if p.0 >= MAX_FILES {
            return;
        }
        let path = e.path();
        let hidden = e.file_name().to_string_lossy().starts_with('.');
        match e.file_type() {
            Ok(t) if t.is_dir() && !hidden && depth > 0 => tree(p, &path, depth - 1),
            Ok(t) if t.is_file() => add(p, &path),
            _ => {}
        }
    }
}

impl Watch {
    pub fn start(&mut self, assets: &Path, library: &Path) {
        self.assets = assets.to_path_buf();
        self.library = library.to_path_buf();
        self.prints = Some(self.look());
        self.polled = Some(Instant::now());
    }

    fn look(&self) -> [Print; 3] {
        let mut limits = Print::default();
        add(&mut limits, &self.assets.join("rig/limits.json"));
        let mut lib = Print::default();
        for kind in ["animations", "poses"] {
            tree(&mut lib, &self.library.join(kind), 1);
        }
        add(&mut lib, &self.library.join(ac_io::commands::PARTS_FILE));
        let mut samples = Print::default();
        tree(&mut samples, &self.assets.join(ac_io::import::SAMPLES), 1);
        [limits, lib, samples]
    }

    /// The window changed the library itself: what is on disk now is seen.
    pub fn synced(&mut self) {
        let now = self.look();
        if let Some(p) = &mut self.prints {
            p[1] = now[1];
        }
    }

    /// What changed since the last look (`limits`, `library`, `samples`), at most every
    /// `POLL_MS`.
    fn changed(&mut self) -> Vec<&'static str> {
        let now = Instant::now();
        if self
            .polled
            .is_some_and(|t| now.duration_since(t).as_millis() < u128::from(POLL_MS))
        {
            return Vec::new();
        }
        self.polled = Some(now);
        let Some(old) = self.prints else { return Vec::new() };
        let new = self.look();
        self.prints = Some(new);
        ["limits", "library", "samples"]
            .into_iter()
            .zip(old.iter().zip(new.iter()))
            .filter(|(_, (a, b))| a != b)
            .map(|(w, _)| w)
            .collect()
    }
}

/// "joint limits (104 bones)", "library (133 animations, 57 poses)", ...
fn said(r: &Value) -> String {
    let mut out = Vec::new();
    for w in r["reloaded"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        out.push(match w {
            "limits" => format!("joint limits ({} bones)", r["limits"]["bones"]),
            "library" => format!(
                "library ({} animations, {} poses)",
                r["library"]["animations"], r["library"]["poses"]
            ),
            _ => format!(
                "samples ({} samples, {} recipes)",
                r["samples"]["samples"], r["samples"]["recipes"]
            ),
        });
    }
    out.join(", ")
}

impl App {
    /// Reload what changed on disk, and say so.
    pub(super) fn watch_tick(&mut self) {
        let what = self.files.watch.changed();
        if what.is_empty() {
            return;
        }
        match self.session.run("assets.reload", &json!({ "what": what })) {
            Ok(r) => {
                self.status = format!("reloaded {} (changed on disk)", said(&r));
                if what.contains(&"library") {
                    self.refresh_library();
                }
            }
            Err(e) => self.status = format!("changed on disk but couldn't be reloaded: {e}"),
        }
    }
}
