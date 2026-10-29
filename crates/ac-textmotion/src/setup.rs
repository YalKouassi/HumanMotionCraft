//! Installing the helper from the app (`text.install`): the setup `tools/textmotion/setup_textmotion.py`
//! runs as a child process through `uv run` (so no Python of the person's own is needed), its
//! output going to `setup.log` in the install folder. It prints numbered step lines
//! (`== [3/6] Text encoder ...`), which [`feed`] reads for the progress.
//!
//! It runs with `--stop-on-eof`: closing its stdin ([`stop`], or this process ending) ends it and
//! everything it started, and starting it again carries on, since the setup skips finished steps.
//! What the app can see is checked before it starts ([`requirements`]); the setup checks the free
//! disk itself and fails at once when there is too little. The setup needs the source tree it
//! sits in (it copies the rig maths from `training/`), found next to the assets folder.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{Value, json};

use crate::install;

/// The setup, from the source tree's root.
pub const SCRIPT: &str = "tools/textmotion/setup_textmotion.py";
/// Its log, in the install folder.
pub const LOG: &str = "setup.log";
/// The Python `uv run` runs the setup with (the model's own environment is the setup's job).
const PYTHON: &str = "3.12";
/// The exit code the setup ends with when its stdin closes.
const STOPPED: i32 = 3;
/// How long a Stop waits for the setup to end itself before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(20);
/// The most of the log read at once.
const CHUNK: u64 = 1 << 20;

/// One thing the install needs, and what was found.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Need {
    pub name: &'static str,
    pub ok: bool,
    /// The install can't start without it (else it only works less well).
    pub required: bool,
    /// What was found, or what to do.
    pub note: String,
}

/// What the log says so far.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Progress {
    /// The step working now (0: none yet) of `steps`.
    pub step: i64,
    pub steps: i64,
    pub label: String,
    /// The last line printed since the step began (a download's progress, an error).
    pub last: String,
    /// The setup printed `== done`: it finished (whatever its exit code says).
    pub done: bool,
}

/// Read `chunk`, the log's next bytes, into `p`; `tail` holds a line not finished yet.
pub fn feed(p: &mut Progress, tail: &mut String, chunk: &str) {
    tail.push_str(chunk);
    // progress bars rewrite their line with \r: each one is a line here
    let Some(end) = tail.rfind(['\n', '\r']) else {
        return;
    };
    let done: String = tail.drain(..=end).collect();
    for l in done.split(['\n', '\r']) {
        line(p, l);
    }
}

/// `raw` without terminal colour codes (`ESC [ ... m`) or other control characters.
fn plain(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.next() == Some('[') {
                // parameters, then one letter ends it
                for d in chars.by_ref() {
                    if d.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else if !c.is_control() {
            out.push(c);
        }
    }
    out
}

fn line(p: &mut Progress, raw: &str) {
    let clean = plain(raw);
    let l = clean.trim();
    if l.is_empty() {
        return;
    }
    if l == "== done" {
        p.done = true;
        return;
    }
    let numbered = l
        .strip_prefix("== [")
        .and_then(|r| r.split_once(']'))
        .and_then(|(n, label)| {
            let (a, b) = n.split_once('/')?;
            Some((a.trim().parse().ok()?, b.trim().parse().ok()?, label.trim()))
        });
    match numbered {
        Some((step, steps, label)) => {
            *p = Progress {
                step,
                steps,
                label: label.to_string(),
                last: String::new(),
                done: false,
            };
        }
        None => p.last = l.chars().take(240).collect(),
    }
}

/// A setup started by this process.
struct Run {
    child: Child,
    /// Closed (dropped) to stop it.
    stdin: Option<ChildStdin>,
    log: PathBuf,
    read: u64,
    tail: String,
    progress: Progress,
    since: Instant,
    stop_asked: Option<Instant>,
    exit: Option<ExitStatus>,
}

static RUN: Mutex<Option<Run>> = Mutex::new(None);
static STAND_IN: AtomicBool = AtomicBool::new(false);
static GPU: OnceLock<Option<String>> = OnceLock::new();

fn lock() -> MutexGuard<'static, Option<Run>> {
    RUN.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Fixture runs: report the install as there and never start the setup.
pub fn use_stand_in() {
    STAND_IN.store(true, Ordering::Relaxed);
}

impl Run {
    fn poll(&mut self) {
        if self.exit.is_none() {
            match self.child.try_wait() {
                Ok(Some(s)) => self.exit = Some(s),
                Ok(None) => {
                    if self.stop_asked.is_some_and(|t| t.elapsed() > STOP_GRACE) {
                        let _ = self.child.kill();
                    }
                }
                Err(_) => {}
            }
        }
        let Ok(mut f) = std::fs::File::open(&self.log) else {
            return;
        };
        if f.seek(SeekFrom::Start(self.read)).is_err() {
            return;
        }
        let mut buf = Vec::new();
        if let Ok(n) = f.take(CHUNK).read_to_end(&mut buf) {
            self.read += n as u64;
            feed(&mut self.progress, &mut self.tail, &String::from_utf8_lossy(&buf));
        }
    }
}

/// The source tree's root: the assets folder's parent, when the setup is in it.
fn tree(assets: Option<&Path>) -> Option<PathBuf> {
    let root = assets?.parent()?.to_path_buf();
    script(&root).is_file().then_some(root)
}

/// The setup in the tree at `root`.
fn script(root: &Path) -> PathBuf {
    SCRIPT.split('/').fold(root.to_path_buf(), |p, part| p.join(part))
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// `name` on PATH.
fn on_path(name: &str) -> Option<PathBuf> {
    let file = exe(name);
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(&file))
        .find(|p| p.is_file())
}

/// `uv`: on PATH, or where its own installer puts it.
fn uv() -> Option<PathBuf> {
    on_path("uv").or_else(|| {
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })?;
        let home = PathBuf::from(home);
        [home.join(".local/bin"), home.join(".cargo/bin")]
            .into_iter()
            .map(|d| d.join(exe("uv")))
            .find(|p| p.is_file())
    })
}

fn quiet(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flashes up
    }
    cmd
}

fn output(cmd: &mut Command) -> Option<String> {
    let out = quiet(cmd)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The NVIDIA GPU (its name and memory), asked once a process.
fn gpu() -> Option<String> {
    GPU.get_or_init(|| {
        let out = output(
            Command::new("nvidia-smi")
                .args(["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"]),
        )?;
        let first = out.lines().next()?;
        let (name, mib) = first.rsplit_once(',')?;
        let gb = mib.trim().parse::<f64>().ok()? / 1024.0;
        Some(format!("{} ({gb:.0} GB)", name.trim()))
    })
    .clone()
}

/// Visual Studio with the C++ tools (Windows), which build the model's motion correction.
fn msvc() -> Result<String, String> {
    let base = std::env::var_os("ProgramFiles(x86)")
        .map_or_else(|| PathBuf::from(r"C:\Program Files (x86)"), PathBuf::from);
    let vswhere = base.join(r"Microsoft Visual Studio\Installer\vswhere.exe");
    let found = output(Command::new(&vswhere).args([
        "-latest",
        "-products",
        "*",
        "-requires",
        "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
        "-property",
        "displayName",
    ]));
    found.filter(|s| !s.is_empty()).ok_or_else(|| {
        "not found: install Visual Studio's \"Desktop development with C++\" (or the Build Tools), as for Rust".into()
    })
}

/// What the install needs, checked now.
pub fn requirements(assets: Option<&Path>) -> Vec<Need> {
    let mut needs = vec![
        Need {
            name: "Source tree",
            ok: tree(assets).is_some(),
            required: true,
            note: match tree(assets) {
                Some(root) => script(&root).display().to_string(),
                None => format!(
                    "{SCRIPT} is not next to the assets folder: run the app from its source tree"
                ),
            },
        },
        Need {
            name: "uv",
            ok: uv().is_some(),
            required: true,
            note: uv().map_or_else(
                || {
                    if cfg!(windows) {
                        "not found: install it with `winget install astral-sh.uv` (https://docs.astral.sh/uv/)".into()
                    } else {
                        "not found: install it from https://docs.astral.sh/uv/".into()
                    }
                },
                |p| p.display().to_string(),
            ),
        },
        Need {
            name: "Git",
            ok: on_path("git").is_some(),
            required: true,
            note: on_path("git").map_or_else(
                || "not found: install Git (https://git-scm.com)".into(),
                |p| p.display().to_string(),
            ),
        },
    ];
    if cfg!(windows) {
        let m = msvc();
        needs.push(Need {
            name: "C++ build tools",
            ok: m.is_ok(),
            required: true,
            note: m.unwrap_or_else(|e| e),
        });
    }
    let g = gpu();
    needs.push(Need {
        name: "NVIDIA GPU",
        ok: g.is_some(),
        required: false,
        note: g.unwrap_or_else(|| {
            "none found: it installs, but making motion takes many minutes on the CPU".into()
        }),
    });
    needs
}

/// What the install is doing, as `text.install` reports it.
pub fn status(dir: &Path) -> Value {
    if STAND_IN.load(Ordering::Relaxed) {
        return json!({ "state": "installed", "dir": dir.display().to_string() });
    }
    let log = dir.join(LOG);
    let mut out = json!({ "dir": dir.display().to_string(), "log": log.display().to_string() });
    let mut g = lock();
    let (state, p, error) = match g.as_mut() {
        Some(r) => {
            r.poll();
            out["seconds"] = json!(r.since.elapsed().as_secs());
            let state = match (r.exit, r.stop_asked) {
                (None, None) => "installing",
                (None, Some(_)) => "stopping",
                // finished: a crash while Python shuts down mustn't hide that
                (Some(s), _) if s.success() || r.progress.done => {
                    if install::read(dir).is_ok() {
                        "installed"
                    } else {
                        "failed"
                    }
                }
                (Some(s), asked) if asked.is_some() || s.code() == Some(STOPPED) => "stopped",
                (Some(_), _) => "failed",
            };
            let error = match state {
                "failed" if r.progress.last.is_empty() => "the setup ended without saying why".into(),
                "failed" => r.progress.last.clone(),
                _ => String::new(),
            };
            (state, r.progress.clone(), error)
        }
        None if install::read(dir).is_ok() => ("installed", Progress::default(), String::new()),
        None => match std::fs::read(&log) {
            // an install that ended in an earlier run of the app
            Ok(bytes) => {
                let (mut p, mut tail) = (Progress::default(), String::new());
                feed(&mut p, &mut tail, &String::from_utf8_lossy(&bytes));
                feed(&mut p, &mut tail, "\n");
                if p.last.starts_with("== stopped") || p.last.is_empty() {
                    ("stopped", p, String::new())
                } else {
                    let e = p.last.clone();
                    ("failed", p, e)
                }
            }
            Err(_) => ("not installed", Progress::default(), String::new()),
        },
    };
    out["state"] = json!(state);
    if p.steps > 0 {
        out["step"] = json!(p.step);
        out["steps"] = json!(p.steps);
        out["label"] = json!(p.label);
    }
    if !p.last.is_empty() && state != "installed" {
        out["last"] = json!(p.last);
    }
    if !error.is_empty() {
        out["error"] = json!(error);
    }
    out
}

/// Whether a setup started here is still going.
pub fn running() -> bool {
    lock().as_mut().is_some_and(|r| {
        r.poll();
        r.exit.is_none()
    })
}

/// Start the setup into `dir` (`repair`: even when it is installed already).
pub fn start(dir: &Path, assets: Option<&Path>, repair: bool) -> Result<(), String> {
    if STAND_IN.load(Ordering::Relaxed) {
        return Err("the fixtures' stand-in installs nothing".into());
    }
    let mut g = lock();
    if let Some(r) = g.as_mut() {
        r.poll();
        if r.exit.is_none() {
            return Err(match r.progress.steps {
                0 => "the install is running already".into(),
                n => format!("the install is running already (step {} of {n})", r.progress.step),
            });
        }
    }
    if !repair && install::read(dir).is_ok() {
        return Err(ac_core::error::hinted(
            format!("motion from text is installed already, in {}", dir.display()),
            "pass repair: true to run the setup again (it updates or repairs the install)",
        ));
    }
    let missing: Vec<String> = requirements(assets)
        .into_iter()
        .filter(|n| n.required && !n.ok)
        .map(|n| format!("{}: {}", n.name, n.note))
        .collect();
    if !missing.is_empty() {
        return Err(format!("the install can't start yet. {}", missing.join("; ")));
    }
    let root = tree(assets).ok_or("the source tree was not found")?;
    let uv = uv().ok_or("uv was not found")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("can't make {}: {e}", dir.display()))?;
    let log = dir.join(LOG);
    let out = std::fs::File::create(&log).map_err(|e| format!("can't write {}: {e}", log.display()))?;
    let err = out
        .try_clone()
        .map_err(|e| format!("can't write {}: {e}", log.display()))?;
    // the setup calls `uv` by name: put the one found first on PATH
    let mut path = vec![uv.parent().map(Path::to_path_buf).unwrap_or_default()];
    path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(path).map_err(|e| e.to_string())?;
    let mut cmd = Command::new(&uv);
    cmd.args(["run", "--no-project", "--python", PYTHON])
        .arg(script(&root))
        .arg("--dir")
        .arg(dir)
        .arg("--stop-on-eof")
        .env("PATH", path)
        .env("PYTHONUNBUFFERED", "1")
        .env("PYTHONUTF8", "1")
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err));
    let mut child = quiet(&mut cmd)
        .spawn()
        .map_err(|e| format!("can't start the setup with {}: {e}", uv.display()))?;
    let stdin = child.stdin.take();
    *g = Some(Run {
        child,
        stdin,
        log,
        read: 0,
        tail: String::new(),
        progress: Progress::default(),
        since: Instant::now(),
        stop_asked: None,
        exit: None,
    });
    Ok(())
}

/// Ask the running setup to stop (it ends what it started, then itself).
pub fn stop() -> Result<(), String> {
    let mut g = lock();
    let r = g.as_mut().ok_or("no install is running")?;
    r.poll();
    if r.exit.is_some() {
        return Err("no install is running".into());
    }
    if r.stop_asked.is_none() {
        r.stdin = None;
        r.stop_asked = Some(Instant::now());
    }
    Ok(())
}
