//! Live motion control's commands: `live.start` a take from the pose on show, `live.input` steer
//! it and move it on some frames, `live.stop`, and `live.clip` a range of the recording into the
//! animation. The take and its recording live here, outside the document, until they are clipped:
//! steering changes nothing to undo (only the derived pose on show, which `live.stop` puts back),
//! and a clip is one undo step. For a clip as a new animation, run `clip.new` and `live.clip` in one
//! `edit.batch`.
//!
//! A script's `live.input` waits for each stretch, so a take gives the same frames every run. The
//! window passes `wait: false`, and the model works on a thread while the current stretch plays on.

use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

use ac_core::Document;
use ac_core::clip::{Interp, Key, KeySource};
use ac_core::command::{CommandSpec, Kind as K, ParamSpec};
use ac_core::edits;
use ac_core::error::hinted;
use ac_core::quat;
use serde_json::{Map, Value, json};

use crate::engine::{self, LiveStart, Pose};
use crate::live::{MAX_FRAMES, State, StepAsk, Stick, Stretch, Take};
use crate::plan::whole_layer;
use crate::{MODEL_FPS, TAG};

type P = Map<String, Value>;
type Out = Result<Value, String>;

/// The target set, under the assets folder.
pub const TARGETS: &str = "posefill-targets/locomotion.json";
/// The most frames one `live.input` moves on.
const MAX_STEP: i64 = 600;
/// Frames of the current stretch that play on while the window waits for the next one: about what
/// a reply takes, so the new stretch starts where the old one has got to.
const LOOKAHEAD: usize = 2;

pub static SPECS: &[CommandSpec] = &[
    CommandSpec::new("live.start", "Live motion: start a take", live_start)
        .icon("ai")
        .inputs(&[ParamSpec::new(
            "max_frames",
            K::Integer,
            "the most frames to record (default ten minutes at 30 fps)",
        )
        .range(2.0, MAX_FRAMES as f64)])
        .outputs(START_OUT),
    CommandSpec::new("live.input", "Live motion: steer", live_input)
        .icon("ai")
        .inputs(&[
            ParamSpec::new(
                "heading",
                K::Number,
                "degrees: 0 the rig's forward (-Y), positive to its left",
            )
            .range(-3600.0, 3600.0),
            ParamSpec::new("speed", K::Number, "m/s, 0 to stop (held to the targets' top speed)")
                .range(0.0, 100.0),
            ParamSpec::new("style", K::Text, "a target of the set to aim at instead of blending by speed"),
            ParamSpec::new("frames", K::Integer, "frames at 30 fps to move on (default 1)").range(0.0, MAX_STEP as f64),
            ParamSpec::new(
                "wait",
                K::Bool,
                "wait for each stretch (default); false lets the model work on a thread while the last stretch plays on",
            ),
        ])
        .outputs(INPUT_OUT),
    CommandSpec::new("live.stop", "Live motion: stop the take", live_stop)
        .icon("ai")
        .outputs(STOP_OUT),
    CommandSpec::new("live.show", "Live motion: look at a recorded frame", live_show)
        .icon("ai")
        .inputs(&[ParamSpec::new("frame", K::Integer, "the recording's frame to show")
            .range(0.0, 1e7)
            .required()])
        .outputs(SHOW_OUT),
    CommandSpec::new("live.clip", "Live motion: keep a range", live_clip)
        .icon("ai")
        .undoable()
        .inputs(&[
            ParamSpec::new("from", K::Integer, "the recording's first frame to keep (default 0)").range(0.0, 1e7),
            ParamSpec::new("to", K::Integer, "its last frame to keep (default the last)").range(0.0, 1e7),
            ParamSpec::new("at", K::Integer, "the animation's frame it starts on (default the playhead)").range(0.0, 1e7),
        ])
        .outputs(CLIP_OUT),
];

const START_OUT: &[ParamSpec] = &[
    ParamSpec::new("session", K::Integer, "the take's number").required(),
    ParamSpec::new("targets", K::TextList, "the target poses of the set").required(),
    ParamSpec::new("max_speed", K::Number, "the set's top speed, m/s").required(),
    ParamSpec::new("max_frames", K::Integer, "the most frames it records").required(),
];

const INPUT_OUT: &[ParamSpec] = &[
    ParamSpec::new(
        "state",
        K::Choice(&["live", "full"]),
        "full: the recording reached its cap and the character stands",
    )
    .required(),
    ParamSpec::new("frames", K::Integer, "frames recorded so far").required(),
    ParamSpec::new("calls", K::Integer, "stretches asked of the model so far").required(),
    ParamSpec::new("speed", K::Number, "the stick smoothed, m/s").required(),
    ParamSpec::new("target", K::Text, "what the last stretch aimed at").required(),
    ParamSpec::new("why", K::Text, "why the last stretch was asked for").required(),
    ParamSpec::new("root", K::Vec3, "the root offset on show").required(),
    ParamSpec::new("notes", K::TextList, "what to know").required(),
];

const STOP_OUT: &[ParamSpec] = &[
    ParamSpec::new("frames", K::Integer, "frames recorded; they stay for live.clip").required(),
    ParamSpec::new("calls", K::Integer, "stretches asked of the model").required(),
    ParamSpec::new(
        "held",
        K::Integer,
        "frames shown with no motion ready: the last pose held",
    )
    .required(),
    ParamSpec::new("late", K::Integer, "frames skipped because a stretch came late").required(),
    ParamSpec::new(
        "round_trip_ms",
        K::Number,
        "mean time a stretch took, helper round trip",
    )
    .required(),
    ParamSpec::new("round_trip_max_ms", K::Number, "the longest").required(),
    ParamSpec::new("model_ms", K::Number, "mean of the model's own time").required(),
];

const SHOW_OUT: &[ParamSpec] = &[
    ParamSpec::new("frame", K::Integer, "the frame on show").required(),
    ParamSpec::new("root", K::Vec3, "its root offset").required(),
];

const CLIP_OUT: &[ParamSpec] = &[
    ParamSpec::new("from", K::Integer, "the animation's first frame written").required(),
    ParamSpec::new("to", K::Integer, "its last").required(),
    ParamSpec::new("frames", K::Integer, "AI keys written between them").required(),
    ParamSpec::new("model", K::Text, "the keys' model tag").required(),
    ParamSpec::new("notes", K::TextList, "what to know").required(),
];

/// The take in progress or last ended, and the reply being waited for.
#[derive(Default)]
struct Live {
    take: Option<Take>,
    reply: Option<Receiver<Reply>>,
    /// Round trips and the model's own times, in ms.
    trips: Vec<f64>,
    model: Vec<f64>,
}

type Reply = Result<(Stretch, f64), String>;

static LIVE: Mutex<Option<Live>> = Mutex::new(None);

fn locked() -> MutexGuard<'static, Option<Live>> {
    LIVE.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A snapshot for a front end.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shown {
    pub running: bool,
    pub frames: usize,
    pub pose: Option<Pose>,
    pub ended: Option<String>,
}

/// How the take stands, and the pose on show.
pub fn shown() -> Shown {
    let guard = locked();
    let Some(t) = guard.as_ref().and_then(|l| l.take.as_ref()) else {
        return Shown::default();
    };
    Shown {
        running: matches!(t.state, State::Live | State::Full),
        frames: t.frames.len(),
        pose: t.pose().cloned(),
        ended: match &t.state {
            State::Ended(why) => why.clone(),
            _ => None,
        },
    }
}

/// Recorded frame `f`, for looking through a recording.
pub fn recorded(f: usize) -> Option<Pose> {
    locked().as_ref()?.take.as_ref()?.frames.get(f).cloned()
}

/// A take is over whatever the document does: the window ends it on a new session.
pub fn reset() {
    *locked() = None;
}

fn live_start(doc: &mut Document, p: &P) -> Out {
    let assets = doc
        .assets
        .as_ref()
        .ok_or("no assets folder is set (start the app with one)")?;
    let path = assets.join(TARGETS);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let targets: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let start = Pose {
        rotations: doc.pose.rotations.clone(),
        root: doc.pose.root,
    };
    let ask = LiveStart {
        rig: crate::plan::rig_path(assets)?,
        targets,
        start: start.clone(),
    };
    let begun = engine::live_start(&ask)?;
    let cap = p
        .get("max_frames")
        .and_then(Value::as_i64)
        .map_or(MAX_FRAMES, |n| {
            usize::try_from(n).unwrap_or(MAX_FRAMES).min(MAX_FRAMES)
        });
    let take = Take::new(begun.session, start, begun.max_speed, cap);
    let mut guard = locked();
    let live = guard.get_or_insert_with(Live::default);
    live.take = Some(take);
    live.reply = None;
    live.trips.clear();
    live.model.clear();
    Ok(json!({
        "session": begun.session, "targets": begun.targets, "max_speed": begun.max_speed,
        "max_frames": cap,
    }))
}

/// A reply that has come back goes into the take.
fn collect(live: &mut Live) {
    let Some(rx) = &live.reply else { return };
    let got = match rx.try_recv() {
        Ok(r) => r,
        Err(TryRecvError::Empty) => return,
        Err(TryRecvError::Disconnected) => Err("the model's thread stopped unexpectedly".to_string()),
    };
    live.reply = None;
    settle(live, got);
}

fn settle(live: &mut Live, got: Reply) {
    let Some(take) = &mut live.take else { return };
    match got {
        Ok((stretch, ms)) => {
            live.trips.push(ms);
            live.model.push(stretch.seconds * 1000.0);
            take.deliver(stretch);
        }
        Err(e) => take.fail(e),
    }
}

fn timed(ask: &StepAsk) -> Reply {
    let t0 = Instant::now();
    let stretch = engine::live_step(ask)?;
    Ok((stretch, t0.elapsed().as_secs_f64() * 1000.0))
}

/// Ask on a thread; the reply is collected on a later frame.
fn ask_off_thread(live: &mut Live, ask: StepAsk) -> Result<(), String> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("live-motion".into())
        .spawn(move || {
            // nobody is listening only if the take was dropped: nothing to tell
            let _ = tx.send(timed(&ask));
        })
        .map_err(|e| format!("can't start the model's thread: {e}"))?;
    live.reply = Some(rx);
    Ok(())
}

fn ended(frames: usize, why: Option<&str>) -> String {
    let kept = format!("the {frames} frames recorded are kept: live.clip takes a range of them");
    match why {
        Some(w) => format!("the take ended: {w}; {kept}"),
        None => format!("the take is stopped; {kept}"),
    }
}

fn live_input(doc: &mut Document, p: &P) -> Out {
    let num = |k: &str| p.get(k).and_then(Value::as_f64);
    let mut guard = locked();
    let live = guard
        .as_mut()
        .filter(|l| l.take.is_some())
        .ok_or_else(|| hinted("no take is running", "start one with live.start"))?;
    let frames = p
        .get("frames")
        .and_then(Value::as_i64)
        .unwrap_or(1)
        .clamp(0, MAX_STEP);
    let wait = p.get("wait").and_then(Value::as_bool).unwrap_or(true);
    let stick = Stick {
        heading: num("heading").unwrap_or(0.0),
        speed: num("speed").unwrap_or(0.0).max(0.0),
        style: p.get("style").and_then(Value::as_str).map(String::from),
    };
    for _ in 0..frames {
        collect(live);
        let Some(take) = &mut live.take else { break };
        if let Some(ask) = take.tick(&stick, if wait { 0 } else { LOOKAHEAD }) {
            if wait {
                let got = timed(&ask);
                settle(live, got);
            } else if let Err(e) = ask_off_thread(live, ask)
                && let Some(t) = &mut live.take
            {
                t.fail(e);
            }
        }
        if let Some(take) = &mut live.take {
            take.show();
        }
    }
    collect(live);
    let Some(take) = &mut live.take else {
        return Err("no take is running".into());
    };
    if let State::Ended(why) = &take.state {
        return Err(ended(take.frames.len(), why.as_deref()));
    }
    let root = take.pose().map_or([0.0; 3], |pose| pose.root);
    if let Some(pose) = take.pose() {
        doc.pose.rotations.clone_from(&pose.rotations);
        doc.pose.root = pose.root;
    }
    let notes = std::mem::take(&mut take.notes);
    Ok(json!({
        "state": if take.state == State::Full { "full" } else { "live" },
        "frames": take.frames.len(), "calls": take.calls, "speed": take.speed(),
        "target": take.target, "why": take.why, "root": root, "notes": notes,
    }))
}

fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        0.0
    } else {
        xs.iter().sum::<f64>() / xs.len() as f64
    }
}

fn live_show(doc: &mut Document, p: &P) -> Out {
    let f = p
        .get("frame")
        .and_then(Value::as_i64)
        .ok_or("frame: a frame of the recording")?;
    let guard = locked();
    let take = guard
        .as_ref()
        .and_then(|l| l.take.as_ref())
        .ok_or_else(|| hinted("there is no recording", "make one with live.start and live.input"))?;
    let last = take.frames.len() as i64 - 1;
    let pose = usize::try_from(f)
        .ok()
        .and_then(|i| take.frames.get(i))
        .ok_or_else(|| format!("frame {f} is not in the recording (frames 0 to {last})"))?;
    doc.pose.rotations.clone_from(&pose.rotations);
    doc.pose.root = pose.root;
    Ok(json!({ "frame": f, "root": pose.root }))
}

fn live_stop(doc: &mut Document, _: &P) -> Out {
    let shown = doc.frame;
    doc.go_to(shown)?;
    let mut guard = locked();
    let live = guard
        .as_mut()
        .filter(|l| l.take.is_some())
        .ok_or_else(|| hinted("no take is running", "start one with live.start"))?;
    live.reply = None;
    let Some(take) = &mut live.take else {
        return Err("no take is running".into());
    };
    if take.state == State::Ended(None) {
        return Err(hinted(
            "the take has already stopped",
            "start another with live.start",
        ));
    }
    take.stop();
    // with no keys `go_to` leaves the pose alone: the one the take started from is the animation's
    if doc.clip.base().keys.is_empty()
        && let Some(first) = take.frames.first()
    {
        doc.pose.rotations.clone_from(&first.rotations);
        doc.pose.root = first.root;
    }
    Ok(json!({
        "frames": take.frames.len(), "calls": take.calls, "held": take.held, "late": take.late,
        "round_trip_ms": mean(&live.trips),
        "round_trip_max_ms": live.trips.iter().copied().fold(0.0, f64::max),
        "model_ms": mean(&live.model),
    }))
}

/// The recording at the animation's rate: `from`..=`to` at 30 fps to `fps`, rotations slerped.
fn resampled(frames: &[Pose], fps: f64) -> Vec<Pose> {
    if (fps - MODEL_FPS).abs() < 1e-9 {
        return frames.to_vec();
    }
    let last = frames.len().saturating_sub(1);
    let n = ac_helper::frames::model_frames(last as i64 + 1, MODEL_FPS, fps).max(2);
    (0..n)
        .filter_map(|i| {
            let t = i as f64 * last as f64 / (n - 1) as f64;
            let at = (t.floor() as usize).min(last.saturating_sub(1));
            let (a, b) = (frames.get(at)?, frames.get(at + 1)?);
            let w = t - at as f64;
            Some(Pose {
                rotations: a
                    .rotations
                    .iter()
                    .zip(&b.rotations)
                    .map(|(x, y)| quat::slerp(*x, *y, w))
                    .collect(),
                root: std::array::from_fn(|c| a.root[c] + (b.root[c] - a.root[c]) * w),
            })
        })
        .collect()
}

fn live_clip(doc: &mut Document, p: &P) -> Out {
    whole_layer(&doc.clip)?;
    let kept = {
        let guard = locked();
        let take = guard
            .as_ref()
            .and_then(|l| l.take.as_ref())
            .ok_or_else(|| hinted("there is no recording", "make one with live.start and live.input"))?;
        if !matches!(take.state, State::Ended(_)) {
            return Err(hinted(
                "the take is still running",
                "stop it first with live.stop",
            ));
        }
        let last = take.frames.len() as i64 - 1;
        let from = p.get("from").and_then(Value::as_i64).unwrap_or(0);
        let to = p.get("to").and_then(Value::as_i64).unwrap_or(last);
        if from < 0 || to > last || from >= to {
            return Err(hinted(
                format!(
                    "the range {from}-{to} is not inside the recording (frames 0 to {last}) with at least 2 frames"
                ),
                "give from below to, both inside the recording",
            ));
        }
        take.frames
            .get(from as usize..=to as usize)
            .ok_or("the range is not inside the recording")?
            .to_vec()
    };
    let bones = doc.skeleton.names.len();
    if kept.iter().any(|f| f.rotations.len() != bones) {
        return Err(format!("the recording is not for this rig ({bones} bones)"));
    }
    let poses = resampled(&kept, doc.clip.fps);
    let at = p
        .get("at")
        .and_then(Value::as_i64)
        .unwrap_or_else(|| doc.key_frame())
        .max(0);
    let end = at + poses.len() as i64 - 1;
    let inside: Vec<i64> = doc
        .clip
        .keys()
        .iter()
        .filter(|k| k.source == KeySource::Manual && at < k.frame && k.frame <= end)
        .map(|k| k.frame)
        .collect();
    if let (Some(first), Some(last)) = (inside.first(), inside.last()) {
        return Err(hinted(
            format!("keys of yours lie on frames {first}-{last}, where the range would go ({at}-{end})"),
            "start it after your last key, keep a shorter range, or clip into a new animation",
        ));
    }
    let mut notes = Vec::new();
    let replaced = doc
        .clip
        .keys()
        .iter()
        .any(|k| k.frame == at && k.source == KeySource::Manual);
    if replaced {
        notes.push(format!("your key on frame {at} became the range's first pose"));
    }
    for (i, pose) in poses.iter().enumerate() {
        let f = at + i as i64;
        let mut k = Key::new(f, pose.rotations.clone(), pose.root, Interp::Smooth);
        if f != at && f != end {
            k.source = KeySource::Ai;
            k.model = edits::POSE_FILL.to_string();
        }
        doc.clip.set_key(k)?;
    }
    let shown = doc.frame;
    doc.go_to(shown)?;
    Ok(json!({
        "from": at, "to": end, "frames": end - at - 1, "model": TAG, "notes": notes,
    }))
}
