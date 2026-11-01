//! A set made of other sets (`review.set_combine`, roadmap 196000): the base model's set is the
//! style sets together, with the free clips if asked. What it was made from stays beside it, in
//! `<name>.combine.json`, and it is built afresh when a run of it starts, so a clip rejected in a
//! style's set is out of the next run. A clip rejected in any of them is rejected here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::review::Review;
use crate::training_set::{self, ClipStatus, SetClip, TrainingSet};

type R<T> = Result<T, String>;

/// The free clips of the base: `training/review/natural-takes.json`'s `include`.
pub const FREE_LIST: &str = "review/natural-takes.json";

/// What a combined set is made from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recipe {
    pub from: Vec<String>,
    pub free: bool,
    pub public: bool,
}

/// The recipe's file, beside the set; the trainer reads only `.set.json`.
pub fn recipe_path(folder: &Path, name: &str) -> PathBuf {
    folder.join(format!("{name}.combine.json"))
}

/// A set's recipe, if it is a combined one.
pub fn recipe(folder: &Path, name: &str) -> R<Option<Recipe>> {
    let path = recipe_path(folder, name);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// `cuts` sorted, with those that touch or overlap made one.
fn merged_cuts(mut cuts: Vec<[usize; 2]>) -> Vec<[usize; 2]> {
    cuts.sort_unstable();
    let mut out: Vec<[usize; 2]> = Vec::with_capacity(cuts.len());
    for [a, b] in cuts {
        match out.last_mut() {
            Some(last) if a <= last[1].saturating_add(1) => last[1] = last[1].max(b),
            _ => out.push([a, b]),
        }
    }
    out
}

/// The clips of the sets together.
pub fn combined(sources: &[TrainingSet], free: &[String]) -> R<BTreeMap<String, SetClip>> {
    let mut clips: BTreeMap<String, SetClip> = BTreeMap::new();
    for s in sources {
        for c in s.clips.values() {
            let e = clips.entry(c.id.clone()).or_insert_with(|| SetClip::new(&c.id));
            if c.status == ClipStatus::Reject {
                e.status = ClipStatus::Reject;
            }
            for l in &c.labels {
                if !e.labels.contains(l) {
                    e.labels.push(l.clone());
                }
            }
            e.cuts.extend_from_slice(&c.cuts);
        }
    }
    for c in clips.values_mut() {
        c.cuts = merged_cuts(std::mem::take(&mut c.cuts));
        training_set::check_labels(&c.labels).map_err(|e| format!("{}: {e}", c.id))?;
    }
    for id in free {
        clips.entry(id.clone()).or_insert_with(|| SetClip::new(id));
    }
    Ok(clips)
}

/// The free clips' ids.
fn free_clips(training: &Path) -> R<Vec<String>> {
    let path = training.join(FREE_LIST);
    let at = |e: String| format!("{}: {e}", path.display());
    let text = std::fs::read_to_string(&path).map_err(|e| at(e.to_string()))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| at(e.to_string()))?;
    let list = v
        .get("include")
        .cloned()
        .ok_or_else(|| at("no include list".to_string()))?;
    serde_json::from_value(list).map_err(|e| at(e.to_string()))
}

impl Review {
    /// Build a set from its recipe; a set that hasn't changed isn't written, so its version still
    /// means its clips.
    fn build_combined(&mut self, name: &str, recipe: &Recipe) -> R<Value> {
        let folder = self.folder()?;
        training_set::check_name(name)?;
        if recipe.from.is_empty() {
            return Err("give the sets to combine".to_string());
        }
        let mut sources = Vec::with_capacity(recipe.from.len());
        for s in &recipe.from {
            if s == name {
                return Err(format!("{name} can't be made from itself"));
            }
            if self::recipe(&folder, s)?.is_some() {
                return Err(format!(
                    "{s} is itself a combined set: name the sets it is made from"
                ));
            }
            sources.push(TrainingSet::load(&folder, s)?);
        }
        let free = if recipe.free {
            free_clips(&self.places.training)?
        } else {
            Vec::new()
        };
        let clips = combined(&sources, &free)?;
        let path = TrainingSet::path(&folder, name);
        let old = if path.exists() {
            if self::recipe(&folder, name)?.is_none() {
                return Err(format!(
                    "there is already a set {name:?}, made by hand: it isn't replaced; give another name"
                ));
            }
            Some(TrainingSet::load(&folder, name)?)
        } else {
            None
        };
        let same = old
            .as_ref()
            .is_some_and(|o| o.clips == clips && o.public == recipe.public);
        let set = TrainingSet {
            name: name.to_string(),
            set_version: old.as_ref().map_or(1, |o| o.set_version + u64::from(!same)),
            public: recipe.public,
            clips,
        };
        if !same {
            set.save(&folder)?;
            // what was undoable was a change to the set that has just been replaced
            std::fs::remove_file(folder.join(format!("{name}.undo.json"))).ok();
            self.replaced_set(&set);
        }
        let file = recipe_path(&folder, name);
        let text = serde_json::to_string_pretty(recipe).map_err(|e| e.to_string())?;
        craft_atomic::atomic_write(&file, text.as_bytes()).map_err(|e| format!("{}: {e}", file.display()))?;
        let keep = set
            .clips
            .values()
            .filter(|c| c.status == ClipStatus::Keep)
            .count();
        Ok(json!({
            "name": name, "set_version": set.set_version, "clips": set.clips.len(), "keep": keep,
            "reject": set.clips.len() - keep, "from": recipe.from, "free": recipe.free,
            "free_clips": free.len(), "changed": !same,
        }))
    }

    /// `review.set_combine`.
    pub(crate) fn set_combine(&mut self, p: &Map<String, Value>) -> R<Value> {
        let name = p
            .get("name")
            .and_then(Value::as_str)
            .ok_or("give name")?
            .to_string();
        let from: Vec<String> = match p.get("from") {
            Some(v) => {
                serde_json::from_value(v.clone()).map_err(|e| format!("from is a list of sets: {e}"))?
            }
            None => return Err("give from: the sets to combine".to_string()),
        };
        let folder = self.folder()?;
        let public = match p.get("public").and_then(Value::as_bool) {
            Some(b) => b,
            None => {
                let mut all = true;
                for s in &from {
                    all &= TrainingSet::load(&folder, s)?.public;
                }
                all
            }
        };
        let free = p.get("free").and_then(Value::as_bool).unwrap_or(false);
        self.build_combined(&name, &Recipe { from, free, public })
    }

    /// Build a set afresh if it is a combined one.
    pub(crate) fn recombine(&mut self, name: &str) -> R<()> {
        let folder = self.folder()?;
        if let Some(r) = recipe(&folder, name)? {
            self.build_combined(name, &r)?;
        }
        Ok(())
    }

    /// What the open set is made from, if it is a combined one.
    pub fn open_recipe(&self) -> Option<Recipe> {
        let folder = self.training.folder.as_ref()?;
        let name = &self.training.active.as_ref()?.name;
        recipe(folder, name).ok().flatten()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn cuts_that_touch_are_one() {
        assert_eq!(
            merged_cuts(vec![[8, 9], [1, 3], [2, 5], [6, 6]]),
            [[1, 6], [8, 9]]
        );
        assert_eq!(merged_cuts(Vec::new()), Vec::<[usize; 2]>::new());
    }
}
