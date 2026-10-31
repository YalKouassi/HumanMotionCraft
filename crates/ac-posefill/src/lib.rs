//! Pose-to-pose fills: `posefill.fill` fills the gap between two keys of yours with motion a
//! model makes from the poses around them, `posefill.helper` says whether the helper is installed
//! and running (and loads or stops it), and `posefill.install` installs it.
//!
//! The motion comes from an optional helper process that `tools/posefill/setup_posefill.py`
//! installs outside the repo ([`install`], [`helper`]; the app runs that setup itself through
//! `posefill.install`, [`setup`]; what every such helper shares is `ac-helper`). It needs an
//! NVIDIA GPU. Without the install every command but `posefill.helper` and `posefill.install`
//! refuses and says how to install it. The frames it makes are AI keys tagged
//! [`ac_core::edits::POSE_FILL`], so an edit next to them leaves them as they are and says so.
//! [`engine`] is the seam: tests use a fake, and the window makes the fill off its UI thread
//! ([`prepare`]) before running the command, which then finds it ready.
//!
//! The model takes [`CONTEXT`] poses before and on key A and 1 to [`MAX_TARGETS`] from key B on,
//! and makes [`MIN_FRAMES`] to [`MAX_FRAMES`] frames at [`MODEL_FPS`]; the animation's own rate
//! is converted there and back. A longer stretch goes through the keys of yours inside it, one
//! call a gap ([`plan`]).
//!
//! The same model drives live motion control (`live.*`): a person steers, stretches of motion are
//! made towards target poses placed ahead, everything shown is recorded, and a range becomes keys
//! ([`live`], [`live_commands`]; registered beside [`SPECS`]).
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
pub mod live;
pub mod live_commands;
pub mod plan;
pub mod setup;
pub mod stand_in;

use ac_core::Document;
use ac_core::command::{CommandSpec, Kind as K, ParamSpec};
use ac_core::edits;
use ac_core::error::hinted;
use serde_json::{Map, Value, json};

pub use engine::{Engine, Fill, Info, Pose, Request, Status};

type P = Map<String, Value>;
type Out = Result<Value, String>;

/// The frame rate the model works at.
pub const MODEL_FPS: f64 = 30.0;
/// The fewest and most frames one call makes (key A's and key B's frames included).
pub const MIN_FRAMES: i64 = 18;
pub const MAX_FRAMES: i64 = 58;
/// The poses the model takes before and on key A.
pub const CONTEXT: usize = 4;
/// The most poses it takes from key B on.
pub const MAX_TARGETS: usize = 4;
/// The keys' model tag.
pub const TAG: &str = edits::POSE_FILL;

pub static SPECS: &[CommandSpec] = &[
    CommandSpec::new("posefill.helper", "Pose fill: helper", posefill_helper)
        .icon("ai")
        .inputs(&[ParamSpec::new(
            "action",
            K::Choice(&["status", "load", "stop"]),
            "status (default) says what is installed and running; load starts the helper and loads the model (a few minutes the first time, while it builds its plan for this GPU); stop ends it",
        )])
        .outputs(HELPER_OUT),
    CommandSpec::new("posefill.install", "Pose fill: install", posefill_install)
        .icon("ai")
        .writes()
        .inputs(&[
            ParamSpec::new(
                "action",
                K::Choice(&["check", "start", "stop"]),
                "check (default) says what the install needs and what it is doing; start runs the setup (about 4 GB downloaded, a few minutes; the window stays usable); stop ends it, and start carries on from there",
            ),
            ParamSpec::new(
                "repair",
                K::Bool,
                "start even when it is installed: the setup updates or repairs it",
            ),
        ])
        .outputs(INSTALL_OUT),
    CommandSpec::new("posefill.fill", "Fill gap from poses", posefill_fill)
        .icon("ai")
        .undoable()
        .inputs(&[
            ParamSpec::new(
                "from",
                K::Integer,
                "your key before the gap (default: around the playhead)",
            )
            .range(0.0, 1e7),
            ParamSpec::new(
                "to",
                K::Integer,
                "your key after the gap; keys of yours between from and to are gone through, one call a gap",
            )
            .range(0.0, 1e7),
        ])
        .outputs(MADE_OUT),
];

const HELPER_OUT: &[ParamSpec] = &[
    ParamSpec::new("installed", K::Bool, "the helper is installed").required(),
    ParamSpec::new("dir", K::Text, "where it is (or would be) installed").required(),
    ParamSpec::new("running", K::Bool, "its process is up").required(),
    ParamSpec::new("loaded", K::Bool, "the model is in memory").required(),
    ParamSpec::new("busy", K::Bool, "it is making a fill now").required(),
    ParamSpec::new("model", K::Text, "the model installed").required(),
    ParamSpec::new(
        "plan",
        K::Text,
        "whether the model's plan for this GPU is built (the first load builds it, a few minutes)",
    ),
    ParamSpec::new("gpu", K::Text, "the GPU the helper runs on"),
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
    ParamSpec::new(
        "licences",
        K::Text,
        "what installing accepts, a list of {part, licence, url}",
    )
    .json(),
    ParamSpec::new(
        "left_out",
        K::Text,
        "what the install never takes from the engine's source",
    ),
];

const MADE_OUT: &[ParamSpec] = &[
    ParamSpec::new("from", K::Integer, "the fill's first frame (a key of yours)").required(),
    ParamSpec::new("to", K::Integer, "its last frame (a key of yours)").required(),
    ParamSpec::new("frames", K::Integer, "AI keys written between them").required(),
    ParamSpec::new("model", K::Text, "the keys' model tag").required(),
    ParamSpec::new(
        "calls",
        K::Integer,
        "model calls made: one a gap between keys of yours",
    )
    .required(),
    ParamSpec::new(
        "seconds",
        K::Number,
        "the model's own time over those calls, as it reports it",
    )
    .required(),
    ParamSpec::new("notes", K::TextList, "what to know about the result").required(),
];

/// Register this crate's commands; returns the clashes (empty: all registered).
pub fn register(session: &mut ac_core::Session) -> Vec<ac_core::CoreError> {
    let mut clashes = session.register(SPECS);
    clashes.extend(session.register(live_commands::SPECS));
    clashes
}

fn posefill_helper(_: &mut Document, p: &P) -> Out {
    let dir = install::dir();
    let found = dir.as_deref().map(install::read);
    let shown = dir.as_ref().map(|d| d.display().to_string()).unwrap_or_default();
    let plan_on_disk = || {
        if dir.as_deref().is_some_and(install::plan_built) {
            "built"
        } else {
            "not built (the first load builds it, a few minutes)"
        }
    };
    match p.get("action").and_then(Value::as_str).unwrap_or("status") {
        "load" => {
            let st = engine::load()?;
            Ok(helper_json(true, &shown, Some(st), engine::info(), None))
        }
        "stop" => {
            engine::stop();
            Ok(helper_json(
                found.as_ref().is_some_and(|f| f.is_ok()),
                &shown,
                Some(Status::default()),
                None,
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
            let info = engine::info().or_else(|| {
                installed.then(|| Info {
                    plan: plan_on_disk().into(),
                    gpu: String::new(),
                })
            });
            let mut out = helper_json(installed, &shown, engine::status(), info, note);
            if out["model"] == "" {
                out["model"] = json!(model);
            }
            Ok(out)
        }
    }
}

fn posefill_install(doc: &mut Document, p: &P) -> Out {
    let dir = install::dir().ok_or_else(|| {
        hinted(
            "no data folder is known to install into",
            "set AC_POSEFILL_DIR to the folder to install into",
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
        out["licences"] = serde_json::to_value(setup::LICENCES).map_err(|e| e.to_string())?;
        out["left_out"] = json!(setup::NOT_INSTALLED);
    }
    Ok(out)
}

fn helper_json(
    installed: bool,
    dir: &str,
    st: Option<Status>,
    info: Option<Info>,
    note: Option<String>,
) -> Value {
    let busy = st.is_none();
    let st = st.unwrap_or(Status {
        running: true,
        ..Status::default()
    });
    let mut v = json!({
        "installed": installed, "dir": dir, "running": st.running, "loaded": st.loaded,
        "busy": busy, "model": st.model,
    });
    if let Some(i) = info {
        v["plan"] = json!(i.plan);
        if !i.gpu.is_empty() {
            v["gpu"] = json!(i.gpu);
        }
    }
    if let Some(n) = note {
        v["note"] = json!(n);
    }
    v
}

/// Make the fills `posefill.fill` would write, without writing them: a front end runs this off
/// its UI thread, then runs the command, which finds them ready.
pub fn prepare(doc: &Document, command: &str, p: &P) -> Result<(), String> {
    match command {
        "posefill.fill" => {
            let plan = plan::plan(doc, p)?;
            plan::run(doc, &plan, engine::make).map(|_| ())
        }
        "posefill.helper" | "live.start" => engine::load().map(|_| ()),
        _ => Err(format!("{command} makes no motion")),
    }
}

/// Whether running `command` with `p` now would wait for the model: not when its fills are ready,
/// nor when it is refused before the model is asked. A front end makes such a run off its UI
/// thread first ([`prepare`], or [`engine::load`] for `posefill.helper`'s load).
pub fn waits(doc: &Document, command: &str, p: &P) -> bool {
    match command {
        "posefill.fill" => {
            let Ok(plan) = plan::plan(doc, p) else {
                return false;
            };
            plan::run(doc, &plan, |req| {
                engine::ready(req).ok_or_else(|| "not made yet".to_string())
            })
            .is_err()
        }
        "posefill.helper" => {
            p.get("action").and_then(Value::as_str) == Some("load")
                && !engine::status().is_some_and(|s| s.loaded)
        }
        "live.start" => !engine::status().is_some_and(|s| s.loaded),
        _ => false,
    }
}

fn posefill_fill(doc: &mut Document, p: &P) -> Out {
    let plan = plan::plan(doc, p)?;
    let fills = plan::run(doc, &plan, engine::make)?;
    let bones = doc.skeleton.names.len();
    if fills.iter().any(|f| f.rotations.iter().any(|r| r.len() != bones)) {
        return Err(format!("the helper's fill is not for this rig ({bones} bones)"));
    }
    let mut notes = plan.notes.clone();
    let mut written = 0;
    let mut seconds = 0.0;
    for ((a, b), fill) in plan.segments().into_iter().zip(&fills) {
        let made = plan::Made {
            fill,
            tag: TAG.to_string(),
        };
        let note = edits::refill(&mut doc.clip, a, b, &made, &doc.links)?;
        if !note.is_empty() {
            return Err(note);
        }
        written += b - a - 1;
        seconds += fill.seconds;
        for n in &fill.notes {
            if !notes.contains(n) {
                notes.push(n.clone());
            }
        }
    }
    let shown = doc.frame;
    doc.go_to(shown)?;
    Ok(json!({
        "from": plan.from(), "to": plan.to(), "frames": written, "model": TAG,
        "calls": fills.len(), "seconds": seconds, "notes": notes,
    }))
}
