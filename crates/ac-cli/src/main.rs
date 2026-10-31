//! `ac`: the app without a window.
//!
//! ```text
//! ac commands [<id> | --md [<file>]] every command (id, label, shortcut, undoable, tool, inputs
//!                                    with kinds and ranges, outputs); <id>: one command's whole
//!                                    definition; --md writes docs/commands.md
//! ac new-command <area.verb> [--crate core|io] [--label <text>]
//!                                    a command stub file, a fixture skeleton, and its registration
//! ac run <command> [<json params>]   run one command on a fresh session, print the result
//! ac script <file.json>              run a list of {command, params}; print each result
//! ac fixtures [--only <name>] [--shots <dir>] [--print] [--bless] [dir]
//!                                    check the feature fixtures (exit 1 on any failure);
//!                                    --shots writes each fixture's views as PNGs and an
//!                                    index.html (description, failures, pictures);
//!                                    --print prints what each checked step returned (JSONL);
//!                                    --bless writes those values into the fixture's `expect`
//!                                    (only keys it already names; descriptions stay yours)
//!                                    --golden-check compares each view with tests/visual/
//!                                    (software adapter, pixel tolerance); --bless-visual writes them;
//!                                    with --shots the gallery shows golden, actual and diff
//!                                    --timings: each fixture and step by time, slowest first
//!                                    --watch: run again when a fixture file changes
//!                                    --schema: write fixtures/schema.json
//! ac render [--script <file>] [--frame <n> | --frames <a,b,..> [--strip]] [--view <v>]
//!           [--size <w>x<h>] [--no-floor] [--background <#rrggbb|transparent>]
//!           [--camera <azimuth,elevation,distance[,x,y,z]>] --out <png>
//!                                    --frames without --strip writes <out>-<frame>.png
//! ac video [--script <file>] [--from <f>] [--to <f>] [--fps <n>] [--view <v>] [--size <w>x<h>]
//!          [--fixed] [--no-floor] [--background <#rrggbb>]
//!          [--camera <azimuth,elevation,distance[,x,y,z]>]
//!          --out <mp4|gif>           play the animation into a video (MP4 through ffmpeg)
//! ac samples [--only <name>] [--bless]
//!                                    remake each sample from assets/samples/recipes and check it
//!                                    matches the shipped assets/samples/<name>.clip.json (--bless
//!                                    writes them)
//! ac mcp [--live <port> [--timeout <s>]]
//!                                    MCP over stdio (default library: the app's); --live drives
//!                                    the running app (started with --control <port>); --timeout:
//!                                    seconds one request to the app may take (1 to 3600; default
//!                                    AC_CONTROL_TIMEOUT, else 120)
//! ```
//! Common flags: `--assets <dir>`, `--library <dir>`, `--jsonl` (one compact JSON result a line).
//! Without `--library`, `run`, `script`, `render` and `video` use a fresh scratch copy of the
//! built-in library (the same as fixtures), named on stderr. `AC_LOG=debug` (any `tracing`
//! filter) logs each command run with its time to stderr.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Value, json};

struct Flags {
    assets: PathBuf,
    library: PathBuf,
    /// `--library` was given.
    library_set: bool,
    jsonl: bool,
    rest: Vec<String>,
}

fn flags() -> Result<Flags, String> {
    let mut assets = None;
    let mut library = None;
    let mut jsonl = false;
    let mut rest = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--assets" => assets = Some(PathBuf::from(it.next().ok_or("--assets needs a dir")?)),
            "--library" => library = Some(PathBuf::from(it.next().ok_or("--library needs a dir")?)),
            "--jsonl" => jsonl = true,
            _ => rest.push(a),
        }
    }
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    // Then beside the executable, as in an installed release (`ac.exe` next to `assets/`).
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let assets = assets
        .or_else(|| ac_cli::find_assets(&cwd))
        .or_else(|| exe_dir.and_then(|d| ac_cli::find_assets(&d)))
        .ok_or("no assets folder found (pass --assets <dir>)")?;
    let library_set = library.is_some();
    let library = library.unwrap_or_else(|| {
        std::env::temp_dir().join(format!("{}-cli-{}", ac_core::product::ID, std::process::id()))
    });
    Ok(Flags {
        assets,
        library,
        library_set,
        jsonl,
        rest,
    })
}

/// A session on `--library`, or on a fresh scratch copy of the built-ins (removed on drop).
struct Scratch {
    session: ac_core::Session,
    remove: Option<PathBuf>,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if let Some(p) = &self.remove {
            let _ = std::fs::remove_dir_all(p);
        }
    }
}

fn session(f: &Flags) -> Result<Scratch, String> {
    if f.library_set {
        eprintln!("library: {}", f.library.display());
    } else {
        let _ = std::fs::remove_dir_all(&f.library);
        eprintln!("library: a scratch copy of the built-ins (pass --library <dir> for another)");
    }
    Ok(Scratch {
        session: ac_cli::session(&f.assets, &f.library)?,
        remove: (!f.library_set).then(|| f.library.clone()),
    })
}

/// Run a script file's steps (a {command, params} list, or an object with `script`).
fn run_script(s: &mut ac_core::Session, file: &str, echo: Option<bool>) -> Result<(), String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
    let steps: Value = serde_json::from_str(&text).map_err(|e| format!("{file}: {e}"))?;
    let steps = steps
        .get("script")
        .unwrap_or(&steps)
        .as_array()
        .cloned()
        .ok_or("a script is a list of {command, params}")?;
    for step in steps {
        let id = step["command"].as_str().unwrap_or_default();
        let v = s
            .run(id, &step.get("params").cloned().unwrap_or(json!({})))
            .map_err(|e| format!("{id}: {e}"))?;
        if let Some(jsonl) = echo {
            print(&json!({ "command": id, "result": v }), jsonl);
        }
    }
    Ok(())
}

fn size(t: &str) -> Result<(u32, u32), String> {
    let (w, h) = t.split_once('x').ok_or("--size is <w>x<h>")?;
    Ok((
        w.parse().map_err(|e| format!("--size: {e}"))?,
        h.parse().map_err(|e| format!("--size: {e}"))?,
    ))
}

fn print(v: &Value, jsonl: bool) {
    let text = if jsonl {
        serde_json::to_string(v)
    } else {
        serde_json::to_string_pretty(v)
    };
    println!("{}", text.unwrap_or_default());
}

fn frame_list(t: &str) -> Result<Vec<f64>, String> {
    t.split(',')
        .map(|x| {
            x.trim()
                .parse::<f64>()
                .map_err(|e| format!("--frames {x:?}: {e}"))
        })
        .collect()
}

fn real_main() -> Result<ExitCode, String> {
    let f = flags()?;
    let mut args = f.rest.iter().map(String::as_str);
    match args.next() {
        Some("commands") => {
            let s = session(&f)?;
            let rest: Vec<&str> = args.collect();
            print(&ac_cli::cmdref::cli(s.session.commands(), &rest, &ac_cli::repo_of(&f.assets))?, f.jsonl);
            Ok(ExitCode::SUCCESS)
        }
        Some("new-command") => {
            let id = args.next().ok_or("new-command <area.verb> [--crate core|io] [--label <text>]")?;
            let (mut owner, mut label) = (None, None);
            while let Some(a) = args.next() {
                match a {
                    "--crate" => owner = Some(ac_cli::scaffold::Owner::parse(args.next().ok_or("--crate needs core or io")?)?),
                    "--label" => label = Some(args.next().ok_or("--label needs a text")?),
                    other => return Err(format!("new-command: unknown argument {other:?}")),
                }
            }
            let s = session(&f)?;
            let out = ac_cli::scaffold::new_command(&ac_cli::repo_of(&f.assets), id, owner, label, s.session.commands())?;
            print(&out, f.jsonl);
            Ok(ExitCode::SUCCESS)
        }
        Some("run") => {
            let id = args.next().ok_or("run <command> [<json params>]")?;
            let params: Value = match args.next() {
                Some(t) => serde_json::from_str(t).map_err(|e| format!("params: {e}"))?,
                None => json!({}),
            };
            let mut s = session(&f)?;
            let v = s.session.run(id, &params).map_err(|e| format!("{id}: {e}"))?;
            print(&v, f.jsonl);
            Ok(ExitCode::SUCCESS)
        }
        Some("script") => {
            let file = args.next().ok_or("script <file.json>")?;
            let mut s = session(&f)?;
            run_script(&mut s.session, file, Some(f.jsonl))?;
            Ok(ExitCode::SUCCESS)
        }
        Some("render") => {
            let (mut script, mut frames, mut view, mut wh, mut out, mut strip) = (None, None, "three-quarter", (800, 600), None, false);
            let (mut no_floor, mut background, mut camera) = (false, None, None);
            while let Some(a) = args.next() {
                let mut v = || args.next().ok_or(format!("{a} needs a value"));
                match a {
                    "--script" => script = Some(v()?),
                    "--frame" => frames = Some(vec![v()?.parse::<f64>().map_err(|e| format!("--frame: {e}"))?]),
                    "--frames" => frames = Some(frame_list(v()?)?),
                    "--strip" => strip = true,
                    "--view" => view = v()?,
                    "--size" => wh = size(v()?)?,
                    "--no-floor" => no_floor = true,
                    "--background" => background = Some(v()?),
                    "--camera" => camera = Some(v()?),
                    "--out" => out = Some(v()?),
                    other => return Err(format!("render: unknown argument {other:?}")),
                }
            }
            let out = PathBuf::from(out.ok_or("render needs --out <png>")?);
            let look = ac_cli::render::look_flags(no_floor, background, camera)?;
            let mut s = session(&f)?;
            if let Some(file) = script {
                run_script(&mut s.session, file, None)?;
            }
            let mut pics = ac_cli::render::Pictures::new(&f.assets, &s.session.document.skeleton.names)?;
            pics.set_look(look);
            let frames = frames.unwrap_or_default();
            if frames.is_empty() {
                std::fs::write(&out, pics.png(&s.session.document, view, wh.0, wh.1)?).map_err(|e| format!("{}: {e}", out.display()))?;
                println!("{}", out.display());
                return Ok(ExitCode::SUCCESS);
            }
            // several frames share one camera that sees them all, so heights and travel compare
            let framing = if frames.len() > 1 {
                Some(ac_cli::render::framing_of(&s.session.document, &frames, f64::from(wh.0) / f64::from(wh.1))?)
            } else {
                None
            };
            let mut shots = Vec::new();
            for fr in &frames {
                s.session.run("frame.set", &json!({ "frame": fr })).map_err(|e| e.to_string())?;
                shots.push(pics.rgba(&s.session.document, view, wh.0, wh.1, framing)?);
            }
            if strip || frames.len() == 1 {
                let (w, rgba) = ac_cli::render::strip(wh.0, wh.1, &shots);
                std::fs::write(&out, ac_render::offscreen::png(w, wh.1, &rgba)?).map_err(|e| format!("{}: {e}", out.display()))?;
                println!("{}", out.display());
            } else {
                let stem = out.with_extension("");
                for (fr, rgba) in frames.iter().zip(&shots) {
                    let p = PathBuf::from(format!("{}-{fr}.png", stem.display()));
                    std::fs::write(&p, ac_render::offscreen::png(wh.0, wh.1, rgba)?).map_err(|e| format!("{}: {e}", p.display()))?;
                    println!("{}", p.display());
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Some("strips") => {
            // maintainers: preview strips for the Official collection (services/community)
            let usage = "strips <library dir> <ids file> --out <dir>";
            let (root, list) = (args.next().ok_or(usage)?, args.next().ok_or(usage)?);
            let mut out = None;
            while let Some(a) = args.next() {
                match a {
                    "--out" => out = Some(PathBuf::from(args.next().ok_or("--out needs a value")?)),
                    other => return Err(format!("strips: unknown argument {other:?}")),
                }
            }
            let out = out.ok_or(usage)?;
            let rig = std::fs::read_to_string(f.assets.join("rig/default_rig.json")).map_err(|e| format!("the rig: {e}"))?;
            let sk = ac_core::rig::Skeleton::from_json(&rig).map_err(|e| e.to_string())?;
            let lib = ac_io::library::Library::new(std::path::Path::new(root), ac_io::library::Kind::Animations);
            let mut pics = ac_cli::render::Pictures::new(&f.assets, &sk.names)?;
            let ids = std::fs::read_to_string(list).map_err(|e| format!("{list}: {e}"))?;
            for id in ids.lines().map(str::trim).filter(|l| !l.is_empty()) {
                let png = ac_cli::render::clip_strip(&mut pics, &sk, &lib.read(id)?, &lib)?;
                let path = out.join(format!("{id}.png"));
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                }
                std::fs::write(&path, png).map_err(|e| format!("{}: {e}", path.display()))?;
                println!("{}", path.display());
            }
            Ok(ExitCode::SUCCESS)
        }
        Some("video") => {
            let (mut no_floor, mut background, mut camera) = (false, None, None);
            let (mut script, mut from, mut to, mut fps, mut view, mut wh, mut out, mut fixed) = (None, None, None, None, "side", (960, 540), None, false);
            while let Some(a) = args.next() {
                let mut v = || args.next().ok_or(format!("{a} needs a value"));
                let num = |t: &str| t.parse::<f64>().map_err(|e| format!("{a}: {e}"));
                match a {
                    "--script" => script = Some(v()?),
                    "--from" => from = Some(num(v()?)?),
                    "--to" => to = Some(num(v()?)?),
                    "--fps" => fps = Some(num(v()?)?),
                    "--view" => view = v()?,
                    "--size" => wh = size(v()?)?,
                    "--fixed" => fixed = true,
                    "--no-floor" => no_floor = true,
                    "--background" => background = Some(v()?),
                    "--camera" => camera = Some(v()?),
                    "--out" => out = Some(v()?),
                    other => return Err(format!("video: unknown argument {other:?}")),
                }
            }
            let out = PathBuf::from(out.ok_or("video needs --out <mp4|gif>")?);
            let look = ac_cli::render::look_flags(no_floor, background, camera)?;
            ac_media::no_transparent(
                &look,
                match ac_media::Format::of(&out)? {
                    ac_media::Format::Mp4 => "an MP4",
                    ac_media::Format::Gif => "a GIF",
                },
            )?;
            let mut s = session(&f)?;
            if let Some(file) = script {
                run_script(&mut s.session, file, None)?;
            }
            let doc = &s.session.document;
            ac_core::evaluation::check(doc, "a video of it")?;
            let fps = fps.unwrap_or(doc.clip.fps);
            let last = (doc.clip.length_with(&doc.links) - 1).max(0) as f64;
            let (from, to) = (from.unwrap_or(0.0), to.unwrap_or(last));
            // the window's Export preview makes the same file (ac-media); --fixed: one camera
            // that sees the whole shot (travel and height show), else it follows the body
            let times = ac_media::shots::times(doc.clip.fps, from, to, fps).map_err(|e| format!("video: {e}"))?;
            let (w, h) = match ac_media::Format::of(&out)? {
                ac_media::Format::Mp4 => (wh.0 & !1, wh.1 & !1), // x264 wants even sizes
                ac_media::Format::Gif => wh,
            };
            let mut shots = ac_media::Shots::new(&f.assets, &doc.skeleton.names)?;
            shots.set_look(look);
            let enc = ac_media::Encoder::create(&out, w, h, fps)?;
            let n = shots.video(doc, &times, view, (w, h), fixed, enc)?;
            let out = out.display();
            println!("{out} ({n} frames at {fps} fps)");
            Ok(ExitCode::SUCCESS)
        }
        Some("samples") => {
            let (mut only, mut bless) = (None, false);
            while let Some(a) = args.next() {
                match a {
                    "--only" => only = Some(args.next().ok_or("--only needs a name")?),
                    "--bless" => bless = true,
                    other => return Err(format!("samples: unknown argument {other:?}")),
                }
            }
            let recipes: Vec<Value> = ac_cli::sample_recipes(&f.assets)?
                .into_iter()
                .filter(|r| only.is_none_or(|o| r["name"] == o))
                .collect();
            if recipes.is_empty() {
                return Err("no sample recipes match".into());
            }
            let dir = f.assets.join(ac_io::import::SAMPLES);
            let made = ac_cli::par_map(&recipes, |r| {
                let name = r["name"].as_str().unwrap_or_default();
                let scratch = std::env::temp_dir().join(format!("ac-sample-{}-{name}", std::process::id()));
                (name.to_string(), ac_cli::make_sample(&f.assets, r, &scratch))
            });
            let mut differ = 0;
            for (name, text) in made {
                let text = text?;
                let path = dir.join(format!("{name}.clip.json"));
                let shipped = std::fs::read_to_string(&path).unwrap_or_default();
                let clip: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
                let keys = clip["keys"].as_array().map_or(&[][..], Vec::as_slice);
                let ai = keys.iter().filter(|k| k["source"] == "ai").count();
                let what = format!("{} keys ({} yours, {ai} AI)", keys.len(), keys.len() - ai);
                if shipped == text {
                    println!("ok    {name}: {what}");
                } else if bless {
                    std::fs::write(&path, &text).map_err(|e| format!("{}: {e}", path.display()))?;
                    println!("wrote {name}: {what}");
                } else {
                    differ += 1;
                    println!("DIFF  {name}: {} differs from its recipe (ac samples --bless rewrites it)", path.display());
                }
            }
            Ok(if differ > 0 { ExitCode::FAILURE } else { ExitCode::SUCCESS })
        }
        Some("mcp") => {
            let mut live = None;
            let mut timeout = None;
            while let Some(a) = args.next() {
                match a {
                    "--live" => live = Some(args.next().ok_or("--live needs a port")?.parse::<u16>().map_err(|e| format!("--live: {e}"))?),
                    "--timeout" => timeout = Some(ac_cli::mcp::parse_timeout(args.next().ok_or("--timeout needs seconds")?)?),
                    other => return Err(format!("mcp: unknown argument {other:?}")),
                }
            }
            if let Some(port) = live {
                ac_cli::mcp::serve_live(&f.assets, port, timeout)?;
                return Ok(ExitCode::SUCCESS);
            }
            if timeout.is_some() {
                return Err("mcp: --timeout is for --live (a headless server waits on nothing)".into());
            }
            let library = if f.library_set { f.library.clone() } else { ac_io::library::app_library_dir() };
            let s = ac_cli::session(&f.assets, &library)?;
            ac_cli::mcp::Server::new(s, f.assets.clone()).serve()?;
            Ok(ExitCode::SUCCESS)
        }
        Some("fixtures") => {
            let rest: Vec<&str> = args.collect();
            let ok = ac_cli::fixtures_cmd::run(&f.assets, &rest)?;
            Ok(if ok { ExitCode::SUCCESS } else { ExitCode::from(1) })
        }
        _ => Err("usage: ac commands [<id> | --md] | new-command <area.verb> | run <command> [<json>] | script <file> | fixtures [--only <name>] [--shots <dir>] [--print] [--bless] [--golden-check] [--bless-visual] [--timings] [--watch] [--schema] [dir] | render ... --out <png> | strips <library dir> <ids file> --out <dir> | video ... --out <mp4> | samples [--only <name>] [--bless] | mcp [--live <port> [--timeout <s>]]".into()),
    }
}

fn main() -> ExitCode {
    ac_io::setup::init_logging();
    // the live community server (fixture runs put the fake in its place), with the account beside
    // the app's own library
    if let Some(dir) = ac_io::library::app_library_dir().parent() {
        ac_community::http::connect(dir.to_path_buf());
    }
    match real_main() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(2)
        }
    }
}
