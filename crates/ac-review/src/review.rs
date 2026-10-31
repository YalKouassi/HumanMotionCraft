//! The review: every set, its scan and the decisions, and the commands that read and change them.
//!
//! The window and `ac-review run <command> <json>` reach the same [`Review::run`], so an agent can
//! review headless (a first pass from the numbers, `by: "<agent>"`) and a person checks it in the
//! window. Commands: `review.list`, `review.take`, `review.decide`, `review.clear`,
//! `review.summary` (`review.commands` lists them with their parameters). The model's flags
//! (`flags.json`, `python -m ac_train.flag`, roadmap 137000) are read beside the scan's: `show:
//! "model"` lists the takes the model fails on, worst first; nothing decides them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::decisions::{self, Decision, Decisions, Status};
use crate::flags::{Flag, Flags};
use crate::scan::{self, ScanTake};
use crate::store::{self, Set};

type R<T> = Result<T, String>;
type P = Map<String, Value>;

/// Most rows `review.list` returns.
pub const MAX_LIST: usize = 10_000;

/// What the queue shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Show {
    /// Flagged by the scan and not decided yet (the work left).
    Open,
    /// Everything the scan flagged.
    Flagged,
    Decided,
    All,
    /// The takes the model fails on (`flags.json`), worst loss first.
    Model,
}

impl Show {
    pub const ALL: [Show; 5] = [Show::Open, Show::Flagged, Show::Decided, Show::All, Show::Model];

    pub fn parse(s: &str) -> R<Show> {
        Show::ALL
            .into_iter()
            .find(|x| x.word() == s)
            .ok_or_else(|| format!("show {s:?}: open, flagged, decided, all or model"))
    }

    pub fn word(self) -> &'static str {
        match self {
            Show::Open => "open",
            Show::Flagged => "flagged",
            Show::Decided => "decided",
            Show::All => "all",
            Show::Model => "model",
        }
    }
}

/// Which takes the queue lists.
#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    pub show: Show,
    pub set: Option<String>,
    pub reason: Option<String>,
    pub search: String,
}

impl Default for Filter {
    fn default() -> Self {
        Filter {
            show: Show::Open,
            set: None,
            reason: None,
            search: String::new(),
        }
    }
}

pub struct Review {
    pub data: PathBuf,
    pub decisions_path: PathBuf,
    pub sets: Vec<Set>,
    scans: Vec<HashMap<String, ScanTake>>,
    pub decisions: Decisions,
    /// The model's flags, or that there are none (`flags.note`).
    pub flags: Flags,
    /// What failed to load (a set, a scan, stale flags): said in the window and by `review.summary`.
    pub problems: Vec<String>,
}

static NO_SCAN: ScanTake = ScanTake {
    take: String::new(),
    frames: 0,
    cuts: Vec::new(),
    reject: String::new(),
    cut_frames: 0,
    floor_m: 0.0,
    glitches: 0,
    skate_share: 0.0,
    float_s: 0.0,
};

fn text(p: &P, k: &str) -> Option<String> {
    p.get(k).and_then(Value::as_str).map(str::to_string)
}

fn capped(s: String, what: &str) -> R<String> {
    if s.chars().count() > decisions::MAX_TEXT {
        return Err(format!("{what} is over {} characters", decisions::MAX_TEXT));
    }
    Ok(s)
}

/// CRC-32 of a text file with its line endings made LF (a Windows checkout and a Linux one
/// agree), as `ac_train/stamp.py` computes it.
pub fn rig_crc32(bytes: &[u8]) -> u32 {
    let mut h = crc32fast::Hasher::new();
    let mut from = 0;
    for (i, w) in bytes.windows(2).enumerate() {
        if w == b"\r\n" {
            h.update(&bytes[from..i]);
            from = i + 1;
        }
    }
    h.update(&bytes[from..]);
    h.finalize()
}

impl Review {
    /// Leave out the sets retargeted onto another rig than `rig_json` (they would play wrong
    /// without a hint); say which, and which have no stamp to tell.
    pub fn check_rig(&mut self, rig_json: &Path) -> R<()> {
        let bytes = std::fs::read(rig_json).map_err(|e| format!("{}: {e}", rig_json.display()))?;
        let want = rig_crc32(&bytes);
        let mut i = 0;
        while i < self.sets.len() {
            let s = &self.sets[i];
            match s.rig_crc32 {
                Some(c) if c != want => {
                    self.problems.push(format!(
                        "{} was built for another rig and is left out: rebuild it (python -m ac_train.datasets {}) or fetch it again",
                        s.name, s.name
                    ));
                    self.sets.remove(i);
                    self.scans.remove(i);
                    continue;
                }
                None => self.problems.push(format!(
                    "{} has no build stamp: rebuild it to be sure it fits the rig (python -m ac_train.datasets {})",
                    s.name, s.name
                )),
                Some(_) => {}
            }
            i += 1;
        }
        Ok(())
    }

    /// Open every set in `data` and the decisions file (which must read, if it exists: it is never
    /// replaced by an empty one).
    pub fn open(data: &Path, decisions_path: &Path) -> R<Review> {
        let decisions = Decisions::load(decisions_path)?;
        let (sets, mut problems) = store::open_all(data);
        let mut scans = Vec::with_capacity(sets.len());
        for s in &sets {
            match scan::load(data, &s.name) {
                Ok(m) => {
                    if m.is_empty() {
                        problems.push(format!(
                            "{} has no scan: run python -m ac_train.quality {}",
                            s.name, s.name
                        ));
                    }
                    scans.push(m)
                }
                Err(e) => {
                    problems.push(e);
                    scans.push(HashMap::new());
                }
            }
        }
        let mut flags = match Flags::load(data) {
            Ok(f) => f,
            Err(e) => {
                problems.push(e);
                Flags::none(data.join(crate::flags::FILE))
            }
        };
        problems.extend(flags.check(&sets));
        Ok(Review {
            data: data.to_path_buf(),
            decisions_path: decisions_path.to_path_buf(),
            sets,
            scans,
            decisions,
            flags,
            problems,
        })
    }

    pub fn id(&self, set: usize, take: usize) -> String {
        let s = self.sets.get(set);
        let t = s.and_then(|s| s.takes.get(take));
        format!("{}/{}", s.map_or("", |s| &s.name), t.map_or("", |t| &t.name))
    }

    /// (set, take) for `<set>/<take>`.
    pub fn find(&self, id: &str) -> R<(usize, usize)> {
        let (set, take) = id
            .split_once('/')
            .ok_or_else(|| format!("{id:?}: a take is <set>/<take>"))?;
        let si = self
            .sets
            .iter()
            .position(|s| s.name == set)
            .ok_or_else(|| format!("no set {set:?}"))?;
        let ti = self
            .sets
            .get(si)
            .and_then(|s| s.find(take))
            .ok_or_else(|| format!("{set} has no take {take:?}"))?;
        Ok((si, ti))
    }

    pub fn scan(&self, set: usize, take: usize) -> &ScanTake {
        let name = self
            .sets
            .get(set)
            .and_then(|s| s.takes.get(take))
            .map(|t| t.name.as_str());
        name.and_then(|n| self.scans.get(set).and_then(|m| m.get(n)))
            .unwrap_or(&NO_SCAN)
    }

    pub fn decision(&self, set: usize, take: usize) -> Option<&Decision> {
        self.decisions.takes.get(&self.id(set, take))
    }

    fn flagged(&self, set: usize, take: usize) -> bool {
        let s = self.scan(set, take);
        !s.cuts.is_empty() || !s.reject.is_empty()
    }

    /// The model's flag on a take, if it is among the takes the model fails on.
    pub fn flag(&self, set: usize, take: usize) -> Option<&Flag> {
        self.flags.get(&self.id(set, take))
    }

    /// Every reason the scans give, once each.
    pub fn reasons(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .scans
            .iter()
            .flat_map(|m| m.values().flat_map(ScanTake::reasons))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// The takes the filter lets through, worst first: undecided before decided, the scan's
    /// rejections, then the most cut, then by name. `Show::Model`: the model's worst loss first.
    pub fn queue(&self, f: &Filter) -> Vec<(usize, usize)> {
        let search = f.search.to_lowercase();
        let mut out = Vec::new();
        for (si, s) in self.sets.iter().enumerate() {
            if f.set.as_ref().is_some_and(|n| *n != s.name) {
                continue;
            }
            for (ti, t) in s.takes.iter().enumerate() {
                let decided = self.decision(si, ti).is_some();
                let keep = match f.show {
                    Show::Open => self.flagged(si, ti) && !decided,
                    Show::Flagged => self.flagged(si, ti),
                    Show::Decided => decided,
                    Show::All => true,
                    Show::Model => self.flag(si, ti).is_some(),
                };
                if !keep {
                    continue;
                }
                if let Some(r) = &f.reason
                    && !self.scan(si, ti).reasons().contains(r)
                {
                    continue;
                }
                if !search.is_empty()
                    && !t.name.to_lowercase().contains(&search)
                    && !t.label.to_lowercase().contains(&search)
                {
                    continue;
                }
                out.push((si, ti));
            }
        }
        if f.show == Show::Model {
            out.sort_by(|a, b| {
                let rank = |&(s, t): &(usize, usize)| self.flag(s, t).map_or(usize::MAX, |x| x.rank);
                rank(a)
                    .cmp(&rank(b))
                    .then(self.id(a.0, a.1).cmp(&self.id(b.0, b.1)))
            });
            return out;
        }
        out.sort_by(|a, b| {
            let key = |&(s, t): &(usize, usize)| {
                let sc = self.scan(s, t);
                (self.decision(s, t).is_some(), sc.reject.is_empty())
            };
            key(a)
                .cmp(&key(b))
                .then(
                    self.scan(b.0, b.1)
                        .share()
                        .total_cmp(&self.scan(a.0, a.1).share()),
                )
                .then(self.id(a.0, a.1).cmp(&self.id(b.0, b.1)))
        });
        out
    }

    /// Record a decision and write the file. `cuts` defaults to the scan's for `cut`.
    pub fn decide(&mut self, set: usize, take: usize, mut d: Decision) -> R<()> {
        let frames = self
            .sets
            .get(set)
            .and_then(|s| s.takes.get(take))
            .map(|t| t.frames)
            .ok_or("no such take")?;
        if d.status == Status::Cut && d.cuts.is_empty() {
            d.cuts = self
                .scan(set, take)
                .cuts
                .iter()
                .map(|&(a, b, _)| [a, b])
                .collect();
            if d.cuts.is_empty() {
                return Err("cut what? select frames first (the scan cut nothing here)".into());
            }
        }
        d.cuts = if d.status == Status::Cut {
            decisions::tidy(&d.cuts, frames)?
        } else {
            Vec::new()
        };
        if d.status == Status::Cut && d.cuts.iter().map(|[a, b]| b - a + 1).sum::<usize>() >= frames {
            return Err("that cuts every frame: reject the take instead".into());
        }
        d.frames = frames;
        d.note = capped(d.note, "the note")?;
        if let Some(k) = &d.kind {
            capped(k.clone(), "the type")?;
        }
        d.tags.retain(|t| !t.trim().is_empty());
        if d.tags.len() > 64 || d.tags.iter().any(|t| t.chars().count() > 64) {
            return Err("at most 64 tags of 64 characters".into());
        }
        let id = self.id(set, take);
        let before = self.decisions.takes.insert(id.clone(), d);
        if let Err(e) = self.decisions.save(&self.decisions_path) {
            // not saved: not decided
            match before {
                Some(b) => self.decisions.takes.insert(id, b),
                None => self.decisions.takes.remove(&id),
            };
            return Err(e);
        }
        Ok(())
    }

    pub fn clear(&mut self, set: usize, take: usize) -> R<bool> {
        let id = self.id(set, take);
        let Some(before) = self.decisions.takes.remove(&id) else {
            return Ok(false);
        };
        if let Err(e) = self.decisions.save(&self.decisions_path) {
            self.decisions.takes.insert(id, before);
            return Err(e);
        }
        Ok(true)
    }

    fn row(&self, s: usize, t: usize) -> Value {
        let take = self.sets.get(s).and_then(|x| x.takes.get(t));
        let sc = self.scan(s, t);
        json!({
            "id": self.id(s, t),
            "label": take.map_or("", |x| &x.label),
            "frames": take.map_or(0, |x| x.frames),
            "scan_cut": sc.cut_frames,
            "scan_reject": sc.reject,
            "reasons": sc.reasons(),
            "decision": self.decision(s, t).map(|d| d.status.word()),
            "model": self.flag(s, t).map(|f| json!({
                "rank": f.rank, "loss": f.loss, "worst": f.worst, "worst_at": f.worst_at, "windows": f.windows,
            })),
        })
    }

    /// The commands and their parameters (`review.commands`).
    pub fn commands() -> Value {
        json!([
            {"id": "review.list", "params": "show? (open|flagged|decided|all|model, default open; model: the takes the model fails on, flags.json), set?, reason?, search?, limit? (default 100)", "returns": "{total, takes: [{id, label, frames, scan_cut, scan_reject, reasons, decision, model: {rank, loss, worst, worst_at, windows} | null}]}, worst first"},
            {"id": "review.take", "params": "id (<set>/<take>)", "returns": "the take, the scan's numbers and cuts, the model's flag (why, the weighted loss terms) or null, the decision"},
            {"id": "review.decide", "params": "id, status (ok|cut|reject), cuts? ([[first, last]], default the scan's for cut), type?, tags?, note?, by? (default person)", "returns": "the decision as saved"},
            {"id": "review.clear", "params": "id", "returns": "{cleared}"},
            {"id": "review.summary", "params": "", "returns": "per set: takes, flagged, decided (ok, cut, reject); flags (the model's: present, note, model, path, takes); problems"},
        ])
    }

    pub fn run(&mut self, command: &str, p: &P) -> R<Value> {
        match command {
            "review.commands" => Ok(Review::commands()),
            "review.list" => {
                let f = Filter {
                    show: Show::parse(&text(p, "show").unwrap_or_else(|| "open".into()))?,
                    set: text(p, "set"),
                    reason: text(p, "reason"),
                    search: text(p, "search").unwrap_or_default(),
                };
                let limit = p
                    .get("limit")
                    .and_then(Value::as_u64)
                    .map_or(100, |n| n as usize)
                    .min(MAX_LIST);
                if f.show == Show::Model && !self.flags.present {
                    return Err(self.flags.note.clone());
                }
                let q = self.queue(&f);
                let rows: Vec<Value> = q.iter().take(limit).map(|&(s, t)| self.row(s, t)).collect();
                Ok(json!({ "total": q.len(), "takes": rows }))
            }
            "review.take" => {
                let (s, t) = self.find(&text(p, "id").ok_or("give the take's id: <set>/<take>")?)?;
                let sc = self.scan(s, t);
                let mut row = self.row(s, t);
                row["scan"] = json!({
                    "cuts": sc.cuts, "floor_m": sc.floor_m, "glitches": sc.glitches,
                    "skate_share": sc.skate_share, "float_s": sc.float_s,
                });
                row["decision"] = serde_json::to_value(self.decision(s, t)).map_err(|e| e.to_string())?;
                row["model"] = match self.flag(s, t) {
                    Some(f) => json!({
                        "rank": f.rank, "loss": f.loss, "worst": f.worst, "worst_at": f.worst_at,
                        "windows": f.windows, "why": f.why, "weighted": f.weighted,
                    }),
                    None => Value::Null,
                };
                if !self.flags.present {
                    row["model_note"] = json!(self.flags.note);
                }
                Ok(row)
            }
            "review.decide" => {
                let (s, t) = self.find(&text(p, "id").ok_or("give the take's id: <set>/<take>")?)?;
                let status = Status::parse(&text(p, "status").ok_or("give a status: ok, cut or reject")?)?;
                let cuts: Vec<[usize; 2]> = match p.get("cuts") {
                    None | Some(Value::Null) => Vec::new(),
                    Some(v) => serde_json::from_value(v.clone())
                        .map_err(|e| format!("cuts are [[first, last], ...]: {e}"))?,
                };
                let tags: Vec<String> = match p.get("tags") {
                    None | Some(Value::Null) => Vec::new(),
                    Some(v) => serde_json::from_value(v.clone())
                        .map_err(|e| format!("tags are a list of words: {e}"))?,
                };
                let d = Decision {
                    status,
                    cuts,
                    kind: text(p, "type").filter(|k| !k.trim().is_empty()),
                    tags,
                    note: text(p, "note").unwrap_or_default(),
                    frames: 0,
                    by: text(p, "by").unwrap_or_else(|| "person".into()),
                };
                self.decide(s, t, d)?;
                serde_json::to_value(self.decision(s, t)).map_err(|e| e.to_string())
            }
            "review.clear" => {
                let (s, t) = self.find(&text(p, "id").ok_or("give the take's id: <set>/<take>")?)?;
                Ok(json!({ "cleared": self.clear(s, t)? }))
            }
            "review.summary" => {
                let sets: Vec<Value> = self
                    .sets
                    .iter()
                    .enumerate()
                    .map(|(si, s)| {
                        let n = |st: Status| {
                            (0..s.takes.len())
                                .filter(|&t| self.decision(si, t).is_some_and(|d| d.status == st))
                                .count()
                        };
                        json!({
                            "set": s.name,
                            "takes": s.takes.len(),
                            "flagged": (0..s.takes.len()).filter(|&t| self.flagged(si, t)).count(),
                            "ok": n(Status::Ok), "cut": n(Status::Cut), "reject": n(Status::Reject),
                        })
                    })
                    .collect();
                let flags = &self.flags;
                Ok(json!({
                    "sets": sets,
                    "flags": {
                        "present": flags.present, "note": flags.note, "model": flags.model,
                        "path": flags.path.display().to_string(), "takes": flags.takes.len(),
                    },
                    "problems": self.problems,
                }))
            }
            other => {
                let ids = [
                    "review.list",
                    "review.take",
                    "review.decide",
                    "review.clear",
                    "review.summary",
                    "review.commands",
                ];
                let near = ac_core::suggest::closest(other, ids.iter().copied());
                Err(if near.is_empty() {
                    format!("no command {other:?} (review.commands lists them)")
                } else {
                    format!("no command {other:?}: {}", ac_core::suggest::did_you_mean(&near))
                })
            }
        }
    }
}
