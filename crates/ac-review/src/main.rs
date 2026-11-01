//! The training data review window, for developers (never shipped with the app).
//!
//! ```text
//! ac-review [--data <dir>] [--decisions <file>] [--assets <dir>] [--library <dir>] [--sets <dir>]
//!           [--set <name>] [--open <set>/<take>] [--frame <n>] [--view list|groups|gallery|styles] [--style <name>]
//!           [--screenshot <png>]
//! ac-review run <command> [<json>] [--data <dir>] [--decisions <file>] [--library <dir>]
//!           [--sets <dir>] [--set <name>]
//! ac-review unpack [<set> ...] [--data <dir>]
//! ```
//! `--data`: the retargeted sets and their scans (default `training/data/rig`); `--decisions`: the
//! review file (default `training/review/decisions.json`); both found from the checkout this runs
//! in. `--library`: a `.clip.json` library to review beside the sets (only when given); `--sets`: the
//! training sets' folder (default `training-sets` beside the app's library, outside any repository);
//! `--set`: the one to open. The window comes back to the library, the folder and the set it last
//! had when they aren't given (`remembered`); `run` uses only what it is given.
//! `run` does one review command headless and prints its JSON (`review.commands` lists them).
//! `unpack` makes the sets' rotation caches ahead of time (the window makes one when it first
//! shows a set's take). `--screenshot` draws the window once settled, writes the PNG and quits.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod scrub;
mod ui;
mod viewer;

use std::path::{Path, PathBuf};

use ac_review::remembered::{Launch, Remembered};
use ac_review::review::Review;

const USAGE: &str = "ac-review [--data <dir>] [--decisions <file>] [--assets <dir>] [--library <dir>] [--sets <dir>] [--set <name>] [--open <set>/<take>] [--frame <n>] [--view list|groups|gallery|styles] [--style <name>] [--screenshot <png>]\n\
                     ac-review run <command> [<json>] [--data <dir>] [--decisions <file>] [--library <dir>] [--sets <dir>] [--set <name>]\n\
                     ac-review unpack [<set> ...] [--data <dir>]";

#[derive(Debug, Default)]
pub struct Args {
    pub data: Option<PathBuf>,
    pub decisions: Option<PathBuf>,
    pub assets: Option<PathBuf>,
    pub library: Option<PathBuf>,
    pub sets: Option<PathBuf>,
    pub set: Option<String>,
    pub open: Option<String>,
    pub frame: Option<usize>,
    pub view: Option<String>,
    /// Open the Styles view on this style, playing it through.
    pub style: Option<String>,
    pub screenshot: Option<PathBuf>,
    /// `run` / `unpack` and their words.
    pub verb: Option<String>,
    pub words: Vec<String>,
    /// What the window couldn't come back to; it says so when it opens.
    pub notes: Vec<String>,
}

/// The folder a launch with no `--sets` and nothing remembered uses.
fn default_sets() -> PathBuf {
    ac_io::library::app_library_dir().with_file_name("training-sets")
}

/// The window's start: what the launch leaves out comes from where it last was. A set coming back
/// opens the Styles view, where a set is gone through.
fn come_back(a: &mut Args) {
    let was = match Remembered::load(&Remembered::file()) {
        Ok(was) => was,
        Err(e) => {
            a.notes.push(format!("where the window last was is lost: {e}"));
            return;
        }
    };
    let mut launch = Launch {
        library: a.library.take(),
        sets: a.sets.take(),
        set: a.set.take(),
    };
    let asked_for_a_set = launch.set.is_some();
    a.notes.extend(was.restore(&mut launch, &default_sets()));
    if !asked_for_a_set && launch.set.is_some() && a.view.is_none() && a.open.is_none() {
        a.view = Some("styles".into());
    }
    a.library = launch.library;
    a.sets = launch.sets;
    a.set = launch.set;
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(w) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{w} needs a value"));
        match w.as_str() {
            "--data" => a.data = Some(value()?.into()),
            "--decisions" => a.decisions = Some(value()?.into()),
            "--assets" => a.assets = Some(value()?.into()),
            "--library" => a.library = Some(value()?.into()),
            "--sets" => a.sets = Some(value()?.into()),
            "--set" => a.set = Some(value()?),
            "--open" => a.open = Some(value()?),
            "--frame" => a.frame = Some(value()?.parse().map_err(|e| format!("--frame: {e}"))?),
            "--view" => a.view = Some(value()?),
            "--style" => a.style = Some(value()?),
            "--screenshot" => a.screenshot = Some(value()?.into()),
            "--help" | "-h" => return Err(USAGE.into()),
            flag if flag.starts_with("--") => return Err(format!("unknown argument {flag:?}\n{USAGE}")),
            "run" | "unpack" if a.verb.is_none() => a.verb = Some(w),
            _ if a.verb.is_some() => a.words.push(w),
            other => return Err(format!("unknown argument {other:?}\n{USAGE}")),
        }
    }
    Ok(a)
}

/// The checkout this runs in: the folder above `assets/rig/default_rig.json`, from the current
/// folder or the executable's.
pub fn checkout() -> Option<PathBuf> {
    let mut starts = vec![std::env::current_dir().ok()?];
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        starts.push(dir);
    }
    starts.into_iter().find_map(|s| {
        s.ancestors()
            .find(|d| d.join("assets/rig/default_rig.json").is_file())
            .map(Path::to_path_buf)
    })
}

pub struct Paths {
    pub data: PathBuf,
    pub decisions: PathBuf,
    pub assets: PathBuf,
}

fn paths(a: &Args) -> Result<Paths, String> {
    let root = checkout();
    let under = |rel: &str| root.as_ref().map(|r| r.join(rel));
    let need =
        |p: Option<PathBuf>, what: &str| p.ok_or_else(|| format!("no checkout found here: pass {what}"));
    Ok(Paths {
        data: need(a.data.clone().or_else(|| under("training/data/rig")), "--data")?,
        decisions: need(
            a.decisions
                .clone()
                .or_else(|| under("training/review/decisions.json")),
            "--decisions",
        )?,
        assets: need(a.assets.clone().or_else(|| under("assets")), "--assets")?,
    })
}

/// The review both front ends use: the sets, the library if given, the training set if named.
pub fn open_review(a: &Args, p: &Paths) -> Result<Review, String> {
    let mut review = Review::open(&p.data, &p.decisions)?;
    let rig_path = p.assets.join("rig/default_rig.json");
    review.check_rig(&rig_path)?;
    if let Some(root) = &a.library {
        let text = std::fs::read_to_string(&rig_path).map_err(|e| format!("{}: {e}", rig_path.display()))?;
        let rig = ac_core::rig::Skeleton::from_json(&text).map_err(|e| e.to_string())?;
        review.add_library(root, &rig.names);
    }
    review.training.folder = Some(a.sets.clone().unwrap_or_else(default_sets));
    if let Some(name) = &a.set {
        review.open_set(Some(name))?;
    }
    Ok(review)
}

fn headless(a: &Args, p: &Paths) -> Result<(), String> {
    let mut review = open_review(a, p)?;
    match a.verb.as_deref() {
        Some("run") => {
            let cmd = a.words.first().ok_or("run <command> [<json>]")?;
            let params = match a.words.get(1) {
                Some(j) => serde_json::from_str(j).map_err(|e| format!("params: {e}"))?,
                None => serde_json::Map::new(),
            };
            let out = review.run(cmd, &params)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        Some("unpack") => {
            for s in &review.sets {
                if !a.words.is_empty() && !a.words.contains(&s.name) {
                    continue;
                }
                if s.unpacked() {
                    println!("{}: unpacked already", s.name);
                    continue;
                }
                println!("{}: unpacking {} frames...", s.name, s.frames);
                s.unpack_job().run()?;
            }
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}

fn main() -> std::process::ExitCode {
    let mut args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return std::process::ExitCode::from(2);
        }
    };
    let p = match paths(&args) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return std::process::ExitCode::from(2);
        }
    };
    if args.verb.is_some() {
        return match headless(&args, &p) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                std::process::ExitCode::from(1)
            }
        };
    }
    come_back(&mut args);
    let title = format!("{} data review", ac_core::product::NAME);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(&title)
            .with_inner_size([1500.0, 920.0]),
        renderer: eframe::Renderer::Wgpu,
        depth_buffer: 0,
        ..Default::default()
    };
    let result = eframe::run_native(
        &title,
        options,
        Box::new(move |cc| Ok(Box::new(ui::App::new(cc, args, p)?))),
    );
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{title} could not start: {e}");
            std::process::ExitCode::from(3)
        }
    }
}
