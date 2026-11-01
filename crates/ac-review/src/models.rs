//! The models page's commands (roadmap 196000): every model under `training/data/models`, the main
//! one on this machine, two compared, and a run or its check started. Nothing here deletes a model,
//! and nothing touches `assets/models`: what everyone gets is a release, the owner's own step.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ac_ai::MainModel;
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::review::Review;
use crate::train_run::{self, Kind, Start};
use crate::training_set::{self, ClipStatus, TrainingSet};

type R<T> = Result<T, String>;
type P = Map<String, Value>;

/// The longest run the page starts, in minutes: a day.
pub const MAX_MINUTES: f64 = 1440.0;

/// Where a review's models and training are.
#[derive(Debug, Clone, Default)]
pub struct Places {
    /// `training/data/models`.
    pub models: PathBuf,
    /// `training`, where Python runs.
    pub training: PathBuf,
    /// The app's models folder.
    pub installed: PathBuf,
    /// `assets/models/MANIFEST.json`, which says what everyone gets.
    pub manifest: Option<PathBuf>,
    /// The library a set's private clips are in.
    pub library: Option<PathBuf>,
}

impl Places {
    /// As they lie around `training/data/rig`, with the app's own models folder.
    pub fn of_data(data: &Path) -> Places {
        let data = std::path::absolute(data).unwrap_or_else(|_| data.to_path_buf());
        let up = data.parent().unwrap_or(&data);
        Places {
            models: up.join("models"),
            training: up.parent().unwrap_or(up).to_path_buf(),
            installed: ac_io::library::app_models_dir(),
            manifest: None,
            library: None,
        }
    }
}

/// One folder of `training/data/models`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ModelRow {
    pub tag: String,
    /// When it was last written (UTC day).
    pub date: String,
    /// `finished`, `stopped early`, `unfinished` or `training now`.
    pub state: String,
    /// Whether it has the card a main model needs.
    pub usable: bool,
    pub set: String,
    pub minutes: Option<f64>,
    pub steps: Option<u64>,
    pub styles: Option<usize>,
    pub score: Option<f64>,
    pub best_score: Option<f64>,
    pub is_main: bool,
    /// The model everyone gets was made from this one.
    pub shipped: bool,
    /// Its card won't read.
    pub problem: Option<String>,
    #[serde(skip)]
    modified: u64,
}

/// The page's list.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ModelList {
    pub rows: Vec<ModelRow>,
    /// What the app opens with on this machine, in words.
    pub opens_with: String,
    /// Whether the person chose the shipped model.
    pub main_is_shipped: bool,
    /// Why the choice in `main.json` can't be followed.
    pub main_problem: Option<String>,
    /// The model everyone gets, in words.
    pub everyone_gets: String,
}

/// The UTC day of a time.
pub fn civil_date(secs: u64) -> String {
    // Howard Hinnant's days-to-civil
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// A name for a new model: `<set>-<today>`, with the next free number when that is taken.
pub fn suggest_name(models: &Path, set: &str) -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let base = format!("{set}-{}", civil_date(secs));
    let mut name = base.clone();
    let mut n = 1;
    while taken(models, &name) {
        n += 1;
        name = format!("{base}-{n}");
    }
    name
}

/// As `ac_train.train.run_tag` has it: a folder with anything in it is a model.
fn taken(models: &Path, tag: &str) -> bool {
    std::fs::read_dir(models.join(tag)).is_ok_and(|mut d| d.next().is_some())
}

/// Why a new model can't have this name, if it can't.
pub fn name_problem(models: &Path, tag: &str) -> Option<String> {
    if training_set::check_name(tag).is_err() || tag.starts_with('-') {
        return Some(format!(
            "{tag:?} can't name a model: letters, digits, - _ and inner dots, not starting with -"
        ));
    }
    if tag == ac_ai::DEFAULT_TAG {
        return Some(format!(
            "{tag} is the model everyone gets: a model of that name would hide it in the app"
        ));
    }
    if taken(models, tag) {
        return Some(format!(
            "{tag} is already a model, and a run never writes over one: give another name"
        ));
    }
    None
}

/// The training run the model everyone gets was made from, from a manifest's `source`.
pub fn shipped_from(manifest: &Value) -> Option<(String, String)> {
    let models = manifest.get("models")?.as_array()?;
    let m = models
        .iter()
        .find(|m| m.get("tag").and_then(Value::as_str) == Some(ac_ai::DEFAULT_TAG))?;
    let tag = m.get("tag")?.as_str()?.to_string();
    let source = m.get("source")?.as_str()?;
    let from = source.strip_prefix("training/data/models/")?.split('/').next()?;
    Some((tag, from.to_string()))
}

fn read_card(dir: &Path) -> R<Option<Value>> {
    let path = dir.join("model.json");
    match std::fs::read_to_string(&path) {
        Ok(t) => serde_json::from_str(&t)
            .map(Some)
            .map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn modified(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

/// The set a card says it was trained on.
fn card_set(card: &Value) -> String {
    if let Some(name) = card.pointer("/training_set/name").and_then(Value::as_str) {
        return name.to_string();
    }
    card.get("sets")
        .and_then(Value::as_array)
        .map(|sets| {
            sets.iter()
                .filter_map(|s| s.get("name").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

fn row_of(dir: &Path, tag: &str) -> ModelRow {
    let mut row = ModelRow {
        tag: tag.to_string(),
        state: "unfinished".into(),
        modified: modified(dir),
        ..ModelRow::default()
    };
    match read_card(dir) {
        Ok(Some(card)) => {
            row.modified = modified(&dir.join("model.json"));
            row.usable = true;
            row.state = if card.get("stopped").and_then(Value::as_bool) == Some(true) {
                "stopped early".into()
            } else {
                "finished".into()
            };
            row.set = card_set(&card);
            row.minutes = card.get("minutes").and_then(Value::as_f64);
            row.steps = card.get("steps").and_then(Value::as_u64);
            row.styles = card.get("styles").and_then(Value::as_array).map(Vec::len);
            row.score = card.pointer("/evaluation/score").and_then(Value::as_f64);
            row.best_score = card.pointer("/best/score").and_then(Value::as_f64);
        }
        Ok(None) => {}
        Err(e) => row.problem = Some(e),
    }
    row.date = civil_date(row.modified);
    row
}

/// Every model, newest first, and which of them the app and everyone else get.
pub fn list(p: &Places) -> ModelList {
    let mut rows: Vec<ModelRow> = std::fs::read_dir(&p.models)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| row_of(&e.path(), &e.file_name().to_string_lossy()))
        .collect();
    let shipped = p
        .manifest
        .as_ref()
        .and_then(|m| std::fs::read_to_string(m).ok())
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|m| shipped_from(&m));
    let now = train_run::running(p).ok().flatten();
    let main = ac_ai::main_model(&p.installed);
    for r in &mut rows {
        r.is_main = main == Ok(MainModel::Tag(r.tag.clone()));
        r.shipped = shipped.as_ref().is_some_and(|(_, from)| *from == r.tag);
        if now
            .as_ref()
            .is_some_and(|j| j.kind == Kind::Train && j.tag == r.tag)
        {
            r.state = "training now".into();
            r.usable = false;
        }
    }
    rows.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.tag.cmp(&b.tag)));
    let everyone_gets = match &shipped {
        Some((tag, from)) => format!("{tag}, made from {from}"),
        None => "the model in assets/models (its manifest doesn't say which run it came from)".into(),
    };
    ModelList {
        rows,
        opens_with: opens_with(&p.installed, &main),
        main_is_shipped: main == Ok(MainModel::Bundled),
        main_problem: main.err(),
        everyone_gets,
    }
}

/// What the app opens with, as it decides (`crates/ac-app/src/app.rs`).
fn opens_with(installed: &Path, main: &Result<MainModel, String>) -> String {
    match main {
        Ok(MainModel::Tag(t)) => t.clone(),
        Ok(MainModel::Bundled) => "the shipped model".into(),
        Ok(MainModel::Unset) | Err(_) => {
            if ac_ai::has_model(installed) {
                ac_ai::default_dir(installed)
                    .ok()
                    .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .unwrap_or_else(|| "the shipped model".into())
            } else {
                "the shipped model".into()
            }
        }
    }
}

/// Write `main.json`; `None`: the shipped model.
fn write_main(installed: &Path, tag: Option<&str>) -> R<()> {
    std::fs::create_dir_all(installed).map_err(|e| format!("{}: {e}", installed.display()))?;
    let path = installed.join("main.json");
    let mut text = json!({ "tag": tag }).to_string();
    text.push('\n');
    craft_atomic::atomic_write(&path, text.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))
}

/// `review.model_main`: this model's export becomes what the app opens with; another's stays. A
/// model already exported takes effect at once (a run never writes over a model, so its export is
/// still right), else exporting is a job.
fn set_main(p: &Places, tag: Option<&str>) -> R<Value> {
    let Some(tag) = tag else {
        write_main(&p.installed, None)?;
        return Ok(json!({ "main": null, "exporting": false }));
    };
    if tag == ac_ai::DEFAULT_TAG {
        return Err(format!(
            "{tag} is the model everyone gets: to open with it, use the shipped model (tag null)"
        ));
    }
    if training_set::check_name(tag).is_err() || tag.starts_with('-') {
        return Err(format!("{tag:?} is no model's name"));
    }
    let dir = p.models.join(tag);
    if !dir.is_dir() {
        return Err(format!("no model {tag} in {}", p.models.display()));
    }
    if read_card(&dir)?.is_none() {
        return Err(format!(
            "{tag} never finished, so it has no card: only a finished or stopped run can be the main model"
        ));
    }
    if train_run::running(p)?.is_some_and(|j| j.kind == Kind::Train && j.tag == tag) {
        return Err(format!("{tag} is training now: stop it or let it finish first"));
    }
    if p.installed.join(tag).join("weights.json").is_file() {
        write_main(&p.installed, Some(tag))?;
        return Ok(json!({ "main": tag, "exporting": false }));
    }
    let job = train_run::start(
        p,
        Start {
            kind: Kind::Main,
            tag: tag.to_string(),
            set: None,
            minutes: None,
            steps: None,
            args: train_run::main_args(p, tag),
            log: p.models.join(format!("main-{tag}.log")),
            keep_log: false,
        },
    )?;
    Ok(json!({ "main": tag, "exporting": true, "pid": job.pid }))
}

fn compared_side(card: &Value, tag: &str) -> Value {
    json!({
        "tag": tag,
        "set": card_set(card),
        "minutes": card.get("minutes"),
        "steps": card.get("steps"),
        "score": card.pointer("/evaluation/score"),
        "best_score": card.pointer("/best/score"),
    })
}

/// The gap bin's first frame count, so `5-10` sorts before `11-20`.
fn bin_start(bin: &str) -> u32 {
    bin.split('-')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(u32::MAX)
}

fn mm(card: &Value, bin: &str, which: &str) -> Option<f64> {
    card.pointer(&format!("/metrics/{bin}/{which}"))
        .and_then(Value::as_f64)
        .map(|m| (m * 10_000.0).round() / 10.0)
}

/// `review.models_compare`: two cards' numbers, gap by gap, and whether they can be compared.
pub fn compare(models: &Path, a: &str, b: &str) -> R<Value> {
    let card = |tag: &str| -> R<Value> {
        if training_set::check_name(tag).is_err() {
            return Err(format!("{tag:?} is no model's name"));
        }
        read_card(&models.join(tag))?.ok_or_else(|| format!("{tag} has no card: it never finished"))
    };
    let (ca, cb) = (card(a)?, card(b)?);
    let mut bins: Vec<String> = Vec::new();
    for c in [&ca, &cb] {
        if let Some(m) = c.get("metrics").and_then(Value::as_object) {
            bins.extend(
                m.keys()
                    .filter(|k| !bins.contains(k))
                    .cloned()
                    .collect::<Vec<_>>(),
            );
        }
    }
    bins.sort_by_key(|b| bin_start(b));
    let rows: Vec<Value> = bins
        .iter()
        .map(|bin| {
            json!({
                "frames": bin,
                "a_mm": mm(&ca, bin, "model_m"),
                "b_mm": mm(&cb, bin, "model_m"),
                "a_plain_mm": mm(&ca, bin, "interp_m"),
                "b_plain_mm": mm(&cb, bin, "interp_m"),
            })
        })
        .collect();
    let mut notes = Vec::new();
    for (c, tag) in [(&ca, a), (&cb, b)] {
        let on = c.get("validated_on").and_then(Value::as_str).unwrap_or("");
        if on.starts_with("training takes") {
            notes.push(format!(
                "{tag} was measured on clips it trained on, which flatters it"
            ));
        }
    }
    let measured = |c: &Value| {
        (
            c.pointer("/evaluation/sets").cloned(),
            c.pointer("/evaluation/holdout").cloned(),
        )
    };
    if measured(&ca) != measured(&cb) {
        notes.push("they were measured on different clips, so the numbers don't compare exactly".to_string());
    }
    Ok(json!({ "a": compared_side(&ca, a), "b": compared_side(&cb, b), "gaps": rows, "notes": notes }))
}

/// The set to run, checked: it keeps clips, and the library its private ones are in was given.
fn runnable(folder: &Path, set: &str, library: Option<&Path>) -> R<TrainingSet> {
    let ts = TrainingSet::load(folder, set)?;
    let kept = ts.clips.values().filter(|c| c.status == ClipStatus::Keep);
    let (mut n, mut private) = (0, 0);
    for c in kept {
        n += 1;
        private += usize::from(training_set::is_private(&c.id));
    }
    if n == 0 {
        return Err(format!("{set} keeps no clips"));
    }
    if private > 0 && library.is_none() {
        return Err(format!(
            "{set} keeps {private} clips of a library, and no library is open (--library <folder>)"
        ));
    }
    Ok(ts)
}

fn abs(p: &Path) -> PathBuf {
    std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf())
}

impl Review {
    /// The commands on models and their parameters.
    pub fn model_commands() -> Value {
        json!([
            {"id": "review.models", "params": "", "returns": "{rows: [{tag, date, state, usable, set, minutes, steps, styles, score, best_score, is_main, shipped, problem}], opens_with, main_is_shipped, main_problem, everyone_gets, job}: every model, newest first"},
            {"id": "review.train_check", "params": "set", "returns": "the job: the trainer's dry run, its totals in the log"},
            {"id": "review.train_start", "params": "set, tag, minutes (1 to 1440), steps? (stop after this many: a quick check)", "returns": "the job. A taken name is refused, and only one job runs at a time; a combined set is rebuilt first. On Windows the run inherits a pipe this command's output goes to, so send the output to a file, not into another program, or that program waits for the run"},
            {"id": "review.train_stop", "params": "force? (end it now, saving nothing more)", "returns": "{said}: a run finishes its step, evaluates and saves, and is marked stopped"},
            {"id": "review.job_status", "params": "", "returns": "{job, running, ended, ended_well, step, minutes, minutes_left, score, best_score, tail, problem}"},
            {"id": "review.model_main", "params": "tag (null: the shipped model)", "returns": "{main, exporting}: what the app opens with from its next start, exported first if it has to be"},
            {"id": "review.models_compare", "params": "a, b", "returns": "{a, b, gaps: [{frames, a_mm, b_mm, a_plain_mm, b_plain_mm}], notes}: the two cards' numbers"},
            {"id": "review.set_combine", "params": "name, from ([set names]), free? (the free clips too), public? (default: all of from are)", "returns": "{name, set_version, clips, keep, reject, from, free, free_clips, changed}: rebuilt whenever a run of it starts"},
        ])
    }

    pub const MODEL_IDS: [&'static str; 8] = [
        "review.models",
        "review.train_check",
        "review.train_start",
        "review.train_stop",
        "review.job_status",
        "review.model_main",
        "review.models_compare",
        "review.set_combine",
    ];

    pub(crate) fn run_models(&mut self, command: &str, p: &P) -> R<Value> {
        let word = |k: &str| -> R<String> {
            p.get(k)
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| format!("give {k}"))
        };
        match command {
            "review.models" => {
                let mut v = serde_json::to_value(list(&self.places)).map_err(|e| e.to_string())?;
                v["job"] =
                    serde_json::to_value(train_run::status(&self.places)).map_err(|e| e.to_string())?;
                Ok(v)
            }
            "review.train_check" => self.train_check(&word("set")?),
            "review.train_start" => {
                let minutes = p.get("minutes").and_then(Value::as_f64).ok_or("give minutes")?;
                let steps = match p.get("steps") {
                    None | Some(Value::Null) => None,
                    Some(v) => Some(
                        v.as_u64()
                            .filter(|n| *n > 0)
                            .ok_or("steps is a whole number above 0")?,
                    ),
                };
                self.train_start(&word("set")?, &word("tag")?, minutes, steps)
            }
            "review.train_stop" => {
                let force = p.get("force").and_then(Value::as_bool).unwrap_or(false);
                Ok(json!({ "said": train_run::stop(&self.places, force)? }))
            }
            "review.job_status" => {
                serde_json::to_value(train_run::status(&self.places)).map_err(|e| e.to_string())
            }
            "review.model_main" => match p.get("tag") {
                Some(Value::Null) => set_main(&self.places, None),
                Some(Value::String(t)) => set_main(&self.places, Some(t)),
                _ => Err("give tag: a model's name, or null for the shipped model".to_string()),
            },
            "review.models_compare" => compare(&self.places.models, &word("a")?, &word("b")?),
            "review.set_combine" => self.set_combine(p),
            other => Err(format!("no command {other:?}")),
        }
    }

    /// Before a run or its check: a combined set is built afresh, so later rejects count.
    fn ready_to_read(&mut self, set: &str) -> R<(PathBuf, Option<PathBuf>)> {
        let folder = abs(&self.folder()?);
        let library = self.places.library.as_deref().map(abs);
        self.recombine(set)?;
        runnable(&folder, set, library.as_deref())?;
        Ok((folder, library))
    }

    fn train_check(&mut self, set: &str) -> R<Value> {
        if let Some(j) = train_run::running(&self.places)? {
            return Err(format!("{} is running: wait for it or stop it first", j.tag));
        }
        let (folder, library) = self.ready_to_read(set)?;
        let job = train_run::start(
            &self.places,
            Start {
                kind: Kind::Check,
                tag: set.to_string(),
                set: Some(set.to_string()),
                minutes: None,
                steps: None,
                args: train_run::check_args(set, &folder, library.as_deref()),
                log: self.places.models.join(format!("check-{set}.log")),
                keep_log: false,
            },
        )?;
        serde_json::to_value(job).map_err(|e| e.to_string())
    }

    fn train_start(&mut self, set: &str, tag: &str, minutes: f64, steps: Option<u64>) -> R<Value> {
        if let Some(why) = name_problem(&self.places.models, tag) {
            return Err(why);
        }
        if !minutes.is_finite() || !(1.0..=MAX_MINUTES).contains(&minutes) {
            return Err(format!("minutes: 1 to {MAX_MINUTES}"));
        }
        if let Some(j) = train_run::running(&self.places)? {
            return Err(format!("{} is running: wait for it or stop it first", j.tag));
        }
        let (folder, library) = self.ready_to_read(set)?;
        // a stop asked of an earlier run of this name must not end this one
        std::fs::remove_file(train_run::stop_file(&self.places.models, tag)).ok();
        let job = train_run::start(
            &self.places,
            Start {
                kind: Kind::Train,
                tag: tag.to_string(),
                set: Some(set.to_string()),
                minutes: Some(minutes),
                steps,
                args: train_run::train_args(set, tag, minutes, steps, &folder, library.as_deref()),
                log: self.places.models.join(format!("{tag}.log")),
                keep_log: true,
            },
        )?;
        serde_json::to_value(job).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn days_are_the_calendars() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(951_782_400), "2000-02-29");
        assert_eq!(civil_date(1_793_491_200), "2026-11-01");
    }

    #[test]
    fn the_shipped_models_run_is_read_from_its_manifest() {
        let m = json!({"models": [
            {"tag": "other", "source": "training/data/models/x/best.pt"},
            {"tag": "parts", "source": "training/data/models/made-up-run/best.pt (ac_train.train, 180 min)"},
        ]});
        assert_eq!(shipped_from(&m), Some(("parts".into(), "made-up-run".into())));
        assert_eq!(shipped_from(&json!({"models": []})), None);
    }
}
