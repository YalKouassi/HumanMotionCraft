//! The desktop app. Every edit the UI makes goes through `Session::run`, the same
//! commands the CLI and MCP drive.
//!
//! ```text
//! <bin> [--assets <dir>] [--library <dir>] [--open <id>] [--frame <n>] [--script <json>]
//!                [--screenshot <png>] [--record <mp4|gif> [--from <f>] [--to <f>] [--fps <n>]]
//!                [--size <w>x<h>] [--control <port>] [--prefs <dir>|memory] [--ui <name>=<value> ...]
//! ```
//! `--record` steps through the animation, writes the whole window to the file and closes
//! (`app/record.rs`). `--size` is the window's size in pixels; an MP4's is rounded down to even.
//! Either exits non-zero if the file couldn't be written, as `--screenshot` does.
//! `--ui` sets the window's own state (for screenshots): `tab=animations|builtin|poses|community`,
//! `source=all|mine|builtin`, `sort=name|recent|modified|length`, `search=<text>`, `dope=on|off`, `overlay=on|off|perf`, `ghost=on|off`,
//! `character=off|beside|over` (your own character, once `retarget.load` put one in the document), `playing=on|off`,
//! `tool=drag|reach|move|rotate`, `camera=<view>` (front, back, side, left, top, bottom,
//! three-quarter: looking at the body), `zoom=<metres>`, `ortho=on|off`, `follow=on|off`,
//! `onion=on|off|keys|frames`, `onion_count=1..5`, `onion_step=<frames>`, `trails=on|off`,
//! `trail_span=<frames each side, 0 all>`, `picker=on|off`, `handles=body|all`,
//! `gizmo=world|local`, `select=<bone>[,<bone>...]|none` (see `app/view_state.rs`),
//! `dialog=export|preview|block|welcome|tips|recover|none` (`app/files`), and
//! the shell's: `window=palette|settings|history|messages|shortcuts|about|form:<command>`,
//! `palette=<search>`, `theme=dark|light`, `scale=<0.5..3>`, `colours=standard|safe`,
//! `units=frames|seconds|timecode`, `maximise=on|off`, `ui_mode=simple|advanced`, `reset_layout=on`, `library|inspector|timeline=on|off`,
//! `side=left|right`, `perf=on|off`, `menu|tools|status|joints=on|off` (the menu bar, the viewport's
//! tool row, the status bar and the joint dots); the inspector panels': `joint=<bone>`, `box=<n>`,
//! `layer=<n>`, `travel=preview|off`, `prompt=<text>` (motion from text), `export_dir=<folder>` (where Export starts), `range_pair=<pair>`,
//! `range_side=L|R|Both` (the Range panel); the Community tab's: `community=<clip id>|mine|none`
//! (its popover, or your uploads), `share=<animation id>` (the Share to community window),
//! `community_window=on|off` (the community in its own window), `community_view=gallery|list`,
//! `community_card=<100..320>` (the gallery's card width).
//! `--control <port>` opens the control port an agent drives the window through (`control.rs`;
//! `ac mcp --live <port>` makes MCP tools of it).
//! `--prefs` says where the saved preferences live; without it they are the app's config folder,
//! except with `--screenshot`, `--record` or `--control`, which keep them in memory (`app/prefs.rs`).

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod app;
mod control;

use std::path::PathBuf;
use std::sync::OnceLock;

/// The largest window `--size` asks for.
const MAX_SIZE: u32 = 7680;

#[derive(Debug, Default, Clone)]
pub struct Args {
    pub assets: Option<PathBuf>,
    pub library: Option<PathBuf>,
    pub open: Option<String>,
    pub frame: Option<f64>,
    pub screenshot: Option<PathBuf>,
    pub record: Option<app::record::Plan>,
    /// The window's size in pixels.
    pub size: Option<(u32, u32)>,
    /// Commands run at startup: a JSON list of {command, params}.
    pub script: Option<PathBuf>,
    /// The control port (localhost) for agents.
    pub control: Option<u16>,
    /// Window state to set at startup: (name, value).
    pub ui: Vec<(String, String)>,
    /// Where the preferences live: a folder, or `memory` (`app/prefs.rs`).
    pub prefs: Option<String>,
}

/// Why the run failed, for its exit code: a shot or a recording that couldn't be written.
static FAILED: OnceLock<String> = OnceLock::new();

/// The run has failed: it exits non-zero and says why. The first reason is kept.
pub fn failed(why: String) {
    let _ = FAILED.set(why);
}

fn parse_size(v: &str) -> Result<(u32, u32), String> {
    let wrong = || format!("--size {v:?}: <width>x<height>, each 16 to {MAX_SIZE}");
    let (w, h) = v.split_once('x').ok_or_else(wrong)?;
    let (w, h): (u32, u32) = (w.parse().map_err(|_| wrong())?, h.parse().map_err(|_| wrong())?);
    if !(16..=MAX_SIZE).contains(&w) || !(16..=MAX_SIZE).contains(&h) {
        return Err(wrong());
    }
    Ok((w, h))
}

fn parse_args() -> Result<Args, String> {
    parse(std::env::args().skip(1))
}

fn parse(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut a = Args::default();
    let (mut record, mut from, mut to, mut fps) = (None, None, None, None);
    let mut it = args;
    while let Some(flag) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--assets" => a.assets = Some(value()?.into()),
            "--library" => a.library = Some(value()?.into()),
            "--open" => a.open = Some(value()?),
            "--frame" => a.frame = Some(value()?.parse().map_err(|e| format!("--frame: {e}"))?),
            "--screenshot" => a.screenshot = Some(value()?.into()),
            "--record" => record = Some(PathBuf::from(value()?)),
            "--from" => from = Some(value()?.parse::<f64>().map_err(|e| format!("--from: {e}"))?),
            "--to" => to = Some(value()?.parse::<f64>().map_err(|e| format!("--to: {e}"))?),
            "--fps" => fps = Some(value()?.parse::<f64>().map_err(|e| format!("--fps: {e}"))?),
            "--size" => a.size = Some(parse_size(&value()?)?),
            "--script" => a.script = Some(value()?.into()),
            "--control" => a.control = Some(value()?.parse().map_err(|e| format!("--control: {e}"))?),
            "--prefs" => a.prefs = Some(value()?),
            "--ui" => {
                let v = value()?;
                let (k, x) = v
                    .split_once('=')
                    .ok_or_else(|| format!("--ui {v:?}: name=value"))?;
                a.ui.push((k.to_string(), x.to_string()));
            }
            "--help" | "-h" => {
                return Err(concat!(env!("CARGO_BIN_NAME"), " [--assets <dir>] [--library <dir>] [--open <id>] [--frame <n>] [--script <json>] [--screenshot <png>] [--record <mp4|gif> [--from <f>] [--to <f>] [--fps <n>]] [--size <w>x<h>] [--control <port>] [--prefs <dir>|memory] [--ui <name>=<value> ...]").into());
            }
            other => return Err(format!("unknown argument {other:?} (try --help)")),
        }
    }
    match record {
        Some(path) => {
            if a.screenshot.is_some() {
                return Err("--record or --screenshot, not both".into());
            }
            // x264 wants even sizes
            if ac_media::Format::of(&path)? == ac_media::Format::Mp4 {
                a.size = a.size.map(|(w, h)| (w & !1, h & !1));
            }
            a.record = Some(app::record::Plan { path, from, to, fps });
        }
        None if from.is_some() || to.is_some() || fps.is_some() => {
            return Err("--from, --to and --fps go with --record".into());
        }
        None => {}
    }
    Ok(a)
}

fn main() -> std::process::ExitCode {
    ac_io::setup::init_logging();
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return std::process::ExitCode::from(2);
        }
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(ac_core::product::NAME)
            .with_inner_size(args.size.map_or([1440.0, 900.0], |(w, h)| [w as f32, h as f32])),
        renderer: eframe::Renderer::Wgpu,
        depth_buffer: 0,
        ..Default::default()
    };
    let result = eframe::run_native(
        ac_core::product::NAME,
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, args)?))),
    );
    match result {
        Ok(()) => match FAILED.get() {
            Some(why) => {
                eprintln!("{why}");
                std::process::ExitCode::FAILURE
            }
            None => std::process::ExitCode::SUCCESS,
        },
        Err(e) => {
            eprintln!("{} could not start: {e}", ac_core::product::NAME);
            std::process::ExitCode::from(3)
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn args(line: &str) -> Result<Args, String> {
        parse(line.split_whitespace().map(String::from))
    }

    #[test]
    fn an_mp4_rounds_its_size_down_to_even_and_a_gif_keeps_it() {
        assert_eq!(
            args("--record a.mp4 --size 1921x1081").unwrap().size,
            Some((1920, 1080))
        );
        assert_eq!(
            args("--record a.gif --size 1921x1081").unwrap().size,
            Some((1921, 1081))
        );
        assert_eq!(
            args("--screenshot a.png --size 1921x1081").unwrap().size,
            Some((1921, 1081))
        );
    }

    #[test]
    fn the_record_flags_are_checked_before_a_window_opens() {
        let plan = args("--record a.mp4 --from 2 --to 9 --fps 15")
            .unwrap()
            .record
            .unwrap();
        assert_eq!((plan.from, plan.to, plan.fps), (Some(2.0), Some(9.0), Some(15.0)));
        for bad in [
            "--record a.mov",
            "--record a.mp4 --screenshot a.png",
            "--from 2",
            "--fps 30",
            "--size 8",
            "--size 8x9000",
            "--size 100xNaN",
        ] {
            assert!(args(bad).is_err(), "{bad}");
        }
    }
}
