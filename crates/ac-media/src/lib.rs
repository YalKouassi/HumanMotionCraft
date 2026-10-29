//! Pictures and video of the open animation as it plays: an MP4 (through ffmpeg) or animated
//! GIF (`clip.video`) and a contact sheet (`clip.sheet`). The window's Export preview, the CLI's
//! `ac video` and MCP all use this crate, so they make the same files.
//!
//! Headless (a GPU device of its own, or the software adapter) and without egui. Paths are
//! parameters: nothing here opens a dialog.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod encode;
pub mod shots;

use std::path::PathBuf;

use ac_core::Document;
use ac_core::command::{self as cmd, CommandSpec, Kind as K, ParamSpec};
use serde_json::{Map, Value, json};

pub use encode::{Encoder, Format, ffmpeg};
pub use shots::Shots;

type P = Map<String, Value>;
type Out = Result<Value, String>;

const VIEW: ParamSpec = ParamSpec::new(
    "view",
    K::Choice(shots::VIEWS),
    "the camera: front, back, left, right, side (default), top or three-quarter",
);
const WIDTH: ParamSpec = ParamSpec::new("width", K::Integer, "pixels across").range(16.0, 4096.0);
const HEIGHT: ParamSpec = ParamSpec::new("height", K::Integer, "pixels down").range(16.0, 4096.0);

pub static SPECS: &[CommandSpec] = &[
    CommandSpec::new("clip.video", "Export preview video", clip_video)
        .icon("video")
        .inputs(&[
            ParamSpec::new("path", K::Text, "the .mp4 (needs ffmpeg) or .gif to write").required(),
            ParamSpec::new("from", K::Number, "the first frame (default 0)").range(0.0, 1e6),
            ParamSpec::new("to", K::Number, "the last frame (default the animation's last)").range(0.0, 1e6),
            ParamSpec::new("fps", K::Number, "frames per second (default the animation's)").range(1.0, 120.0),
            VIEW,
            WIDTH,
            HEIGHT,
            ParamSpec::new(
                "fixed",
                K::Bool,
                "one camera that sees the whole shot, so travel shows (default: it follows the body)",
            ),
        ])
        .writes()
        .outputs(VIDEO_OUT),
    CommandSpec::new("clip.sheet", "Export contact sheet", clip_sheet)
        .icon("sheet")
        .inputs(&[
            ParamSpec::new("path", K::Text, "the .png to write").required(),
            ParamSpec::new("frames", K::IntList, "the frames to show (1 to 64)"),
            ParamSpec::new(
                "count",
                K::Integer,
                "else this many, spread over the animation (default 6)",
            )
            .range(1.0, 64.0),
            VIEW,
            ParamSpec::new("width", K::Integer, "a cell's pixels across (default 320)").range(16.0, 2048.0),
            ParamSpec::new("height", K::Integer, "a cell's pixels down (default 400)").range(16.0, 2048.0),
            ParamSpec::new("cols", K::Integer, "cells to a row (default: one row)").range(1.0, 64.0),
        ])
        .writes()
        .outputs(SHEET_OUT),
];

const PATH_OUT: ParamSpec = ParamSpec::new("path", K::Text, "the file written").required();
const NOTE_OUT: ParamSpec = ParamSpec::new(
    "note",
    K::Text,
    "how the pictures were made, when it matters (the software adapter)",
);

const VIDEO_OUT: &[ParamSpec] = &[
    PATH_OUT,
    ParamSpec::new("format", K::Choice(&["mp4", "gif"]), "the file's format").required(),
    ParamSpec::new("frames", K::Integer, "frames written").required(),
    ParamSpec::new("fps", K::Number, "frames per second").required(),
    ParamSpec::new("width", K::Integer, "pixels across").required(),
    ParamSpec::new("height", K::Integer, "pixels down").required(),
    ParamSpec::new("from", K::Number, "the first frame").required(),
    ParamSpec::new("to", K::Number, "the last frame").required(),
    NOTE_OUT,
];

const SHEET_OUT: &[ParamSpec] = &[
    PATH_OUT,
    ParamSpec::new("frames", K::Number, "the frames shown")
        .list()
        .required(),
    ParamSpec::new("width", K::Integer, "the sheet's pixels across").required(),
    ParamSpec::new("height", K::Integer, "the sheet's pixels down").required(),
    NOTE_OUT,
];

/// Register this crate's commands; returns the clashes (empty: all registered).
pub fn register(session: &mut ac_core::Session) -> Vec<ac_core::CoreError> {
    session.register(SPECS)
}

fn int(p: &P, name: &str) -> Option<u32> {
    p.get(name).and_then(Value::as_f64).map(|x| x as u32)
}

fn assets(doc: &Document) -> Result<PathBuf, String> {
    doc.assets
        .clone()
        .ok_or_else(|| "no assets folder is set (start the app with one)".to_string())
}

/// Play the open animation into a video file.
fn clip_video(doc: &mut Document, p: &P) -> Out {
    let path = PathBuf::from(cmd::text(p, "path").unwrap_or_default());
    let format = Format::of(&path)?;
    let (dw, dh) = match format {
        Format::Mp4 => (960, 540),
        Format::Gif => (480, 270),
    };
    let (mut w, mut h) = (int(p, "width").unwrap_or(dw), int(p, "height").unwrap_or(dh));
    if format == Format::Mp4 {
        // x264 wants even sizes
        (w, h) = (w & !1, h & !1);
    }
    let last = (doc.clip.length_with(&doc.links) - 1).max(0) as f64;
    let from = p.get("from").and_then(Value::as_f64).unwrap_or(0.0);
    let to = p
        .get("to")
        .and_then(Value::as_f64)
        .unwrap_or(last)
        .min(last.max(from));
    let fps = p.get("fps").and_then(Value::as_f64).unwrap_or(doc.clip.fps);
    let times = shots::times(doc.clip.fps, from, to, fps)?;
    let view = cmd::text(p, "view").unwrap_or("side");
    let fixed = p.get("fixed").and_then(Value::as_bool).unwrap_or(false);
    let mut shots = Shots::new(&assets(doc)?, &doc.skeleton.names)?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    }
    let enc = Encoder::create(&path, w, h, fps)?;
    let frames = shots.video(doc, &times, view, (w, h), fixed, enc)?;
    let mut out = json!({
        "path": path.to_string_lossy(), "format": format.name(), "frames": frames,
        "fps": fps, "width": w, "height": h, "from": from, "to": times.last().copied().unwrap_or(from),
    });
    if let Some(n) = shots.note {
        out["note"] = json!(n);
    }
    Ok(out)
}

/// A contact sheet of the open animation as a PNG.
fn clip_sheet(doc: &mut Document, p: &P) -> Out {
    let path = PathBuf::from(cmd::text(p, "path").unwrap_or_default());
    if path.extension().is_none_or(|e| !e.eq_ignore_ascii_case("png")) {
        return Err(format!("{}: the file must end in .png", path.display()));
    }
    let mut args = json!({});
    if let Some(f) = p.get("frames") {
        args["frames"] = f.clone();
    }
    if let Some(c) = p.get("count") {
        args["count"] = c.clone();
    }
    let frames = doc.sheet_frames(&args)?;
    let cell = (
        int(p, "width").unwrap_or(320),
        int(p, "height").unwrap_or(400),
        int(p, "cols").unwrap_or(frames.len() as u32).clamp(1, 64),
    );
    let view = cmd::text(p, "view").unwrap_or("side");
    let mut shots = Shots::new(&assets(doc)?, &doc.skeleton.names)?;
    let png = shots.sheet(doc, &frames, view, cell)?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    }
    craft_atomic::atomic_write(&path, &png).map_err(|e| format!("can't write {}: {e}", path.display()))?;
    let (w, h, _) = ac_render::offscreen::unpng(&png)?;
    let mut out = json!({ "path": path.to_string_lossy(), "frames": frames, "width": w, "height": h });
    if let Some(n) = shots.note {
        out["note"] = json!(n);
    }
    Ok(out)
}
