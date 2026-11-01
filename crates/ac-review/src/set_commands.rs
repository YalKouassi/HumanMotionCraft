//! The `review.*` commands on training sets. One set is active at a time; the window and
//! `ac-review run --set <name>` reach these through [`Review::run`].
//!
//! Each change is one undo step however many clips it touches (`review.undo`; the steps last as
//! long as the process and the active set).

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::decisions;
use crate::review::Review;
use crate::review::{Filter, Show};
use crate::store;
use crate::styles;
use crate::totals::{self, Totals, Usage, Usages};
use crate::training_set::{self, ClipStatus, SetClip, TrainingSet};

type R<T> = Result<T, String>;
type P = Map<String, Value>;

/// Most undo steps kept.
const MAX_UNDO: usize = 200;

type Changes = Vec<(String, Option<SetClip>)>;

/// A style and what its list holds now.
pub struct StyleRow {
    pub style: &'static styles::Style,
    pub clips: usize,
    pub labelled: usize,
    pub minutes: f64,
}

/// The training sets' side of a review.
#[derive(Default)]
pub struct Training {
    /// Where the sets are; none: the set commands say so.
    pub folder: Option<PathBuf>,
    pub active: Option<TrainingSet>,
    undo: Vec<Changes>,
}

fn word(p: &P, k: &str) -> R<String> {
    p.get(k)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("give {k}"))
}

fn words(p: &P, k: &str) -> R<Option<Vec<String>>> {
    match p.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => serde_json::from_value(v.clone())
            .map(Some)
            .map_err(|e| format!("{k} is a list of words: {e}")),
    }
}

/// The set's undo steps, beside it so they outlast the session; the trainer reads only `.set.json`.
fn undo_path(folder: &Path, name: &str) -> PathBuf {
    folder.join(format!("{name}.undo.json"))
}

/// None when there is no file or it can't be read: only the undo steps are lost, never the set.
fn load_undo(folder: &Path, name: &str) -> Vec<Changes> {
    std::fs::read_to_string(undo_path(folder, name))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

impl Review {
    /// Keeping the steps is a convenience, so failing to write them doesn't refuse the change.
    fn save_undo(&self) {
        let (Some(folder), Some(set)) = (&self.training.folder, &self.training.active) else {
            return;
        };
        if let Ok(text) = serde_json::to_string(&self.training.undo) {
            let _ = std::fs::write(undo_path(folder, &set.name), text);
        }
    }

    /// Add a person's library (`<root>/animations/<group>/<name>.clip.json`): each group reads as
    /// a set named `library:<group>`, so its clips' ids are `library:<group>/<name>`.
    pub fn add_library(&mut self, root: &Path, rig: &[String]) {
        let (sets, problems) = store::open_library(root, rig);
        if !sets.is_empty() {
            self.problems.retain(|p| !p.starts_with(store::NO_RIG_TAKES));
        }
        for s in sets {
            self.push_set(s);
        }
        self.problems.extend(problems);
    }

    pub fn folder(&self) -> R<PathBuf> {
        self.training
            .folder
            .clone()
            .ok_or_else(|| "no folder for training sets (--sets <dir>)".to_string())
    }

    pub fn set_names(&mut self) -> Vec<String> {
        let Ok(folder) = self.folder() else {
            return Vec::new();
        };
        let (sets, problems) = training_set::list(&folder);
        for p in problems {
            if !self.problems.contains(&p) {
                self.problems.push(p);
            }
        }
        sets.into_iter().map(|s| s.name).collect()
    }

    fn describe_set(&self) -> Value {
        let Some(s) = &self.training.active else {
            return Value::Null;
        };
        let missing = s.clips.keys().filter(|id| self.find(id).is_err()).count();
        json!({
            "name": s.name, "set_version": s.set_version, "public": s.public,
            "clips": s.clips.len(), "missing": missing,
        })
    }

    /// Make a set active (`None`: back to the decisions file). Its clips that aren't here (a
    /// library not given) stay in the file and are counted as `missing`.
    pub fn open_set(&mut self, name: Option<&str>) -> R<Value> {
        let folder = self.folder()?;
        self.training.active = match name {
            Some(n) => Some(TrainingSet::load(&folder, n)?),
            None => None,
        };
        self.training.undo = name.map(|n| load_undo(&folder, n)).unwrap_or_default();
        Ok(self.describe_set())
    }

    fn fresh(&mut self, set: TrainingSet) -> R<Value> {
        let folder = self.folder()?;
        if TrainingSet::path(&folder, &set.name).exists() {
            return Err(format!("there is already a set {:?}", set.name));
        }
        set.save(&folder)?;
        self.training.undo.clear();
        self.save_undo();
        self.training.active = Some(set);
        Ok(self.describe_set())
    }

    /// Change clips of the active set and save; not saved, not changed. Returns how many changed.
    fn apply(&mut self, changes: Changes, record: bool) -> R<usize> {
        let folder = self.folder()?;
        let set = self.training.active.as_mut().ok_or("no training set is open")?;
        let mut before: Changes = Vec::with_capacity(changes.len());
        for (id, new) in changes {
            let old = set.clips.get(&id).cloned();
            if old == new {
                continue;
            }
            match new {
                Some(c) => set.clips.insert(id.clone(), c),
                None => set.clips.remove(&id),
            };
            before.push((id, old));
        }
        if before.is_empty() {
            return Ok(0);
        }
        set.set_version += 1;
        if let Err(e) = set.save(&folder) {
            set.set_version -= 1;
            for (id, old) in before {
                match old {
                    Some(c) => set.clips.insert(id, c),
                    None => set.clips.remove(&id),
                };
            }
            return Err(e);
        }
        let n = before.len();
        if record {
            self.training.undo.push(before);
            if self.training.undo.len() > MAX_UNDO {
                self.training.undo.remove(0);
            }
            self.save_undo();
        }
        Ok(n)
    }

    /// Frames of a clip that must be here.
    fn frames_of(&self, id: &str) -> R<usize> {
        let (s, t) = self.find(id)?;
        self.sets
            .get(s)
            .and_then(|x| x.takes.get(t))
            .map(|t| t.frames)
            .ok_or_else(|| format!("no clip {id}"))
    }

    /// `review.set_clip`: the fields given, on every id, as one undo step; a clip not in the set
    /// yet is added as `keep`.
    fn set_clips(&mut self, ids: &[String], p: &P) -> R<usize> {
        if ids.len() > training_set::MAX_CLIPS {
            return Err(format!(
                "{} clips: at most {}",
                ids.len(),
                training_set::MAX_CLIPS
            ));
        }
        let set = self.training.active.as_ref().ok_or("no training set is open")?;
        let status = match p.get("status").and_then(Value::as_str) {
            Some(w) => Some(ClipStatus::parse(w)?),
            None => None,
        };
        let cuts: Option<Vec<[usize; 2]>> = match p.get("cuts") {
            None | Some(Value::Null) => None,
            Some(v) => Some(
                serde_json::from_value(v.clone())
                    .map_err(|e| format!("cuts are [[first, last], ...]: {e}"))?,
            ),
        };
        let labels = words(p, "labels")?;
        let add = words(p, "add_labels")?.unwrap_or_default();
        let remove = words(p, "remove_labels")?.unwrap_or_default();
        let mut changes = Vec::with_capacity(ids.len());
        for id in ids {
            let frames = self.frames_of(id)?;
            let mut c = set.clips.get(id).cloned().unwrap_or_else(|| SetClip::new(id));
            if let Some(s) = status {
                c.status = s;
            }
            if let Some(cuts) = &cuts {
                c.cuts = decisions::tidy(cuts, frames)?;
                if c.cuts.iter().map(|[a, b]| b - a + 1).sum::<usize>() >= frames {
                    return Err(format!("{id}: that cuts every frame: reject the clip instead"));
                }
            }
            if let Some(l) = &labels {
                c.labels = l.clone();
            }
            for l in &add {
                if !c.labels.contains(l) {
                    c.labels.push(l.clone());
                }
            }
            c.labels.retain(|l| !remove.contains(l) && !l.trim().is_empty());
            training_set::check_labels(&c.labels).map_err(|e| format!("{id}: {e}"))?;
            changes.push((id.clone(), Some(c)));
        }
        self.apply(changes, true)
    }

    fn ids_of(p: &P) -> R<Vec<String>> {
        if let Some(ids) = words(p, "ids")? {
            return Ok(ids);
        }
        Ok(vec![word(p, "id").map_err(|_| "give id or ids".to_string())?])
    }

    /// `review.set_copy_labels`: the clips in both sets take `from`'s labels.
    fn copy_labels(&mut self, from: &str) -> R<Value> {
        let source = TrainingSet::load(&self.folder()?, from)?;
        let set = self.training.active.as_ref().ok_or("no training set is open")?;
        let mut changes = Vec::new();
        let mut unmatched = 0;
        for (id, c) in &set.clips {
            match source.clips.get(id) {
                Some(s) => {
                    let mut c = c.clone();
                    c.labels = s.labels.clone();
                    changes.push((id.clone(), Some(c)));
                }
                None => unmatched += 1,
            }
        }
        let matched = changes.len();
        let changed = self.apply(changes, true)?;
        Ok(json!({ "matched": matched, "changed": changed, "not_in_source": unmatched }))
    }

    /// Whether a clip belongs in a style's list: labelled with it, or suggested by its name and not
    /// yet given other labels. A clip the open set rejects is out of every list.
    pub fn in_style(&self, s: usize, t: usize, style: &str) -> bool {
        let clip = self
            .training
            .active
            .as_ref()
            .and_then(|a| a.clips.get(&self.id(s, t)));
        if let Some(c) = clip {
            if c.status == ClipStatus::Reject {
                return false;
            }
            if c.labels.iter().any(|l| l == style) {
                return true;
            }
            if !c.labels.is_empty() {
                return false;
            }
        }
        let (Some(st), Some(set)) = (styles::named(style), self.sets.get(s)) else {
            return false;
        };
        let Some(take) = set.takes.get(t) else {
            return false;
        };
        styles::suggests(st, &format!("{} {} {}", set.name, take.name, take.label))
    }

    /// Each style with its description and what its list holds now.
    pub fn style_rows(&self) -> Vec<StyleRow> {
        styles::all()
            .iter()
            .map(|st| {
                let mut row = StyleRow {
                    style: st,
                    clips: 0,
                    labelled: 0,
                    minutes: 0.0,
                };
                for (si, s) in self.sets.iter().enumerate() {
                    for (ti, t) in s.takes.iter().enumerate() {
                        if self.in_style(si, ti, &st.name) {
                            row.clips += 1;
                            row.minutes += t.frames as f64 / t.fps / 60.0;
                            let labelled = self
                                .training
                                .active
                                .as_ref()
                                .and_then(|a| a.clips.get(&self.id(si, ti)))
                                .is_some_and(|c| c.labels.contains(&st.name));
                            row.labelled += usize::from(labelled);
                        }
                    }
                }
                row
            })
            .collect()
    }

    /// `review.usage`: label a pack or clips, or take the label off (`usage` null).
    fn set_usage(&mut self, p: &P) -> R<Value> {
        let usage: Option<Usage> = match p.get("usage") {
            None | Some(Value::Null) => None,
            Some(v) => Some(
                serde_json::from_value(v.clone())
                    .map_err(|_| "usage: share-freely, train-only, just-me or null".to_string())?,
            ),
        };
        let folder = self.folder()?;
        let mut usages = Usages::load(&folder)?;
        let mut changed = 0;
        if let Some(pack) = p.get("pack").and_then(Value::as_str) {
            match usage {
                Some(u) => usages.packs.insert(pack.to_string(), u),
                None => usages.packs.remove(pack),
            };
            changed = 1;
        } else {
            for id in Review::ids_of(p)? {
                match usage {
                    Some(u) => usages.clips.insert(id, u),
                    None => usages.clips.remove(&id),
                };
                changed += 1;
            }
        }
        usages.save(&folder)?;
        Ok(json!({ "changed": changed, "usage": usage }))
    }

    /// The open set's totals and warnings. A usage file that won't read leaves every library clip
    /// Just me, and is said.
    pub fn set_totals(&self) -> R<Totals> {
        let set = self.training.active.as_ref().ok_or("no training set is open")?;
        let (usages, bad) = match Usages::load(&self.folder()?) {
            Ok(s) => (s, None),
            Err(e) => (Usages::default(), Some(e)),
        };
        let mut t = totals::totals(set, &usages, |id| {
            let (s, i) = self.find(id).ok()?;
            let take = self.sets.get(s)?.takes.get(i)?;
            Some((take.frames, take.fps))
        });
        if let Some(e) = bad {
            t.warnings
                .push(format!("every library clip counts as Just me: {e}"));
        }
        Ok(t)
    }

    /// The commands on training sets and their parameters.
    pub fn set_commands() -> Value {
        json!([
            {"id": "review.sets", "params": "", "returns": "{folder, sets: [names], active: {name, set_version, public, clips, missing} | null}"},
            {"id": "review.set_new", "params": "name, public? (default false)", "returns": "the set, now active"},
            {"id": "review.set_duplicate", "params": "as, name? (default the active set)", "returns": "the copy at version 1, now active"},
            {"id": "review.set_open", "params": "name (null: none, K C R go to the decisions file)", "returns": "the set"},
            {"id": "review.set_public", "params": "public", "returns": "the set"},
            {"id": "review.set_copy_labels", "params": "from", "returns": "{matched, changed, not_in_source}: clips in both take from's labels"},
            {"id": "review.set_clip", "params": "id or ids, status? (keep|reject), labels? (replace), add_labels?, remove_labels?, cuts?", "returns": "{changed}: one undo step; a clip not in the set is added as keep"},
            {"id": "review.set_remove", "params": "id or ids", "returns": "{changed}: one undo step"},
            {"id": "review.groups", "params": "show? (default all), set?, reason?, search?", "returns": "{groups: [{group, pack, name, clips, minutes, keep, reject}]}: takes differing by a trailing number are a group"},
            {"id": "review.set_group", "params": "group, status? (keep|reject), labels?, add_labels?, remove_labels?", "returns": "{changed}: every clip of the group, one undo step; a single clip can then be overridden"},
            {"id": "review.totals", "params": "", "returns": "the open set: labels {label: {clips, minutes}}, share_freely, train_only, just_me, unmarked {clips, minutes}, rejected, missing, packs, warnings"},
            {"id": "review.usage", "params": "pack, or id or ids; usage (share-freely|train-only|just-me|null: take it off)", "returns": "{changed, usage}: kept beside the sets, in no set's file"},
            {"id": "review.styles", "params": "", "returns": "{styles: [{name, kind, about, includes, leaves_out, clips, labelled, minutes}]}: what each style should hold and what its list holds now"},
            {"id": "review.style_keep", "params": "style, set? (one data set)", "returns": "{changed}: every clip still in the style's list is kept and labelled with it, one undo step"},
            {"id": "review.undo", "params": "", "returns": "{undone, left}: the active set's last change, in this process"},
        ])
    }

    pub const SET_IDS: [&'static str; 15] = [
        "review.sets",
        "review.set_new",
        "review.set_duplicate",
        "review.set_open",
        "review.set_public",
        "review.set_copy_labels",
        "review.set_clip",
        "review.set_remove",
        "review.groups",
        "review.set_group",
        "review.totals",
        "review.usage",
        "review.styles",
        "review.style_keep",
        "review.undo",
    ];

    pub(crate) fn run_set(&mut self, command: &str, p: &P) -> R<Value> {
        match command {
            "review.sets" => {
                let names = self.set_names();
                let folder = self.training.folder.as_ref().map(|f| f.display().to_string());
                Ok(json!({ "folder": folder, "sets": names, "active": self.describe_set() }))
            }
            "review.set_new" => {
                let public = p.get("public").and_then(Value::as_bool).unwrap_or(false);
                self.fresh(TrainingSet::new(&word(p, "name")?, public)?)
            }
            "review.set_duplicate" => {
                let to = word(p, "as")?;
                training_set::check_name(&to)?;
                let mut copy = match p.get("name").and_then(Value::as_str) {
                    Some(n) => TrainingSet::load(&self.folder()?, n)?,
                    None => self
                        .training
                        .active
                        .clone()
                        .ok_or("no training set is open: give name")?,
                };
                copy.name = to;
                copy.set_version = 1;
                self.fresh(copy)
            }
            "review.set_open" => {
                let name = p.get("name").and_then(Value::as_str).map(str::to_string);
                self.open_set(name.as_deref())
            }
            "review.set_public" => {
                let public = p
                    .get("public")
                    .and_then(Value::as_bool)
                    .ok_or("give public: true or false")?;
                let folder = self.folder()?;
                let set = self.training.active.as_mut().ok_or("no training set is open")?;
                if set.public != public {
                    set.public = public;
                    set.set_version += 1;
                    if let Err(e) = set.save(&folder) {
                        set.public = !public;
                        set.set_version -= 1;
                        return Err(e);
                    }
                }
                Ok(self.describe_set())
            }
            "review.set_copy_labels" => self.copy_labels(&word(p, "from")?),
            "review.set_clip" => {
                let ids = Review::ids_of(p)?;
                Ok(json!({ "changed": self.set_clips(&ids, p)? }))
            }
            "review.set_remove" => {
                let changes = Review::ids_of(p)?.into_iter().map(|id| (id, None)).collect();
                Ok(json!({ "changed": self.apply(changes, true)? }))
            }
            "review.groups" => {
                let mut with_show = p.clone();
                with_show.entry("show").or_insert_with(|| json!("all"));
                let f = Review::filter_of(&with_show)?;
                let rows: Vec<Value> = self
                    .groups(&self.queue(&f))
                    .into_iter()
                    .map(|g| {
                        json!({
                            "group": g.key, "pack": g.pack, "name": g.name, "clips": g.takes.len(),
                            "minutes": g.minutes, "keep": g.keep, "reject": g.reject,
                        })
                    })
                    .collect();
                Ok(json!({ "groups": rows }))
            }
            "review.set_group" => {
                let key = word(p, "group")?;
                let ids: Vec<String> = self
                    .group_takes(&key)
                    .into_iter()
                    .map(|(s, t)| self.id(s, t))
                    .collect();
                if ids.is_empty() {
                    return Err(format!("no group {key:?}"));
                }
                Ok(json!({ "changed": self.set_clips(&ids, p)? }))
            }
            "review.totals" => serde_json::to_value(self.set_totals()?).map_err(|e| e.to_string()),
            "review.usage" => self.set_usage(p),
            "review.styles" => {
                let rows: Vec<Value> = self
                    .style_rows()
                    .into_iter()
                    .map(|r| {
                        json!({
                            "name": r.style.name, "kind": r.style.kind, "about": r.style.about,
                            "includes": r.style.includes, "leaves_out": r.style.leaves_out,
                            "clips": r.clips, "labelled": r.labelled, "minutes": r.minutes,
                        })
                    })
                    .collect();
                Ok(json!({ "styles": rows }))
            }
            "review.style_keep" => {
                let style = word(p, "style")?;
                if styles::named(&style).is_none() {
                    return Err(format!("no style {style:?}"));
                }
                let f = Filter {
                    show: Show::All,
                    style: Some(style.clone()),
                    set: p.get("set").and_then(Value::as_str).map(str::to_string),
                    ..Filter::default()
                };
                let ids: Vec<String> = self.queue(&f).into_iter().map(|(s, t)| self.id(s, t)).collect();
                let mut keep = Map::new();
                keep.insert("status".into(), json!("keep"));
                keep.insert("add_labels".into(), json!([style]));
                Ok(json!({ "changed": self.set_clips(&ids, &keep)? }))
            }
            "review.undo" => {
                let Some(step) = self.training.undo.pop() else {
                    return Ok(json!({ "undone": 0, "left": 0 }));
                };
                match self.apply(step.clone(), false) {
                    Ok(n) => {
                        self.save_undo();
                        Ok(json!({ "undone": n, "left": self.training.undo.len() }))
                    }
                    Err(e) => {
                        self.training.undo.push(step);
                        Err(e)
                    }
                }
            }
            other => Err(format!("no command {other:?}")),
        }
    }
}
