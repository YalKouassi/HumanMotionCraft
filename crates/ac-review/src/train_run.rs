//! The one job the review app runs at a time (roadmap 196000): a training run, the check before one,
//! or the export that makes a model the main one. It is `python -m ac_train...`, with no training
//! logic here. It outlives the window, so what it is goes in `current-job.json` beside the models,
//! and how it ended is worked out from what it left.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::{self, Places};

type R<T> = Result<T, String>;

pub const JOB_FILE: &str = "current-job.json";
/// How much of a log's end is read: a run's numbers and what a check found are all at its end.
const LOG_TAIL: u64 = 256 * 1024;
/// Lines of the log the page shows.
const TAIL_LINES: usize = 60;
/// Starts the line each job writes, so a name's earlier log isn't read as this run's.
const MARK: &str = "=== started";
const CHECKED: &str = "--dry-run: nothing trained";
const STOPPED_EARLY: &str = "asked to stop before the first step";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Train,
    Check,
    Main,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Job {
    pub kind: Kind,
    /// The model's name; for a check, the set's.
    pub tag: String,
    pub set: Option<String>,
    pub minutes: Option<f64>,
    pub steps: Option<u64>,
    pub pid: u32,
    /// The program's file name: a process with this id that is something else isn't the job.
    pub image: String,
    pub started: u64,
    pub log: PathBuf,
}

/// What the page shows of the job.
#[derive(Debug, Clone, Default, Serialize)]
pub struct JobStatus {
    pub job: Option<Job>,
    pub running: bool,
    /// How it ended, once it has.
    pub ended: Option<String>,
    pub ended_well: bool,
    pub step: Option<u64>,
    pub minutes: Option<f64>,
    pub minutes_left: Option<f64>,
    pub score: Option<f64>,
    pub best_score: Option<f64>,
    pub tail: Vec<String>,
    /// What couldn't be found out.
    pub problem: Option<String>,
}

fn job_file(p: &Places) -> PathBuf {
    p.models.join(JOB_FILE)
}

pub fn stop_file(models: &Path, tag: &str) -> PathBuf {
    models.join(format!("{tag}.stop"))
}

pub fn load_job(p: &Places) -> R<Option<Job>> {
    let path = job_file(p);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The training folder's Python. No other is tried: a run on the wrong one would fail far later.
pub fn python(training: &Path) -> R<PathBuf> {
    [".venv/Scripts/python.exe", ".venv/bin/python"]
        .iter()
        .map(|rel| training.join(rel))
        .find(|p| p.is_file())
        .ok_or_else(|| {
            format!(
                "no Python in {}: make it as training/README.md says",
                training.join(".venv").display()
            )
        })
}

/// What a set's run is told about the set.
fn set_args(set: &str, folder: &Path, library: Option<&Path>) -> Vec<String> {
    let mut out = vec![
        format!("--set={set}"),
        format!("--set-folder={}", folder.display()),
    ];
    if let Some(l) = library {
        out.push(format!("--library={}", l.display()));
    }
    out
}

/// The trainer's arguments for a run.
pub fn train_args(
    set: &str,
    tag: &str,
    minutes: f64,
    steps: Option<u64>,
    folder: &Path,
    library: Option<&Path>,
) -> Vec<String> {
    let mut out = vec!["-m".to_string(), "ac_train.train".to_string()];
    out.extend(set_args(set, folder, library));
    out.push(format!("--tag={tag}"));
    out.push(format!("--minutes={minutes}"));
    if let Some(n) = steps {
        out.push(format!("--steps={n}"));
    }
    out
}

/// The trainer's arguments for the look before a run.
pub fn check_args(set: &str, folder: &Path, library: Option<&Path>) -> Vec<String> {
    let mut out = vec!["-m".to_string(), "ac_train.train".to_string()];
    out.extend(set_args(set, folder, library));
    out.push("--dry-run".to_string());
    out
}

/// A model's goldens prove a port against PyTorch; a main model's are thrown away.
fn golden_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("ac-review-golden-{tag}"))
}

/// The exporter's arguments to make a model main: its best checkpoint, if it has one.
pub fn main_args(p: &Places, tag: &str) -> Vec<String> {
    let checkpoint = if p.models.join(tag).join("best.pt").is_file() {
        "best"
    } else {
        "model"
    };
    vec![
        "-m".to_string(),
        "ac_train.export".to_string(),
        tag.to_string(),
        format!("--checkpoint={checkpoint}"),
        format!("--out={}", p.installed.join(tag).display()),
        format!("--golden={}", golden_dir(tag).display()),
        "--main".to_string(),
    ]
}

/// No console window for a process started from the window.
fn quiet(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Whether the process is there and is the job.
#[cfg(windows)]
fn alive(job: &Job) -> R<bool> {
    let out =
        quiet(Command::new("tasklist").args(["/FI", &format!("PID eq {}", job.pid), "/NH", "/FO", "CSV"]))
            .output()
            .map_err(|e| format!("can't ask whether the job still runs (tasklist): {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let wanted = format!("\"{}\"", job.pid);
    Ok(text.lines().any(|l| {
        let mut fields = l.split(',');
        let image = fields.next().unwrap_or("").trim_matches('"');
        image.eq_ignore_ascii_case(&job.image) && fields.next() == Some(wanted.as_str())
    }))
}

/// Whether the process is there.
#[cfg(not(windows))]
fn alive(job: &Job) -> R<bool> {
    let out = quiet(
        Command::new("kill")
            .args(["-0", &job.pid.to_string()])
            .stderr(Stdio::null()),
    )
    .status()
    .map_err(|e| format!("can't ask whether the job still runs (kill -0): {e}"))?;
    Ok(out.success())
}

/// The job running now, if one is.
pub fn running(p: &Places) -> R<Option<Job>> {
    match load_job(p)? {
        Some(j) if alive(&j)? => Ok(Some(j)),
        _ => Ok(None),
    }
}

/// What a job is, in a sentence's words.
fn what(job: &Job) -> String {
    match job.kind {
        Kind::Train => format!("the training run {}", job.tag),
        Kind::Check => format!("the check of {}", job.tag),
        Kind::Main => format!("the export of {}", job.tag),
    }
}

/// What to start.
pub struct Start {
    pub kind: Kind,
    pub tag: String,
    pub set: Option<String>,
    pub minutes: Option<f64>,
    pub steps: Option<u64>,
    pub args: Vec<String>,
    pub log: PathBuf,
    /// Add to the log, as a run under a name that had one before does.
    pub keep_log: bool,
}

/// Start a job, unless one is running. It writes to its log and goes on after the window closes.
pub fn start(p: &Places, asked: Start) -> R<Job> {
    if let Some(j) = running(p)? {
        return Err(format!("{} is running: wait for it or stop it first", what(&j)));
    }
    let program = python(&p.training)?;
    std::fs::create_dir_all(&p.models).map_err(|e| format!("{}: {e}", p.models.display()))?;
    let at = |e: std::io::Error| format!("{}: {e}", asked.log.display());
    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(asked.keep_log)
        .truncate(!asked.keep_log)
        .open(&asked.log)
        .map_err(at)?;
    let started = now();
    writeln!(out, "{MARK} {} UTC", models::civil_date(started)).map_err(at)?;
    let err = out.try_clone().map_err(at)?;
    let mut cmd = Command::new(&program);
    cmd.args(&asked.args)
        .current_dir(&p.training)
        .env("PYTHONUNBUFFERED", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err);
    let mut child = quiet(&mut cmd)
        .spawn()
        .map_err(|e| format!("{}: {e}", program.display()))?;
    let job = Job {
        kind: asked.kind,
        tag: asked.tag,
        set: asked.set,
        minutes: asked.minutes,
        steps: asked.steps,
        pid: child.id(),
        image: program
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        started,
        log: asked.log,
    };
    // reaped when it ends, for as long as this process lasts; it goes on without us
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    let saved = serde_json::to_vec_pretty(&job)
        .map_err(|e| e.to_string())
        .and_then(|bytes| {
            craft_atomic::atomic_write(&job_file(p), &bytes)
                .map_err(|e| format!("{}: {e}", job_file(p).display()))
        });
    if let Err(e) = saved {
        return Err(format!(
            "{} started (process {}), but the window won't be able to follow it: {e}",
            what(&job),
            job.pid
        ));
    }
    Ok(job)
}

/// Stop the job: a training run finishes its step, evaluates and saves. `force` ends the process
/// now, which saves nothing more than the last best checkpoint.
pub fn stop(p: &Places, force: bool) -> R<String> {
    let job = running(p)?.ok_or("nothing is running")?;
    if force {
        kill(job.pid)?;
        if job.kind == Kind::Train {
            std::fs::remove_file(stop_file(&p.models, &job.tag)).ok();
        }
        return Ok(format!(
            "{} was ended at once: it left only what it had saved",
            what(&job)
        ));
    }
    if job.kind != Kind::Train {
        return Err(format!(
            "{} can only be ended at once, by forcing it, or left to finish",
            what(&job)
        ));
    }
    let path = stop_file(&p.models, &job.tag);
    std::fs::write(&path, b"").map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(format!(
        "{} will finish its step, evaluate and save, which can take a few minutes",
        what(&job)
    ))
}

#[cfg(windows)]
fn kill(pid: u32) -> R<()> {
    let out = quiet(Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]))
        .output()
        .map_err(|e| format!("taskkill: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "taskkill: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

#[cfg(not(windows))]
fn kill(pid: u32) -> R<()> {
    let out = Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()
        .map_err(|e| format!("kill: {e}"))?;
    if out.success() {
        Ok(())
    } else {
        Err(format!("kill: process {pid} couldn't be ended"))
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The end of a log: what the job wrote since its start line, or as much of it as is read.
fn log_end(path: &Path) -> String {
    let Ok(mut f) = std::fs::File::open(path) else {
        return String::new();
    };
    let len = f.metadata().map_or(0, |m| m.len());
    if f.seek(SeekFrom::Start(len.saturating_sub(LOG_TAIL))).is_err() {
        return String::new();
    }
    let mut bytes = Vec::new();
    if f.read_to_end(&mut bytes).is_err() {
        return String::new();
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    match text.rfind(MARK) {
        Some(at) => text.get(at..).unwrap_or_default().to_string(),
        None => text,
    }
}

/// A trainer's progress line (`{"step": 1000, "minutes": 3.2, "score": 0.6, ...}`) for a person.
fn progress_line(v: &Value) -> Option<String> {
    let step = v.get("step")?.as_u64()?;
    let minutes = v.get("minutes").and_then(Value::as_f64).unwrap_or(0.0);
    let score = v
        .get("score")
        .and_then(Value::as_f64)
        .map_or(String::new(), |s| format!(", score {s:.3}"));
    Some(format!("step {step}, {minutes:.1} min{score}"))
}

/// Read what the log says into the status.
fn read_log(text: &str, s: &mut JobStatus) {
    let mut lines: Vec<String> = Vec::new();
    for l in text.lines() {
        let parsed: Option<Value> = l
            .starts_with("{\"step\"")
            .then(|| serde_json::from_str(l).ok())
            .flatten();
        match parsed.as_ref().and_then(|v| progress_line(v).map(|t| (v, t))) {
            Some((v, shown)) => {
                s.step = v.get("step").and_then(Value::as_u64);
                s.minutes = v.get("minutes").and_then(Value::as_f64);
                if let Some(score) = v.get("score").and_then(Value::as_f64) {
                    s.score = Some(score);
                    s.best_score = Some(s.best_score.map_or(score, |b: f64| b.min(score)));
                }
                lines.push(shown);
            }
            None => lines.push(l.to_string()),
        }
    }
    let from = lines.len().saturating_sub(TAIL_LINES);
    s.tail = lines.split_off(from);
}

/// How a job that isn't running ended, from what it left: (the words, whether it went as asked).
fn how_it_ended(p: &Places, job: &Job, log: &str) -> (String, bool) {
    match job.kind {
        Kind::Train => {
            let card = job_card(&p.models, &job.tag);
            match card {
                Some(c) if c.get("stopped").and_then(Value::as_bool) == Some(true) => {
                    (format!("{} stopped early and saved what it had", job.tag), true)
                }
                Some(_) => (format!("{} finished", job.tag), true),
                None if log.contains(STOPPED_EARLY) => (
                    format!("{} stopped before its first step: nothing was written", job.tag),
                    true,
                ),
                None => (
                    format!("{} ended without finishing: see the end of its log", job.tag),
                    false,
                ),
            }
        }
        Kind::Check => {
            if log.contains(CHECKED) {
                (format!("{}: checked, nothing trained", job.tag), true)
            } else {
                (
                    format!("the check of {} failed: see the end of its log", job.tag),
                    false,
                )
            }
        }
        Kind::Main => {
            if ac_ai::main_model(&p.installed) == Ok(ac_ai::MainModel::Tag(job.tag.clone())) {
                (format!("{} is the main model", job.tag), true)
            } else {
                (
                    format!("{} didn't become the main model: see the end of its log", job.tag),
                    false,
                )
            }
        }
    }
}

fn job_card(models: &Path, tag: &str) -> Option<Value> {
    let text = std::fs::read_to_string(models.join(tag).join("model.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// The job now, for the page and `review.job_status`.
pub fn status(p: &Places) -> JobStatus {
    let mut s = JobStatus::default();
    let job = match load_job(p) {
        Ok(Some(j)) => j,
        Ok(None) => return s,
        Err(e) => {
            s.problem = Some(e);
            return s;
        }
    };
    match alive(&job) {
        Ok(r) => s.running = r,
        Err(e) => s.problem = Some(e),
    }
    let log = log_end(&job.log);
    read_log(&log, &mut s);
    if let (Some(total), Some(used)) = (job.minutes, s.minutes) {
        s.minutes_left = Some((total - used).max(0.0));
    }
    if !s.running && s.problem.is_none() {
        let (words, well) = how_it_ended(p, &job, &log);
        s.ended = Some(words);
        s.ended_well = well;
        if job.kind == Kind::Main {
            std::fs::remove_dir_all(golden_dir(&job.tag)).ok();
        }
    }
    s.job = Some(job);
    s
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn a_run_asks_the_trainer_for_what_was_picked_and_nothing_else() {
        let set = Path::new("/sets");
        let lib = Path::new("/lib");
        assert_eq!(
            train_args("base", "base-1", 180.0, None, set, Some(lib)),
            [
                "-m",
                "ac_train.train",
                "--set=base",
                "--set-folder=/sets",
                "--library=/lib",
                "--tag=base-1",
                "--minutes=180"
            ]
        );
        let quick = train_args("base", "t", 1.5, Some(20), set, None);
        assert!(quick.contains(&"--steps=20".to_string()), "{quick:?}");
        assert!(quick.contains(&"--minutes=1.5".to_string()), "{quick:?}");
        assert!(!quick.iter().any(|a| a.starts_with("--library")));
        let check = check_args("base", set, None);
        assert!(check.contains(&"--dry-run".to_string()));
        assert!(!check.iter().any(|a| a.starts_with("--tag")));
    }

    #[test]
    fn the_logs_numbers_are_read_and_its_earlier_runs_are_not() {
        let dir = std::env::temp_dir().join(format!("ac-review-log-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("a.log");
        std::fs::write(
            &log,
            format!(
                "{MARK} earlier\n{{\"step\": 9000, \"minutes\": 90.0, \"score\": 0.1}}\n{MARK} now\nsets: x\n\
                 {{\"step\": 1000, \"minutes\": 3.2, \"loss\": 1.0, \"score\": 0.7}}\n\
                 {{\"step\": 2000, \"minutes\": 6.4, \"loss\": 0.9, \"score\": 0.8}}\n"
            ),
        )
        .unwrap();
        let mut s = JobStatus::default();
        read_log(&log_end(&log), &mut s);
        assert_eq!(s.step, Some(2000));
        assert_eq!(s.minutes, Some(6.4));
        assert_eq!(s.score, Some(0.8));
        assert_eq!(s.best_score, Some(0.7));
        assert_eq!(s.tail.last().unwrap(), "step 2000, 6.4 min, score 0.800");
        assert!(!s.tail.iter().any(|l| l.contains("9000")));
        std::fs::remove_dir_all(&dir).ok();
    }
}
