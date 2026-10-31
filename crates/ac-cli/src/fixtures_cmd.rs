//! `ac fixtures`: check the feature fixtures.
//!
//! ```text
//! ac fixtures [--only <name>] [--shots <dir>] [--print] [--bless] [--golden-check]
//!             [--bless-visual] [--timings] [--watch] [--schema] [dir]
//! ```
//! `--timings` lists each fixture and each step by time, slowest first; `--watch` runs again
//! whenever a fixture file changes (the one named by `--only`, else the files that changed);
//! `--schema` writes `fixtures/schema.json` from the fixture types and the registered commands.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::fixture_spec;
use crate::gallery::{self, Item};

#[derive(Default)]
struct Options {
    only: Option<String>,
    dir: Option<PathBuf>,
    shots: Option<PathBuf>,
    print: bool,
    bless: bool,
    golden: bool,
    bless_visual: bool,
    timings: bool,
    watch: bool,
    schema: bool,
}

fn options(args: &[&str]) -> Result<Options, String> {
    let mut o = Options::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .map(|v| v.to_string())
                .ok_or(format!("{a} needs a value"))
        };
        match *a {
            "--only" => o.only = Some(value()?),
            "--shots" => o.shots = Some(PathBuf::from(value()?)),
            "--print" => o.print = true,
            "--bless" => o.bless = true,
            "--golden-check" => o.golden = true,
            "--bless-visual" => (o.golden, o.bless_visual) = (true, true),
            "--timings" => o.timings = true,
            "--watch" => o.watch = true,
            "--schema" => o.schema = true,
            flag if flag.starts_with("--") => return Err(format!("fixtures: unknown flag {flag:?}")),
            d => o.dir = Some(PathBuf::from(d)),
        }
    }
    Ok(o)
}

/// Every command id, from a session without a library (nothing is installed).
fn registry(assets: &Path) -> Result<Vec<&'static ac_core::CommandSpec>, String> {
    let built = ac_io::setup::SessionBuilder::new(assets)
        .commands(ac_media::SPECS)
        .commands(ac_textmotion::SPECS)
        .commands(ac_posefill::SPECS)
        .commands(ac_posefill::live_commands::SPECS)
        .commands(ac_community::SPECS)
        .build()?;
    Ok(built.session.commands().to_vec())
}

/// `fixtures/schema.json` as it should read for these commands.
pub fn schema_text(assets: &Path) -> Result<String, String> {
    let commands = registry(assets)?;
    Ok(serde_json::to_string_pretty(&fixture_spec::schema(&commands)).map_err(|e| e.to_string())? + "\n")
}

/// The fixtures `--only` selects (all without it). An unknown name is an error that names the
/// closest ones.
fn select(all: Vec<(PathBuf, Value)>, only: Option<&str>) -> Result<Vec<(PathBuf, Value)>, String> {
    let Some(o) = only else { return Ok(all) };
    let (hit, rest): (Vec<_>, Vec<_>) = all
        .into_iter()
        .partition(|(_, fx)| fx["name"].as_str() == Some(o));
    if !hit.is_empty() {
        return Ok(hit);
    }
    let names: Vec<&str> = rest.iter().filter_map(|(_, fx)| fx["name"].as_str()).collect();
    let near = ac_core::suggest::closest(o, names.iter().copied());
    Err(if near.is_empty() {
        format!("no fixture named {o:?} (`ac fixtures` runs them all; names are in each file's \"name\")")
    } else {
        format!("no fixture named {o:?}; did you mean {}?", near.join(", "))
    })
}

/// Run `fixtures` once and print what happened. Returns (ran, failed).
fn run_set(assets: &Path, fixtures: &[(PathBuf, Value)], o: &Options) -> Result<(usize, usize), String> {
    let repo = crate::repo_of(assets);
    let mut failed = 0;
    let mut items = Vec::new();
    let mut times: Vec<(f64, String)> = Vec::new();
    let mut steps: Vec<(f64, String)> = Vec::new();
    for (path, fx) in fixtures {
        let started = Instant::now();
        let mut out = crate::run_fixture(assets, fx);
        times.push((started.elapsed().as_secs_f64() * 1e3, out.name.clone()));
        steps.extend(
            out.steps_ms
                .iter()
                .map(|(i, cmd, ms)| (*ms, format!("{} step {i} {cmd}", out.name))),
        );
        let mut goldens = Vec::new();
        if o.golden {
            match crate::golden_compare(
                assets,
                fx,
                &repo.join("tests").join("visual"),
                &repo.join("target").join("visual-diff"),
                o.bless_visual,
                o.shots.as_deref(),
            ) {
                Ok((fails, kept)) => {
                    out.failures.extend(fails);
                    goldens = kept;
                }
                Err(e) => out.failures.push(format!("golden pictures: {e}")),
            }
        }
        let mut views = Vec::new();
        if let Some(dir) = &o.shots {
            match crate::fixture_views(assets, fx, dir) {
                Ok(files) => {
                    files.iter().for_each(|p| println!("      view {}", p.display()));
                    views = files;
                }
                Err(e) => println!("      views: {e}"),
            }
        }
        if o.print {
            for (i, cmd, v) in &out.actual {
                println!(
                    "{}",
                    json!({ "fixture": out.name, "step": i, "command": cmd, "result": v })
                );
            }
        }
        if o.bless {
            let n = crate::bless_file(path, fx, &out)?;
            println!(
                "      blessed {n} step(s) in {}: check them against the description",
                path.display()
            );
        }
        if out.failures.is_empty() {
            println!("ok    {}", out.name);
        } else {
            failed += 1;
            println!("FAIL  {} ({})", out.name, path.display());
            for m in &out.failures {
                println!("      {m}");
            }
        }
        items.push(Item {
            fixture: fx.clone(),
            outcome: out,
            views,
            goldens,
        });
    }
    if let Some(dir) = &o.shots {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let index = dir.join("index.html");
        std::fs::write(&index, gallery::page(&items)).map_err(|e| format!("{}: {e}", index.display()))?;
        println!("      gallery {}", index.display());
    }
    if o.timings {
        print_slowest("fixtures", times, usize::MAX);
        print_slowest("steps", steps, 20);
    }
    println!("{} fixtures, {failed} failed", fixtures.len());
    Ok((fixtures.len(), failed))
}

fn print_slowest(what: &str, mut list: Vec<(f64, String)>, most: usize) {
    list.sort_by(|a, b| b.0.total_cmp(&a.0));
    let total: f64 = list.iter().map(|t| t.0).sum();
    println!("timings, {what} (slowest first, {total:.0} ms in all):");
    for (ms, name) in list.iter().take(most) {
        println!("  {ms:>9.1} ms  {name}");
    }
}

/// `ac fixtures ...`; `Ok(true)`: every fixture that ran passed (and at least one ran).
pub fn run(assets: &Path, args: &[&str]) -> Result<bool, String> {
    let o = options(args)?;
    let dir = o
        .dir
        .clone()
        .unwrap_or_else(|| crate::repo_of(assets).join("fixtures"));
    if o.schema {
        let path = dir.join("schema.json");
        std::fs::write(&path, schema_text(assets)?).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("wrote {}", path.display());
        return Ok(true);
    }
    let selected = select(crate::load_fixtures(&dir)?, o.only.as_deref())?;
    let (ran, failed) = run_set(assets, &selected, &o)?;
    if o.watch {
        watch(assets, &dir, &o)?;
    }
    Ok(failed == 0 && ran > 0)
}

/// Run again on every change under `dir` until stopped (Ctrl+C).
fn watch(assets: &Path, dir: &Path, o: &Options) -> Result<(), String> {
    use notify::Watcher as _;
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |ev: notify::Result<notify::Event>| {
        // a send fails only once the loop below has ended
        if let Err(e) = tx.send(ev) {
            tracing::debug!("watch stopped: {e}");
        }
    })
    .map_err(|e| format!("can't watch {}: {e}", dir.display()))?;
    watcher
        .watch(dir, notify::RecursiveMode::NonRecursive)
        .map_err(|e| format!("can't watch {}: {e}", dir.display()))?;
    println!("watching {} (Ctrl+C stops)", dir.display());
    loop {
        let first = rx.recv().map_err(|e| format!("watch: {e}"))?;
        let mut events = vec![first];
        // editors write in bursts: wait for a quiet moment
        while let Ok(ev) = rx.recv_timeout(Duration::from_millis(250)) {
            events.push(ev);
        }
        let mut changed: Vec<PathBuf> = Vec::new();
        for ev in events {
            match ev {
                Ok(ev) if !ev.kind.is_access() => changed.extend(ev.paths),
                Ok(_) => {}
                Err(e) => println!("watch: {e}"),
            }
        }
        changed.retain(|p| {
            p.extension().is_some_and(|x| x == "json") && p.file_name().is_some_and(|n| n != "schema.json")
        });
        changed.sort();
        changed.dedup();
        if changed.is_empty() {
            continue;
        }
        let all = match crate::load_fixtures(dir) {
            Ok(all) => all,
            Err(e) => {
                println!("{e}");
                continue;
            }
        };
        let picked = match &o.only {
            Some(_) => select(all, o.only.as_deref()),
            None => Ok(all
                .into_iter()
                .filter(|(p, _)| changed.iter().any(|c| c.file_name() == p.file_name()))
                .collect()),
        };
        match picked {
            Ok(set) if !set.is_empty() => {
                println!("--- changed: {}", names(&changed));
                if let Err(e) = run_set(assets, &set, o) {
                    println!("{e}");
                }
            }
            Ok(_) => {}
            Err(e) => println!("{e}"),
        }
    }
}

fn names(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect::<Vec<_>>()
        .join(", ")
}
