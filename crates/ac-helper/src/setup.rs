//! Installing a helper from the app: its setup (`tools/<name>/setup_<name>.py`) runs as a child
//! process through `uv run` (so no Python of the person's own is needed), its output going to
//! `setup.log` in the install folder. It prints numbered step lines (`== [3/6] Text encoder ...`),
//! which [`feed`] reads for the progress.
//!
//! It runs with `--stop-on-eof`: closing its stdin ([`Installer::stop`], or this process ending)
//! ends it and everything it started, and starting it again carries on, since the setup skips
//! finished steps. What the app can see is checked before it starts
//! ([`Installer::requirements`]); the setup checks the free disk itself and fails at once when
//! there is too little. The setup needs the source tree it sits in (it copies the rig maths from
//! `training/`), found next to the assets folder.
//!
//! Without a uv of the person's own, the install first fetches one ([`crate::uv_fetch`]): the
//! download and the unpacking run as children too, one after the other, then the setup.
//!
//! Each helper crate describes its setup in a [`Spec`] and holds one [`Installer`] in a static:
//! one setup of each kind runs at a time.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{Value, json};

use crate::install::{self, Kind};
use crate::process::quiet;
use crate::uv_fetch;

/// The setup's log, in the install folder.
pub const LOG: &str = "setup.log";
/// The exit code a setup ends with when its stdin closes.
const STOPPED: i32 = 3;
/// How long a Stop waits for the setup to end itself before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(20);
/// The most of the log read at once.
const CHUNK: u64 = 1 << 20;

/// One helper's setup.
pub struct Spec {
    pub kind: &'static Kind,
    /// The setup, from the source tree's root (`tools/textmotion/setup_textmotion.py`).
    pub script: &'static str,
    /// The Python `uv run` runs the setup with (the model's own environment is the setup's job).
    pub python: &'static str,
    /// Windows: the setup builds C++ (Visual Studio's build tools are required).
    pub msvc: bool,
    /// The setup refuses without an NVIDIA GPU.
    pub gpu_required: bool,
    /// What to say when no NVIDIA GPU is found.
    pub without_gpu: &'static str,
}

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

/// What a run's child is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Stage {
    FetchUv,
    UnpackUv,
    Setup,
}

/// A setup started by this process.
struct Run {
    child: Child,
    stage: Stage,
    /// The install folder.
    dir: PathBuf,
    /// The source tree the setup is in.
    root: PathBuf,
    /// Why a stage before the setup failed (the log says it too).
    failed: Option<String>,
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

/// One helper's installs: the setup running now (one at a time), and the fixtures' stand-in.
pub struct Installer {
    spec: &'static Spec,
    run: Mutex<Option<Run>>,
    stand_in: AtomicBool,
}

static GPU: OnceLock<Option<String>> = OnceLock::new();

impl Run {
    fn poll(&mut self, spec: &Spec) {
        if self.exit.is_none() {
            match self.child.try_wait() {
                Ok(Some(s)) => {
                    self.exit = Some(s);
                    self.advance(spec);
                }
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

    /// A stage before the setup ended: start the next one, or say why it can't go on.
    fn advance(&mut self, spec: &Spec) {
        let Some(status) = self.exit else {
            return;
        };
        if self.stage == Stage::Setup || self.stop_asked.is_some() {
            return;
        }
        let next = if status.success() {
            match self.stage {
                Stage::FetchUv => uv_fetch::unpack(&self.dir).map(|c| (c, Stage::UnpackUv)),
                _ => uv_fetch::finish(&self.dir)
                    .and_then(|uv| setup_command(spec, &uv, &self.root, &self.dir))
                    .map(|c| (c, Stage::Setup)),
            }
        } else {
            let what = if self.stage == Stage::FetchUv {
                "downloaded: check the internet connection and press Install again"
            } else {
                "unpacked: press Install again"
            };
            Err(format!("uv could not be {what} ({status})"))
        };
        let started = next.and_then(|(cmd, stage)| {
            let (child, stdin) = launch(cmd, &self.log)?;
            Ok((child, stdin, stage))
        });
        match started {
            Ok((child, stdin, stage)) => {
                self.child = child;
                self.stdin = stdin;
                self.stage = stage;
                self.exit = None;
            }
            Err(e) => {
                // in the log too, where a later run of the app reads why it ended
                self.failed = Some(match append(&self.log, &format!("{e}\n")) {
                    Ok(()) => e,
                    Err(w) => format!("{e} ({w})"),
                });
            }
        }
    }
}

fn append(log: &Path, text: &str) -> Result<(), String> {
    use std::io::Write;
    std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(log)
        .and_then(|mut f| f.write_all(text.as_bytes()))
        .map_err(|e| format!("can't write {}: {e}", log.display()))
}

/// Start `cmd` with its output going to the end of `log`, and a pipe for its stdin.
fn launch(mut cmd: Command, log: &Path) -> Result<(Child, Option<ChildStdin>), String> {
    let open = || {
        std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(log)
            .map_err(|e| format!("can't write {}: {e}", log.display()))
    };
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::from(open()?))
        .stderr(Stdio::from(open()?));
    let mut child = quiet(&mut cmd)
        .spawn()
        .map_err(|e| format!("can't start {}: {e}", cmd.get_program().to_string_lossy()))?;
    let stdin = child.stdin.take();
    Ok((child, stdin))
}

/// The setup in the tree at `root`, run by `uv` into `dir`.
fn setup_command(spec: &Spec, uv: &Path, root: &Path, dir: &Path) -> Result<Command, String> {
    // the setup calls `uv` by name: put this one first on PATH
    let mut path = vec![uv.parent().map(Path::to_path_buf).unwrap_or_default()];
    path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(path).map_err(|e| e.to_string())?;
    let mut cmd = Command::new(uv);
    // a Python uv downloads into the install, whatever is (or isn't) installed and however the
    // person's uv is set up
    cmd.args(["run", "--no-project", "--managed-python", "--python", spec.python])
        .arg(script(spec, root))
        .arg("--dir")
        .arg(dir)
        .arg("--stop-on-eof")
        .env("PATH", path)
        .env("UV_PYTHON_INSTALL_DIR", dir.join("python"))
        .env("UV_PYTHON_DOWNLOADS", "automatic")
        .env("PYTHONUNBUFFERED", "1")
        .env("PYTHONUTF8", "1")
        .current_dir(root);
    Ok(cmd)
}

/// The source tree's root: the assets folder's parent, when the setup is in it.
fn tree(spec: &Spec, assets: Option<&Path>) -> Option<PathBuf> {
    let root = assets?.parent()?.to_path_buf();
    script(spec, &root).is_file().then_some(root)
}

/// The setup in the tree at `root`.
fn script(spec: &Spec, root: &Path) -> PathBuf {
    spec.script
        .split('/')
        .fold(root.to_path_buf(), |p, part| p.join(part))
}

pub(crate) fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// `name` on PATH.
pub(crate) fn on_path(name: &str) -> Option<PathBuf> {
    let file = exe(name);
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(&file))
        .find(|p| p.is_file())
}

/// `uv`: on PATH, where its own installer puts it, or the one fetched into the install `dir`.
fn uv(dir: &Path) -> Option<PathBuf> {
    on_path("uv").or_else(|| {
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
        home.iter()
            .flat_map(|h| [h.join(".local/bin"), h.join(".cargo/bin")])
            .map(|d| d.join(exe("uv")))
            .chain([uv_fetch::exe(dir)])
            .find(|p| p.is_file())
    })
}

/// The uv requirement: found, or fetched by the install (else what to do).
fn uv_need(dir: &Path) -> Need {
    let (ok, note) = match (uv(dir), uv_fetch::possible()) {
        (Some(p), _) => (true, p.display().to_string()),
        (None, Ok(())) => (
            true,
            format!(
                "not installed: the install downloads uv {} (about {} MB) into {}",
                uv_fetch::VERSION,
                uv_fetch::MB,
                uv_fetch::home(dir).display()
            ),
        ),
        (None, Err(why)) => (false, format!("not found, and {why}")),
    };
    Need {
        name: "uv",
        ok,
        required: true,
        note,
    }
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
pub fn gpu() -> Option<String> {
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

impl Installer {
    pub const fn new(spec: &'static Spec) -> Installer {
        Installer {
            spec,
            run: Mutex::new(None),
            stand_in: AtomicBool::new(false),
        }
    }

    pub fn spec(&self) -> &'static Spec {
        self.spec
    }

    fn lock(&self) -> MutexGuard<'_, Option<Run>> {
        self.run.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Fixture runs: report the install as there and never start the setup.
    pub fn use_stand_in(&self) {
        self.stand_in.store(true, Ordering::Relaxed);
    }

    fn read(&self, dir: &Path) -> Result<install::Install, String> {
        install::read(self.spec.kind, dir)
    }

    /// What the install into `dir` needs, checked now.
    pub fn requirements(&self, assets: Option<&Path>, dir: &Path) -> Vec<Need> {
        let spec = self.spec;
        let mut needs = vec![
            Need {
                name: "Source tree",
                ok: tree(spec, assets).is_some(),
                required: true,
                note: match tree(spec, assets) {
                    Some(root) => script(spec, &root).display().to_string(),
                    None => {
                        format!(
                            "{} is not next to the assets folder: run the app from its source tree",
                            spec.script
                        )
                    }
                },
            },
            uv_need(dir),
        ];
        if spec.msvc && cfg!(windows) {
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
            required: spec.gpu_required,
            note: g.unwrap_or_else(|| spec.without_gpu.to_string()),
        });
        needs
    }

    /// What the install is doing, as the crate's install command reports it.
    pub fn status(&self, dir: &Path) -> Value {
        if self.stand_in.load(Ordering::Relaxed) {
            return json!({ "state": "installed", "dir": dir.display().to_string() });
        }
        let log = dir.join(LOG);
        let mut out = json!({ "dir": dir.display().to_string(), "log": log.display().to_string() });
        let mut g = self.lock();
        let (state, p, error) = match g.as_mut() {
            Some(r) => {
                r.poll(self.spec);
                out["seconds"] = json!(r.since.elapsed().as_secs());
                let state = match (r.exit, r.stop_asked) {
                    _ if r.failed.is_some() => "failed",
                    (None, None) => "installing",
                    (None, Some(_)) => "stopping",
                    // finished: a crash while Python shuts down mustn't hide that
                    (Some(s), _) if s.success() || r.progress.done => {
                        if self.read(dir).is_ok() {
                            "installed"
                        } else {
                            "failed"
                        }
                    }
                    (Some(s), asked) if asked.is_some() || s.code() == Some(STOPPED) => "stopped",
                    (Some(_), _) => "failed",
                };
                let error = match state {
                    "failed" if r.failed.is_some() => r.failed.clone().unwrap_or_default(),
                    "failed" if r.progress.last.is_empty() => "the setup ended without saying why".into(),
                    "failed" => r.progress.last.clone(),
                    _ => String::new(),
                };
                (state, r.progress.clone(), error)
            }
            None if self.read(dir).is_ok() => ("installed", Progress::default(), String::new()),
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
    pub fn running(&self) -> bool {
        self.lock().as_mut().is_some_and(|r| {
            r.poll(self.spec);
            r.exit.is_none()
        })
    }

    /// Start the setup into `dir` (`repair`: even when it is installed already).
    pub fn start(&self, dir: &Path, assets: Option<&Path>, repair: bool) -> Result<(), String> {
        if self.stand_in.load(Ordering::Relaxed) {
            return Err("the fixtures' stand-in installs nothing".into());
        }
        let spec = self.spec;
        let mut g = self.lock();
        if let Some(r) = g.as_mut() {
            r.poll(spec);
            if r.exit.is_none() {
                return Err(match r.progress.steps {
                    0 => "the install is running already".into(),
                    n => format!("the install is running already (step {} of {n})", r.progress.step),
                });
            }
        }
        if !repair && self.read(dir).is_ok() {
            return Err(ac_core::error::hinted(
                format!("{} is installed already, in {}", spec.kind.what, dir.display()),
                "pass repair: true to run the setup again (it updates or repairs the install)",
            ));
        }
        let missing: Vec<String> = self
            .requirements(assets, dir)
            .into_iter()
            .filter(|n| n.required && !n.ok)
            .map(|n| format!("{}: {}", n.name, n.note))
            .collect();
        if !missing.is_empty() {
            return Err(format!("the install can't start yet. {}", missing.join("; ")));
        }
        let root = tree(spec, assets).ok_or("the source tree was not found")?;
        std::fs::create_dir_all(dir).map_err(|e| format!("can't make {}: {e}", dir.display()))?;
        let log = dir.join(LOG);
        std::fs::File::create(&log).map_err(|e| format!("can't write {}: {e}", log.display()))?;
        let (cmd, stage) = match uv(dir) {
            Some(uv) => (setup_command(spec, &uv, &root, dir)?, Stage::Setup),
            None => {
                append(&log, &uv_fetch::announce())?;
                (uv_fetch::download(dir)?, Stage::FetchUv)
            }
        };
        let (child, stdin) = launch(cmd, &log)?;
        *g = Some(Run {
            child,
            stage,
            dir: dir.to_path_buf(),
            root,
            failed: None,
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
    pub fn stop(&self) -> Result<(), String> {
        let mut g = self.lock();
        let r = g.as_mut().ok_or("no install is running")?;
        r.poll(self.spec);
        if r.exit.is_some() {
            return Err("no install is running".into());
        }
        if r.stop_asked.is_none() {
            r.stdin = None;
            r.stop_asked = Some(Instant::now());
            if r.stage != Stage::Setup {
                // a download or unpacking has nothing to tidy: it ends now, and Install starts it
                // again (a kill fails only when it has ended already)
                let _ = r.child.kill();
                append(
                    &r.log,
                    "== stopped: the app ended the install (run it again to carry on)\n",
                )?;
            }
        }
        Ok(())
    }
}
