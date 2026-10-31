//! Motion from text: `text.generate` makes a new stretch of animation from a prompt, `text.fill`
//! fills the gap between two of your keys from one, `text.helper` says whether the helper is
//! installed and running (and loads or stops it), and `text.install` installs it.
//!
//! The motion comes from an optional helper process that `tools/textmotion/setup_textmotion.py`
//! installs outside the repo ([`install`], [`helper`]; the app runs that setup itself through
//! `text.install`, [`setup`]; what every such helper shares is `ac-helper`); without it every command but `text.helper` and `text.install`
//! refuses and says how to install it. The frames it makes are AI keys tagged
//! `"text <prompt>"` ([`ac_core::edits::GENERATED`]), so an edit next to them leaves them as they
//! are and says so. [`engine`] is the seam: tests use a fake, and the window makes the motion off
//! its UI thread ([`prepare`]) before running the command, which then finds it ready.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod engine;
pub mod helper;
pub mod install;
pub mod setup;
pub mod stand_in;

use ac_core::Document;
use ac_core::clip::{Clip, Interp, Key, KeySource, LayerMode, LayerSource};
use ac_core::command::{CommandSpec, Kind as K, ParamSpec};
use ac_core::edits::{self, Filler};
use ac_core::error::hinted;
use ac_core::quat::{Quat, Vec3};
use serde_json::{Map, Value, json};

pub use engine::{Engine, Held, Motion, Request, Status};

type P = Map<String, Value>;
type Out = Result<Value, String>;

/// The frame rate the model works at and the most frames it makes at once (10 s).
pub const MODEL_FPS: f64 = 30.0;
pub const MODEL_FRAMES: i64 = 300;
/// The longest prompt taken.
pub const MAX_PROMPT: usize = 500;

const PROMPT: ParamSpec = ParamSpec::new(
    "prompt",
    K::Text,
    "what the body does, in plain words (\"A person waves hello with their right hand.\")",
)
.required();
const SEED: ParamSpec = ParamSpec::new(
    "seed",
    K::Integer,
    "another seed makes another take of the same prompt (default 1)",
)
.range(0.0, 1e9);

pub static SPECS: &[CommandSpec] = &[
    CommandSpec::new("text.helper", "Motion from text: helper", text_helper)
        .icon("ai")
        .inputs(&[ParamSpec::new(
            "action",
            K::Choice(&["status", "load", "stop"]),
            "status (default) says what is installed and running; load starts the helper and loads the model (a minute or more); stop ends it",
        )])
        .outputs(HELPER_OUT),
    CommandSpec::new("text.install", "Motion from text: install", text_install)
        .icon("ai")
        .writes()
        .inputs(&[
            ParamSpec::new(
                "action",
                K::Choice(&["check", "start", "stop"]),
                "check (default) says what the install needs and what it is doing; start runs the setup (about 22 GB downloaded, an hour or more; the window stays usable); stop ends it, and start carries on from there",
            ),
            ParamSpec::new(
                "repair",
                K::Bool,
                "start even when it is installed: the setup updates or repairs it",
            ),
        ])
        .outputs(INSTALL_OUT),
    CommandSpec::new("text.generate", "Generate motion from text", text_generate)
        .icon("ai")
        .undoable()
        .inputs(&[
            PROMPT,
            ParamSpec::new("seconds", K::Number, "how long (default 4)").range(0.1, 9.9),
            SEED,
            ParamSpec::new(
                "at",
                K::Integer,
                "the frame it starts on (default the playhead); it starts from the pose there when the animation has keys",
            )
            .range(0.0, 1e7),
        ])
        .outputs(MADE_OUT),
    CommandSpec::new("text.fill", "Fill gap from text", text_fill)
        .icon("ai")
        .undoable()
        .inputs(&[
            PROMPT,
            ParamSpec::new(
                "from",
                K::Integer,
                "your key before the gap (default: around the playhead)",
            )
            .range(0.0, 1e7),
            ParamSpec::new("to", K::Integer, "your key after the gap").range(0.0, 1e7),
            SEED,
        ])
        .outputs(MADE_OUT),
];

const HELPER_OUT: &[ParamSpec] = &[
    ParamSpec::new("installed", K::Bool, "the helper is installed").required(),
    ParamSpec::new("dir", K::Text, "where it is (or would be) installed").required(),
    ParamSpec::new("running", K::Bool, "its process is up").required(),
    ParamSpec::new("loaded", K::Bool, "the model is in memory").required(),
    ParamSpec::new("busy", K::Bool, "it is making motion now").required(),
    ParamSpec::new("model", K::Text, "the model installed").required(),
    ParamSpec::new("note", K::Text, "what is missing and how to install it"),
];

const INSTALL_OUT: &[ParamSpec] = &[
    ParamSpec::new(
        "state",
        K::Choice(&[
            "not installed",
            "installing",
            "stopping",
            "installed",
            "failed",
            "stopped",
        ]),
        "what the install is doing",
    )
    .required(),
    ParamSpec::new("dir", K::Text, "where it is (or would be) installed").required(),
    ParamSpec::new("log", K::Text, "the setup's log"),
    ParamSpec::new(
        "step",
        K::Integer,
        "the setup's step working now (or where it ended), of steps",
    ),
    ParamSpec::new("steps", K::Integer, "how many steps the setup has"),
    ParamSpec::new("label", K::Text, "what that step does"),
    ParamSpec::new(
        "last",
        K::Text,
        "the last line the setup printed (a download's progress)",
    ),
    ParamSpec::new("seconds", K::Integer, "how long the setup started here has run"),
    ParamSpec::new("error", K::Text, "why it failed"),
    ParamSpec::new(
        "requirements",
        K::Text,
        "when it is not installed or running: what it needs, a list of {name, ok, required, note}",
    )
    .json(),
    ParamSpec::new("ready", K::Bool, "everything required is there: start can run"),
];

const MADE_OUT: &[ParamSpec] = &[
    ParamSpec::new("from", K::Integer, "the motion's first frame (a key of yours)").required(),
    ParamSpec::new("to", K::Integer, "its last frame (a key of yours)").required(),
    ParamSpec::new("frames", K::Integer, "AI keys written between them").required(),
    ParamSpec::new("model", K::Text, "the keys' model tag: \"text\" and the prompt").required(),
    ParamSpec::new("notes", K::TextList, "what to know about the result").required(),
];

/// Register this crate's commands; returns the clashes (empty: all registered).
pub fn register(session: &mut ac_core::Session) -> Vec<ac_core::CoreError> {
    session.register(SPECS)
}

fn text_helper(_: &mut Document, p: &P) -> Out {
    let dir = install::dir();
    let found = dir.as_deref().map(install::read);
    let shown = dir.as_ref().map(|d| d.display().to_string()).unwrap_or_default();
    match p.get("action").and_then(Value::as_str).unwrap_or("status") {
        "load" => {
            let st = engine::load()?;
            Ok(helper_json(true, &shown, Some(st), None))
        }
        "stop" => {
            engine::stop();
            Ok(helper_json(
                found.as_ref().is_some_and(|f| f.is_ok()),
                &shown,
                Some(Status::default()),
                None,
            ))
        }
        _ => {
            let (installed, note, model) = match &found {
                Some(Ok(i)) => (true, None, i.model.clone()),
                Some(Err(e)) => (false, Some(e.replace(ac_core::error::HINT, ": ")), String::new()),
                None => (
                    false,
                    Some(format!("no data folder is known: {}", install::HOW)),
                    String::new(),
                ),
            };
            let mut out = helper_json(installed, &shown, engine::status(), note);
            if out["model"] == "" {
                out["model"] = json!(model);
            }
            Ok(out)
        }
    }
}

fn text_install(doc: &mut Document, p: &P) -> Out {
    let dir = install::dir().ok_or_else(|| {
        hinted(
            "no data folder is known to install into",
            "set AC_TEXTMOTION_DIR to the folder to install into",
        )
    })?;
    let assets = doc.assets.clone();
    match p.get("action").and_then(Value::as_str).unwrap_or("check") {
        "start" => {
            let repair = p.get("repair").and_then(Value::as_bool).unwrap_or(false);
            setup::start(&dir, assets.as_deref(), repair)?;
        }
        "stop" => setup::stop()?,
        _ => {}
    }
    let mut out = setup::status(&dir);
    if matches!(
        out["state"].as_str(),
        Some("not installed" | "failed" | "stopped")
    ) {
        let needs = setup::requirements(assets.as_deref(), &dir);
        out["ready"] = json!(needs.iter().all(|n| n.ok || !n.required));
        out["requirements"] = serde_json::to_value(needs).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

fn helper_json(installed: bool, dir: &str, st: Option<Status>, note: Option<String>) -> Value {
    let busy = st.is_none();
    let st = st.unwrap_or(Status {
        running: true,
        ..Status::default()
    });
    let mut v = json!({
        "installed": installed, "dir": dir, "running": st.running, "loaded": st.loaded,
        "busy": busy, "model": st.model,
    });
    if let Some(n) = note {
        v["note"] = json!(n);
    }
    v
}

/// The prompt, trimmed and on one line.
fn prompt(p: &P) -> Result<String, String> {
    let raw = p.get("prompt").and_then(Value::as_str).unwrap_or_default();
    let text = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return Err("the prompt is empty: say what the body does".into());
    }
    if text.chars().count() > MAX_PROMPT {
        return Err(format!("the prompt is over {MAX_PROMPT} characters"));
    }
    Ok(text)
}

fn seed(p: &P) -> i64 {
    p.get("seed").and_then(Value::as_i64).unwrap_or(1)
}

/// The keys' model tag.
pub fn tag(prompt: &str) -> String {
    format!("{} {prompt}", edits::GENERATED)
}

/// The edited layer takes whole poses.
fn whole_layer(clip: &Clip) -> Result<(), String> {
    let l = clip.layers.get(clip.edit).ok_or("no layer to edit")?;
    if l.source != LayerSource::Keys || l.mode == LayerMode::Additive || l.only.is_some() {
        return Err(hinted(
            format!("layer {:?} doesn't take whole poses", l.name),
            "motion from text writes the whole body: edit a full keys layer (the base layer is one)",
        ));
    }
    Ok(())
}

/// `frames` frames (both ends) at `fps` fit in what the model makes at once.
fn fits(frames: i64, fps: f64) -> Result<(), String> {
    let n = ac_helper::frames::model_frames(frames, fps, MODEL_FPS);
    if frames < 2 || n > MODEL_FRAMES {
        let most = (MODEL_FRAMES - 1) as f64 / MODEL_FPS;
        return Err(format!(
            "{frames} frames is {:.1} s; the model makes 2 frames to {most:.1} s at once",
            (frames - 1) as f64 / fps
        ));
    }
    Ok(())
}

/// The body's pose on frame `f`, as the animation plays it.
fn held(doc: &Document, f: i64) -> Result<Held, String> {
    let (r, o) = doc.clip.sample_with(&[f as f64], &doc.links)?;
    Ok(Held {
        rotations: r.into_iter().next().ok_or("no pose")?,
        root: o.into_iter().next().ok_or("no pose")?,
    })
}

fn request(
    doc: &Document,
    prompt: &str,
    frames: i64,
    seed: i64,
    start: Option<Held>,
    end: Option<Held>,
) -> Result<Request, String> {
    let assets = doc
        .assets
        .as_ref()
        .ok_or("no assets folder is set (start the app with one)")?;
    Ok(Request {
        prompt: prompt.to_string(),
        frames: usize::try_from(frames).map_err(|e| e.to_string())?,
        fps: doc.clip.fps,
        rig: assets.join("rig/default_rig.json").display().to_string(),
        seed,
        start,
        end,
    })
}

/// What `text.generate` asks for, and the frames it covers.
fn generate_plan(doc: &Document, p: &P) -> Result<(Request, i64, i64), String> {
    let prompt = prompt(p)?;
    whole_layer(&doc.clip)?;
    let fps = doc.clip.fps;
    let seconds = p.get("seconds").and_then(Value::as_f64).unwrap_or(4.0);
    let frames = (seconds * fps).round() as i64 + 1;
    fits(frames, fps)?;
    let at = p
        .get("at")
        .and_then(Value::as_i64)
        .unwrap_or_else(|| doc.key_frame())
        .max(0);
    let end = at + frames - 1;
    let inside: Vec<i64> = doc
        .clip
        .keys()
        .iter()
        .map(|k| k.frame)
        .filter(|&f| at < f && f <= end)
        .collect();
    if let (Some(first), Some(last)) = (inside.first(), inside.last()) {
        let which = if first == last {
            format!("a key lies on frame {first}")
        } else {
            format!("{} keys lie on frames {first}-{last}", inside.len())
        };
        return Err(hinted(
            format!("{which}, where the motion would go ({at}-{end})"),
            "start it after your last key, make it shorter, or fill between two of your keys with text.fill",
        ));
    }
    let start = if doc.clip.length_with(&doc.links) > 0 {
        Some(held(doc, at)?)
    } else {
        None
    };
    Ok((request(doc, &prompt, frames, seed(p), start, None)?, at, end))
}

/// What `text.fill` asks for: the gap's keys and the request.
fn fill_plan(doc: &Document, p: &P) -> Result<(Request, i64, i64), String> {
    let prompt = prompt(p)?;
    whole_layer(&doc.clip)?;
    let (a, b) = match (
        p.get("from").and_then(Value::as_i64),
        p.get("to").and_then(Value::as_i64),
    ) {
        (None, None) => edits::span_around(&doc.clip, doc.key_frame())
            .ok_or("put the playhead between two of your keys, or give `from` and `to`")?,
        (Some(a), Some(b)) => (a, b),
        _ => return Err("give both `from` and `to`, or neither".into()),
    };
    let m = edits::manual_frames(&doc.clip);
    if !m.contains(&a) || !m.contains(&b) || a >= b || m.iter().any(|&f| a < f && f < b) {
        return Err(format!("{a} and {b} are not neighbouring keys of yours"));
    }
    if b - a < 2 {
        return Err(format!("{a} and {b} have no frames between them to fill"));
    }
    fits(b - a + 1, doc.clip.fps)?;
    let (start, end) = (held(doc, a)?, held(doc, b)?);
    Ok((
        request(doc, &prompt, b - a + 1, seed(p), Some(start), Some(end))?,
        a,
        b,
    ))
}

/// Make the motion `command` (`text.generate` or `text.fill`) would write, without writing it:
/// a front end runs this off its UI thread, then runs the command, which finds the motion ready.
pub fn prepare(doc: &Document, command: &str, p: &P) -> Result<(), String> {
    let (req, _, _) = match command {
        "text.generate" => generate_plan(doc, p)?,
        "text.fill" => fill_plan(doc, p)?,
        "text.helper" => return engine::load().map(|_| ()),
        _ => return Err(format!("{command} makes no motion")),
    };
    engine::make(&req).map(|_| ())
}

/// Whether running `command` with `p` now would wait for the model (a minute or more): not when
/// its motion is ready, nor when it is refused before the model is asked. A front end makes such
/// a run off its UI thread first ([`prepare`], or [`engine::load`] for `text.helper`'s load).
pub fn waits(doc: &Document, command: &str, p: &P) -> bool {
    let plan = match command {
        "text.generate" => generate_plan(doc, p),
        "text.fill" => fill_plan(doc, p),
        "text.helper" => {
            return p.get("action").and_then(Value::as_str) == Some("load")
                && !engine::status().is_some_and(|s| s.loaded);
        }
        _ => return false,
    };
    plan.is_ok_and(|(req, _, _)| engine::ready(&req).is_none())
}

/// The motion for `req`, with as many bones as the rig.
fn motion(doc: &Document, req: &Request) -> Result<std::sync::Arc<Motion>, String> {
    let m = engine::make(req)?;
    let bones = doc.skeleton.names.len();
    if m.rotations.iter().any(|f| f.len() != bones) {
        return Err(format!("the helper's motion is not for this rig ({bones} bones)"));
    }
    Ok(m)
}

fn text_generate(doc: &mut Document, p: &P) -> Out {
    let (req, at, end) = generate_plan(doc, p)?;
    let m = motion(doc, &req)?;
    let model = tag(&req.prompt);
    let mut written = 0;
    for (i, (r, o)) in m.rotations.iter().zip(&m.roots).enumerate() {
        let f = at + i as i64;
        let manual = f == at || f == end;
        if f == at
            && doc
                .clip
                .keys()
                .iter()
                .any(|k| k.frame == at && k.source == KeySource::Manual)
        {
            continue; // your key stays: the motion starts from it
        }
        let mut k = Key::new(f, r.clone(), *o, Interp::Smooth);
        if !manual {
            k.source = KeySource::Ai;
            k.model = model.clone();
            written += 1;
        }
        doc.clip.set_key(k)?;
    }
    let mut notes = m.notes.clone();
    if req.start.is_none() {
        notes.push("the animation had no keys: the motion starts where the model puts it".into());
    }
    let shown = doc.frame;
    doc.go_to(shown)?;
    Ok(json!({ "from": at, "to": end, "frames": written, "model": model, "notes": notes }))
}

/// The motion made for a gap, handed to `edits::refill` (frames strictly between the keys).
struct Made<'a> {
    motion: &'a Motion,
    tag: String,
}

impl Filler for Made<'_> {
    fn fill(&self, _: &Clip, a: i64, b: i64) -> Result<(Vec<Vec<Quat>>, Vec<Vec3>, String), String> {
        let inner = usize::try_from(b - a - 1).map_err(|e| e.to_string())?;
        let rots = self
            .motion
            .rotations
            .get(1..=inner)
            .ok_or("the motion is too short")?;
        let roots = self
            .motion
            .roots
            .get(1..=inner)
            .ok_or("the motion is too short")?;
        Ok((rots.to_vec(), roots.to_vec(), self.tag.clone()))
    }

    fn long(&self) -> bool {
        true
    }
}

fn text_fill(doc: &mut Document, p: &P) -> Out {
    let (req, a, b) = fill_plan(doc, p)?;
    let m = motion(doc, &req)?;
    let made = Made {
        motion: &m,
        tag: tag(&req.prompt),
    };
    let note = edits::refill(&mut doc.clip, a, b, &made, &doc.links)?;
    if !note.is_empty() {
        return Err(note);
    }
    let shown = doc.frame;
    doc.go_to(shown)?;
    Ok(json!({ "from": a, "to": b, "frames": b - a - 1, "model": made.tag, "notes": m.notes }))
}
