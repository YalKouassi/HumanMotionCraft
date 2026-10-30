//! The desktop app. Every edit the UI makes goes through `Session::run`, the same
//! commands the CLI and MCP drive.
//!
//! ```text
//! <bin> [--assets <dir>] [--library <dir>] [--open <id>] [--frame <n>] [--script <json>]
//!                [--screenshot <png>] [--control <port>] [--prefs <dir>|memory]
//!                [--ui <name>=<value> ...]
//! ```
//! `--ui` sets the window's own state (for screenshots): `tab=animations|builtin|poses`,
//! `source=all|mine|builtin`, `sort=name|recent|modified|length`, `search=<text>`, `dope=on|off`, `overlay=on|off|perf`, `ghost=on|off`,
//! `tool=drag|reach|move|rotate`, `camera=<view>` (front, back, side, left, top, bottom,
//! three-quarter: looking at the body), `zoom=<metres>`, `ortho=on|off`, `follow=on|off`,
//! `onion=on|off|keys|frames`, `onion_count=1..5`, `onion_step=<frames>`, `trails=on|off`,
//! `trail_span=<frames each side, 0 all>`, `picker=on|off`, `handles=body|all`,
//! `gizmo=world|local`, `select=<bone>[,<bone>...]|none` (see `app/view_state.rs`),
//! `dialog=export|preview|block|welcome|tips|recover|none` (`app/files`), and
//! the shell's: `window=palette|settings|history|messages|shortcuts|about|form:<command>`,
//! `palette=<search>`, `theme=dark|light`, `scale=<0.5..3>`, `colours=standard|safe`,
//! `units=frames|seconds|timecode`, `maximise=on|off`, `library|inspector|timeline=on|off`,
//! `side=left|right`, `perf=on|off`; the inspector panels': `joint=<bone>`, `box=<n>`,
//! `layer=<n>`, `travel=preview|off`, `prompt=<text>` (motion from text), `range_pair=<pair>`,
//! `range_side=L|R|Both` (the Range panel).
//! `--control <port>` opens the control port an agent drives the window through (`control.rs`;
//! `ac mcp --live <port>` makes MCP tools of it).
//! `--prefs` says where the saved preferences live; without it they are the app's config folder,
//! except with `--screenshot` or `--control`, which keep them in memory (`app/prefs.rs`).

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

#[derive(Debug, Default, Clone)]
pub struct Args {
    pub assets: Option<PathBuf>,
    pub library: Option<PathBuf>,
    pub open: Option<String>,
    pub frame: Option<f64>,
    pub screenshot: Option<PathBuf>,
    /// Commands run at startup: a JSON list of {command, params}.
    pub script: Option<PathBuf>,
    /// The control port (localhost) for agents.
    pub control: Option<u16>,
    /// Window state to set at startup: (name, value).
    pub ui: Vec<(String, String)>,
    /// Where the preferences live: a folder, or `memory` (`app/prefs.rs`).
    pub prefs: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--assets" => a.assets = Some(value()?.into()),
            "--library" => a.library = Some(value()?.into()),
            "--open" => a.open = Some(value()?),
            "--frame" => a.frame = Some(value()?.parse().map_err(|e| format!("--frame: {e}"))?),
            "--screenshot" => a.screenshot = Some(value()?.into()),
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
                return Err(concat!(env!("CARGO_BIN_NAME"), " [--assets <dir>] [--library <dir>] [--open <id>] [--frame <n>] [--script <json>] [--screenshot <png>] [--control <port>] [--prefs <dir>|memory] [--ui <name>=<value> ...]").into());
            }
            other => return Err(format!("unknown argument {other:?} (try --help)")),
        }
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
            .with_inner_size([1440.0, 900.0]),
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
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{} could not start: {e}", ac_core::product::NAME);
            std::process::ExitCode::from(3)
        }
    }
}
