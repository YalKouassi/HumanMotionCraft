//! `animation_preview`: the clip as an animated GIF (inline when small, else a file) and, if asked,
//! an MP4. The frames to draw are worked out here from the editor state, so headless and live
//! render exactly the same plan.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::backend::Animation;

/// Most frames one preview draws; a longer range is thinned (and the result says so).
pub const MAX_FRAMES: u64 = 240;
/// Most pixels over all frames (e.g. 240 frames of 400 x 400).
pub const MAX_PIXELS: u64 = 40_000_000;
/// GIFs up to this many KiB come back inline unless `inline_max_kb` says otherwise.
pub const INLINE_KB: u64 = 1024;
/// The largest inline GIF a caller may ask for.
pub const MAX_INLINE_KB: u64 = 8192;

fn num(args: &Value, name: &str) -> Result<Option<f64>, String> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => match v.as_f64() {
            Some(x) if x.is_finite() => Ok(Some(x)),
            _ => Err(format!("{name} must be a finite number, got {v}")),
        },
    }
}

fn whole(args: &Value, name: &str, default: u64, lo: u64, hi: u64) -> Result<u64, String> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => match v.as_u64() {
            Some(x) if (lo..=hi).contains(&x) => Ok(x),
            _ => Err(format!("{name} must be a whole number {lo}..={hi}, got {v}")),
        },
    }
}

/// A file to write: an absolute path ending in `ext`, not in the root of a repository (a folder
/// with `Cargo.toml` or `.git`).
pub fn out_path(path: &str, ext: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(path);
    if !p.is_absolute() {
        return Err(format!("{path:?} must be an absolute path"));
    }
    if p.extension().is_none_or(|e| !e.eq_ignore_ascii_case(ext)) {
        return Err(format!("{path:?} must end in .{ext}"));
    }
    let dir = p.parent().ok_or_else(|| format!("{path:?} has no folder"))?;
    if dir.join("Cargo.toml").exists() || dir.join(".git").exists() {
        return Err(format!(
            "{} is the root of a repository; write under a folder in it (target/, fixtures/) or elsewhere",
            dir.display()
        ));
    }
    Ok(p)
}

/// The plan both backends render: `{times, view, width, height, fixed, delay, gif, mp4, fps}`,
/// and a note for anything changed from what was asked.
pub fn plan(args: &Value, state: &Value) -> Result<(Value, Vec<String>), String> {
    let clip_fps = state["fps"].as_f64().filter(|f| *f > 0.0).unwrap_or(30.0);
    let length = state["length"].as_f64().unwrap_or(1.0).max(1.0);
    let last = (length - 1.0).max(0.0);
    let from = num(args, "from")?.unwrap_or(0.0);
    let to = num(args, "to")?.unwrap_or(last);
    if !(0.0..=1e6).contains(&from) || !(0.0..=1e6).contains(&to) || to < from {
        return Err(format!(
            "frames {from}..{to}: from and to are 0..=1e6, from first"
        ));
    }
    let fps = num(args, "fps")?.unwrap_or(clip_fps.min(30.0));
    if !(1.0..=60.0).contains(&fps) {
        return Err(format!("fps {fps} must be 1..=60"));
    }
    let width = whole(args, "width", 320, 16, 1024)?;
    let height = whole(args, "height", 240, 16, 1024)?;
    let max_frames = whole(
        args,
        "max_frames",
        MAX_FRAMES,
        1,
        ac_render::anim::MAX_FRAMES as u64,
    )?;
    let view = args["view"].as_str().unwrap_or("side");
    let mut notes = Vec::new();
    // frames of the clip between pictures: the clip's fps over the preview's
    let mut step = clip_fps / fps;
    let mut n = ((to - from) / step).floor() as u64 + 1;
    let cap = max_frames.min(MAX_PIXELS / (width * height)).max(1);
    if n > cap {
        step = if cap > 1 {
            (to - from) / (cap - 1) as f64
        } else {
            to - from + 1.0
        };
        n = cap;
        notes.push(format!(
            "{from}..{to} at {fps} fps is more than {cap} pictures: drawn every {step:.2} frames instead (it still plays in real time)"
        ));
    }
    let times: Vec<f64> = (0..n)
        .map(|i| from + i as f64 * step)
        .filter(|t| *t <= to + 1e-9)
        .collect();
    let look = ac_media::look_of(args)?;
    ac_media::no_transparent(&look, "an animated preview")?;
    let mp4 = match args.get("mp4").and_then(Value::as_str) {
        Some(p) => {
            if width % 2 != 0 || height % 2 != 0 {
                return Err(format!(
                    "an MP4 needs an even width and height, got {width}x{height}"
                ));
            }
            Some(out_path(p, "mp4")?.to_string_lossy().into_owned())
        }
        None => None,
    };
    let gif = match args.get("gif") {
        None | Some(Value::Null) => true,
        Some(Value::Bool(b)) => *b,
        Some(v) => return Err(format!("gif must be true or false, got {v}")),
    };
    if !gif && mp4.is_none() {
        return Err("nothing to make: gif is false and no mp4 path".into());
    }
    let fixed = match args.get("fixed") {
        None | Some(Value::Null) => true,
        Some(Value::Bool(b)) => *b,
        Some(v) => return Err(format!("fixed must be true or false, got {v}")),
    };
    let mut planned = json!({
        "times": times,
        "view": view,
        "width": width,
        "height": height,
        "fixed": fixed,
        "delay": ac_render::anim::delay_cs(clip_fps / step),
        "fps": clip_fps / step,
        "gif": gif,
        "mp4": mp4,
    });
    for name in ac_media::LOOK_ARGS {
        if let Some(v) = args.get(*name) {
            planned[*name] = v.clone();
        }
    }
    Ok((planned, notes))
}

/// The tool's result: the GIF inline (small) or written to a file, and what was drawn.
pub fn result(
    args: &Value,
    plan: &Value,
    notes: Vec<String>,
    anim: Animation,
    n: u32,
) -> Result<Value, String> {
    let times = plan["times"].as_array().map_or(&[][..], Vec::as_slice);
    let mut info = json!({
        "frames": anim.frames,
        "from": times.first(),
        "to": times.last(),
        "fps": plan["fps"],
        "view": plan["view"],
        "size": [plan["width"], plan["height"]],
        "camera": if plan["fixed"] == true { "fixed: sees every frame" } else { "follows the root" },
        "mp4": anim.mp4,
        "notes": notes,
    });
    let Some(gif) = anim.gif else {
        return Ok(json!({ "content": [{ "type": "text", "text": info.to_string() }] }));
    };
    let inline_kb = whole(args, "inline_max_kb", INLINE_KB, 0, MAX_INLINE_KB)?;
    info["gif_bytes"] = json!(gif.len());
    let too_big = gif.len() as u64 > inline_kb * 1024;
    let file = match args.get("out").and_then(Value::as_str) {
        Some(p) => Some(out_path(p, "gif")?),
        None if too_big => {
            let dir = std::env::temp_dir().join(format!("{}-previews", ac_core::product::ID));
            Some(dir.join(format!("preview-{}-{n}.gif", std::process::id())))
        }
        None => None,
    };
    if let Some(path) = &file {
        write(path, &gif)?;
        info["gif"] = json!(path.to_string_lossy());
    }
    if too_big {
        let note = format!(
            "the GIF is {} KiB, over {inline_kb} KiB (inline_max_kb): written to the file, not shown inline",
            gif.len() / 1024
        );
        if let Some(a) = info["notes"].as_array_mut() {
            a.push(json!(note));
        }
        return Ok(json!({ "content": [{ "type": "text", "text": info.to_string() }] }));
    }
    Ok(json!({ "content": [
        { "type": "image", "data": crate::render::base64(&gif), "mimeType": "image/gif" },
        { "type": "text", "text": info.to_string() },
    ]}))
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn a_long_range_is_thinned_and_says_so() {
        let state = json!({ "fps": 30, "length": 1000 });
        let (p, notes) = plan(&json!({}), &state).unwrap();
        assert_eq!(p["times"].as_array().unwrap().len(), MAX_FRAMES as usize);
        assert_eq!(p["times"][0], 0.0);
        assert!((p["times"][239].as_f64().unwrap() - 999.0).abs() < 1e-9);
        assert_eq!(notes.len(), 1);
        let (p, notes) = plan(&json!({ "from": 10, "to": 20, "fps": 15 }), &state).unwrap();
        assert_eq!(p["times"], json!([10.0, 12.0, 14.0, 16.0, 18.0, 20.0]));
        assert_eq!(p["delay"], 7);
        assert!(notes.is_empty());
    }

    #[test]
    fn bad_requests_are_refused() {
        let state = json!({ "fps": 30, "length": 60 });
        for bad in [
            json!({ "from": 20, "to": 10 }),
            json!({ "fps": 0 }),
            json!({ "width": 5000 }),
            json!({ "from": "x" }),
            json!({ "mp4": "relative.mp4" }),
            json!({ "mp4": "C:/x/out.mp4", "width": 321 }),
            json!({ "gif": false }),
            json!({ "background": "transparent" }),
            json!({ "background": "green" }),
            json!({ "azimuth": 1.0 }),
            json!({ "azimuth": 1.0, "elevation": 0.1, "distance": 1e9 }),
            json!({ "target": [0, 0, 1] }),
            json!({ "floor": "no" }),
        ] {
            assert!(plan(&bad, &state).is_err(), "{bad}");
        }
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../a.gif");
        assert!(out_path(&root.to_string_lossy(), "gif").is_err());
    }
}
