//! The session's library as a [`PoseSource`]: every animation under `<library>/animations`, read
//! and parsed on demand and kept while its file is unchanged (its modified time and size), so a
//! long AI fill that needs anchor poses doesn't parse the whole library every time, and a clip
//! saved during the session is seen on the next ask.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ac_core::clip::Clip;
use ac_core::clipfile;
use ac_core::posesource::{PoseSource, SourceClip, Sourced};

use crate::library::{Kind, Library};

/// A file as it was when parsed: modified time (seconds, nanoseconds) and size.
type Stamp = (u64, u32, u64);

#[derive(Debug)]
pub struct LibraryPoses {
    root: PathBuf,
    cache: Mutex<BTreeMap<PathBuf, (Stamp, Arc<Clip>)>>,
}

impl LibraryPoses {
    pub fn new(library: &Path) -> LibraryPoses {
        LibraryPoses {
            root: library.to_path_buf(),
            cache: Mutex::new(BTreeMap::new()),
        }
    }
}

fn stamp(path: &Path) -> Result<Stamp, String> {
    let m = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let t = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or((0, 0), |d| (d.as_secs(), d.subsec_nanos()));
    Ok((t.0, t.1, m.len()))
}

impl PoseSource for LibraryPoses {
    fn name(&self) -> String {
        format!("the library at {}", self.root.display())
    }

    fn clips(&self) -> Result<Sourced, String> {
        let lib = Library::new(&self.root, Kind::Animations);
        let mut out = Sourced::default();
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| "the library clip cache is poisoned".to_string())?;
        let mut seen = Vec::new();
        for item in lib.items() {
            let Some(id) = item.get("id").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let path = match lib.path(id) {
                Ok(p) => p,
                Err(e) => {
                    out.notes.push(format!("{id}: {e}"));
                    continue;
                }
            };
            let st = match stamp(&path) {
                Ok(s) => s,
                Err(e) => {
                    out.notes.push(format!("{id}: can't read: {e}"));
                    continue;
                }
            };
            seen.push(path.clone());
            let clip = match cache.get(&path) {
                Some((s, c)) if *s == st => c.clone(),
                _ => {
                    let parsed = std::fs::read_to_string(&path)
                        .map_err(|e| e.to_string())
                        .and_then(|t| clipfile::load_clip(&t));
                    match parsed {
                        Ok(c) => {
                            let c = Arc::new(c);
                            cache.insert(path.clone(), (st, c.clone()));
                            c
                        }
                        Err(e) => {
                            cache.remove(&path);
                            out.notes.push(format!("{id}: can't read: {e}"));
                            continue;
                        }
                    }
                }
            };
            // sample clips are for evaluating, not a source for the fills of other animations
            if clip.evaluation_only {
                continue;
            }
            out.clips.push(SourceClip {
                id: id.to_string(),
                clip,
            });
        }
        // a clip deleted from the library leaves the cache with it
        cache.retain(|p, _| seen.contains(p));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn the_built_in_library_is_read_once_and_in_id_order() {
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/library");
        let src = LibraryPoses::new(&assets);
        let first = src.clips().unwrap();
        assert!(first.clips.len() > 10, "{} clips", first.clips.len());
        assert!(first.notes.is_empty(), "{:?}", first.notes);
        let ids: Vec<(&str, &str)> = first
            .clips
            .iter()
            .map(|c| c.id.split_once('/').unwrap())
            .collect();
        let mut sorted = ids.clone();
        sorted.sort_by(|a, b| {
            crate::library::natural_cmp(a.0, b.0).then_with(|| crate::library::natural_cmp(a.1, b.1))
        });
        assert_eq!(ids, sorted);
        let again = src.clips().unwrap();
        assert_eq!(again.clips.len(), first.clips.len());
        for (a, b) in first.clips.iter().zip(&again.clips) {
            assert!(Arc::ptr_eq(&a.clip, &b.clip), "{} was parsed again", a.id);
        }
    }

    #[test]
    fn a_broken_file_is_a_note_not_a_failure() {
        let dir = std::env::temp_dir().join(format!("ac-anchorlib-{}", std::process::id()));
        let anims = dir.join("animations/Mine");
        std::fs::create_dir_all(&anims).unwrap();
        std::fs::write(anims.join("Bad.clip.json"), "{ not json").unwrap();
        let src = LibraryPoses::new(&dir);
        let got = src.clips().unwrap();
        assert!(got.clips.is_empty());
        assert_eq!(got.notes.len(), 1, "{:?}", got.notes);
        assert!(
            got.notes[0].starts_with("Mine/Bad: can't read:"),
            "{}",
            got.notes[0]
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
