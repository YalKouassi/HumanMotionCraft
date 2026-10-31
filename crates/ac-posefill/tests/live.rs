//! The live.* commands over a fake engine (the helper itself needs the install and an NVIDIA GPU):
//! a scripted take, a clip of a range in one undo step, the cap, refused ranges, and a helper that
//! dies mid-take with the recording kept.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use ac_core::clip::KeySource;
use ac_core::{Document, Session, Skeleton};
use ac_posefill::engine::{self, LiveBegun, LiveStart};
use ac_posefill::live::{MOVING_TOKENS, StepAsk, Stretch};
use ac_posefill::live_commands;
use ac_posefill::{Engine, Fill, Info, Request, Status};
use serde_json::{Value, json};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The engine and the take are one per process: the tests take turns.
static TURN: Mutex<()> = Mutex::new(());

/// Holds the start pose and moves the root's x by one a frame, 30 frames a stretch.
#[derive(Clone, Default)]
struct Fake {
    asks: Arc<Mutex<Vec<StepAsk>>>,
    start: Arc<Mutex<Option<LiveStart>>>,
    x: Arc<Mutex<f64>>,
    die_on_call: Option<usize>,
    delay: Duration,
}

impl Engine for Fake {
    fn status(&self) -> Status {
        Status {
            running: true,
            loaded: true,
            model: "fake".into(),
        }
    }
    fn info(&self) -> Info {
        Info::default()
    }
    fn load(&mut self, _: &mut dyn FnMut(&str)) -> Result<(), String> {
        Ok(())
    }
    fn fill(&mut self, _: &Request, _: &mut dyn FnMut(&str)) -> Result<Fill, String> {
        Err("not asked".into())
    }
    fn live_start(&mut self, req: &LiveStart, _: &mut dyn FnMut(&str)) -> Result<LiveBegun, String> {
        *self.start.lock().unwrap() = Some(req.clone());
        Ok(LiveBegun {
            session: 7,
            targets: vec!["stand".into(), "walk".into()],
            max_speed: 3.0,
        })
    }
    fn live_step(&mut self, ask: &StepAsk, _: &mut dyn FnMut(&str)) -> Result<Stretch, String> {
        std::thread::sleep(self.delay);
        let mut asks = self.asks.lock().unwrap();
        asks.push(ask.clone());
        if self.die_on_call == Some(asks.len()) {
            return Err("the helper ended".into());
        }
        let start = self.start.lock().unwrap().clone().unwrap().start;
        let mut x = self.x.lock().unwrap();
        *x += ask.shown as f64;
        let base = *x;
        Ok(Stretch {
            rotations: vec![start.rotations; 30],
            roots: (0..30).map(|i| [base + i as f64, 0.0, start.root[2]]).collect(),
            transition: 19,
            target: "walk".into(),
            seconds: 0.005,
            notes: vec![],
        })
    }
}

/// An engine from before live motion: the trait's own refusal.
struct Old;

impl Engine for Old {
    fn status(&self) -> Status {
        Status::default()
    }
    fn info(&self) -> Info {
        Info::default()
    }
    fn load(&mut self, _: &mut dyn FnMut(&str)) -> Result<(), String> {
        Ok(())
    }
    fn fill(&mut self, _: &Request, _: &mut dyn FnMut(&str)) -> Result<Fill, String> {
        Err("not asked".into())
    }
}

fn session() -> Session {
    let text = std::fs::read_to_string(repo().join("assets/rig/default_rig.json")).unwrap();
    let mut doc = Document::new(Skeleton::from_json(&text).unwrap());
    doc.assets = Some(repo().join("assets"));
    let mut s = Session::new(doc);
    assert!(ac_posefill::register(&mut s).is_empty());
    s
}

fn setup(fake: Fake) -> (MutexGuard<'static, ()>, Session) {
    let guard = TURN.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    live_commands::reset();
    engine::use_engine(Box::new(fake));
    (guard, session())
}

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    s.run(id, &p).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn refused(s: &mut Session, id: &str, p: Value) -> String {
    match s.run(id, &p) {
        Ok(v) => panic!("{id} should refuse, gave {v}"),
        Err(e) => e.to_string(),
    }
}

/// A take: start, `frames` at the stick, stop.
fn take(s: &mut Session, frames: i64) {
    run(s, "live.start", json!({}));
    run(
        s,
        "live.input",
        json!({ "heading": 0, "speed": 1.0, "frames": frames }),
    );
    run(s, "live.stop", json!({}));
}

#[test]
fn a_take_starts_from_the_pose_on_show_and_sends_the_shipped_targets() {
    let fake = Fake::default();
    let (_g, mut s) = setup(fake.clone());
    run(&mut s, "pose.set_root", json!({ "root": [0.25, 0.0, 1.0] }));
    let out = run(&mut s, "live.start", json!({}));
    assert_eq!(out["session"], 7);
    assert_eq!(out["max_speed"], 3.0);
    let start = fake.start.lock().unwrap().clone().unwrap();
    assert_eq!(start.start.root, [0.25, 0.0, 1.0]);
    assert_eq!(start.targets["format"], "humanmotioncraft.posetargets");
    assert!(start.rig.ends_with("default_rig.json"));
}

#[test]
fn a_scripted_take_asks_when_it_should_and_records_what_it_showed() {
    let fake = Fake::default();
    let (_g, mut s) = setup(fake.clone());
    run(&mut s, "live.start", json!({}));
    let one = run(&mut s, "live.input", json!({ "frames": 3 }));
    assert_eq!(
        (&one["frames"], &one["calls"], &one["why"]),
        (&json!(4), &json!(1), &json!("start"))
    );
    assert_eq!(one["root"][0], 3.0, "stretch frames 1 to 3");

    let two = run(&mut s, "live.input", json!({ "heading": 90, "speed": 1.0 }));
    assert_eq!((&two["calls"], &two["why"]), (&json!(2), &json!("started")));
    assert_eq!(two["root"][0], 4.0, "on from frame 3, with no jump");

    // nineteen frames of that stretch, then the next asks with them all shown
    run(&mut s, "live.input", json!({ "heading": 0, "speed": 0.0 }));
    let stop = run(&mut s, "live.stop", json!({}));
    assert_eq!(stop["frames"], 6);
    assert_eq!((&stop["held"], &stop["late"]), (&json!(0), &json!(0)));
    assert!(stop["model_ms"].as_f64().unwrap() > 4.9);

    let asks = fake.asks.lock().unwrap();
    assert_eq!(asks.len(), 3);
    assert_eq!(asks[0].shown, 0);
    assert_eq!(asks[0].tokens, None);
    assert_eq!(asks[1].shown, 3);
    assert_eq!(asks[1].tokens.as_deref(), Some(MOVING_TOKENS));
    // heading 90 is to the rig's left, +X, smoothed from forward, -Y
    assert!(
        asks[1].velocity[0] > 0.0 && asks[1].velocity[1] < 0.0,
        "{:?}",
        asks[1].velocity
    );
    assert_eq!(asks[2].velocity, [0.0, 0.0], "letting go asks for a stop");
    assert!(asks.iter().all(|a| a.session == 7));
}

#[test]
fn a_clip_of_a_range_is_one_undo_step_with_ai_keys_between_keys_of_yours() {
    let (_g, mut s) = setup(Fake::default());
    take(&mut s, 10);
    let before = s.document.clip.keys().len();
    let out = run(&mut s, "live.clip", json!({ "from": 2, "to": 8, "at": 20 }));
    assert_eq!(
        (&out["from"], &out["to"], &out["frames"]),
        (&json!(20), &json!(26), &json!(5))
    );
    assert_eq!(out["model"], ac_core::edits::POSE_FILL);
    let keys = s.document.clip.keys();
    let k = |f: i64| keys.iter().find(|k| k.frame == f).unwrap();
    assert_eq!(k(20).source, KeySource::Manual);
    assert_eq!(k(26).source, KeySource::Manual);
    assert_eq!(
        (k(23).source, k(23).model.as_str()),
        (KeySource::Ai, ac_core::edits::POSE_FILL)
    );
    assert_eq!(k(20).root[0], 2.0);
    assert_eq!(k(26).root[0], 8.0);
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.document.clip.keys().len(), before);
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(s.document.clip.keys().len(), before + 7);
}

#[test]
fn clipping_into_a_new_animation_in_one_batch_undoes_as_one_step() {
    let (_g, mut s) = setup(Fake::default());
    run(&mut s, "live.start", json!({}));
    run(&mut s, "live.input", json!({ "frames": 10 }));
    run(&mut s, "live.stop", json!({}));
    // clip.new is in ac-io; here the same thing on an empty clip: the batch is the session's
    let out = run(
        &mut s,
        "edit.batch",
        json!({ "commands": [
            { "command": "live.clip", "params": { "from": 0, "to": 5, "at": 0 } },
            { "command": "live.clip", "params": { "from": 5, "to": 10, "at": 5 } }
        ] }),
    );
    assert!(out.is_object());
    assert_eq!(s.document.clip.keys().len(), 11);
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.document.clip.keys().len(), 0);
}

#[test]
fn a_clip_converts_the_recording_to_the_animations_rate() {
    let (_g, mut s) = setup(Fake::default());
    take(&mut s, 30);
    s.document.clip.fps = 60.0;
    let out = run(&mut s, "live.clip", json!({ "from": 0, "to": 10, "at": 0 }));
    // 10 frames at 30 fps are 20 at 60, the ends included 21
    assert_eq!(out["to"], 20);
    let keys = s.document.clip.keys();
    let k = keys.iter().find(|k| k.frame == 3).unwrap();
    assert!((k.root[0] - 1.5).abs() < 1e-9, "{:?}", k.root);
}

#[test]
fn bad_ranges_and_keys_in_the_way_are_refused_and_change_nothing() {
    let (_g, mut s) = setup(Fake::default());
    assert!(refused(&mut s, "live.clip", json!({})).contains("no recording"));
    assert!(refused(&mut s, "live.input", json!({})).contains("no take"));
    assert!(refused(&mut s, "live.stop", json!({})).contains("no take"));
    run(&mut s, "live.start", json!({}));
    run(&mut s, "live.input", json!({ "frames": 5 }));
    assert!(refused(&mut s, "live.clip", json!({})).contains("still running"));
    run(&mut s, "live.stop", json!({}));
    assert!(refused(&mut s, "live.clip", json!({ "from": 3, "to": 3 })).contains("not inside"));
    assert!(refused(&mut s, "live.clip", json!({ "from": 4, "to": 2 })).contains("not inside"));
    assert!(refused(&mut s, "live.clip", json!({ "to": 6 })).contains("frames 0 to 5"));
    run(&mut s, "frame.set", json!({ "frame": 3 }));
    run(&mut s, "key.set", json!({}));
    assert!(refused(&mut s, "live.clip", json!({ "at": 0 })).contains("frames 3-3"));
    assert_eq!(s.document.clip.keys().len(), 1);
}

#[test]
fn a_clip_starting_on_your_key_says_it_became_the_ranges_first_pose() {
    let (_g, mut s) = setup(Fake::default());
    run(&mut s, "key.set", json!({}));
    take(&mut s, 5);
    let out = run(&mut s, "live.clip", json!({ "at": 0 }));
    assert_eq!(
        out["notes"][0],
        "your key on frame 0 became the range's first pose"
    );
}

#[test]
fn the_cap_stops_the_character_and_keeps_the_recording() {
    let (_g, mut s) = setup(Fake::default());
    assert_eq!(
        run(&mut s, "live.start", json!({ "max_frames": 5 }))["max_frames"],
        5
    );
    let out = run(&mut s, "live.input", json!({ "frames": 20 }));
    assert_eq!((&out["state"], &out["frames"]), (&json!("full"), &json!(5)));
    assert!(out["notes"][0].as_str().unwrap().contains("recording is full"));
    let again = run(&mut s, "live.input", json!({ "frames": 20 }));
    assert_eq!(again["frames"], 5);
    assert_eq!(again["notes"], json!([]), "said once");
    assert_eq!(run(&mut s, "live.stop", json!({}))["frames"], 5);
    assert_eq!(run(&mut s, "live.clip", json!({ "at": 0 }))["to"], 4);
}

#[test]
fn a_helper_that_dies_mid_take_says_so_and_the_recording_stays() {
    let fake = Fake {
        die_on_call: Some(2),
        ..Fake::default()
    };
    let (_g, mut s) = setup(fake);
    run(&mut s, "live.start", json!({}));
    run(&mut s, "live.input", json!({ "frames": 10 }));
    let said = refused(&mut s, "live.input", json!({ "speed": 1.0 }));
    assert!(
        said.contains("the helper ended") && said.contains("the 11 frames recorded are kept"),
        "{said}"
    );
    assert!(refused(&mut s, "live.input", json!({ "speed": 1.0 })).contains("kept"));
    let stop = run(&mut s, "live.stop", json!({}));
    assert_eq!(stop["frames"], 11);
    assert_eq!(run(&mut s, "live.clip", json!({ "at": 0 }))["to"], 10);
    assert_eq!(live_commands::shown().ended.as_deref(), Some("the helper ended"));
}

#[test]
fn the_window_never_waits_and_the_stretch_arrives_while_the_start_pose_holds() {
    let fake = Fake {
        delay: Duration::from_millis(40),
        ..Fake::default()
    };
    let (_g, mut s) = setup(fake);
    run(&mut s, "live.start", json!({}));
    let t0 = Instant::now();
    let out = run(&mut s, "live.input", json!({ "wait": false }));
    assert!(t0.elapsed() < Duration::from_millis(30), "{:?}", t0.elapsed());
    assert_eq!(out["root"][0], 0.0, "nothing back yet: the start pose holds");
    while t0.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(5));
        let out = run(&mut s, "live.input", json!({ "wait": false, "frames": 0 }));
        if out["target"] == "walk" {
            let on = run(&mut s, "live.input", json!({ "wait": false }));
            assert_eq!(on["root"][0], 1.0);
            let stop = run(&mut s, "live.stop", json!({}));
            assert!(stop["round_trip_ms"].as_f64().unwrap() >= 39.0);
            assert_eq!(stop["held"], 1);
            return;
        }
    }
    panic!("the stretch never came back");
}

#[test]
fn an_engine_without_live_motion_refuses() {
    let _g = TURN.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    live_commands::reset();
    engine::use_engine(Box::new(Old));
    let mut s = session();
    assert!(refused(&mut s, "live.start", json!({})).contains("no live motion"));
    assert!(!live_commands::shown().running);
}

#[test]
fn steering_shows_the_pose_and_stopping_puts_the_animations_pose_back() {
    let (_g, mut s) = setup(Fake::default());
    let start = s.document.pose.root;
    run(&mut s, "live.start", json!({}));
    run(&mut s, "live.input", json!({ "frames": 4 }));
    assert_eq!(s.document.pose.root[0], 4.0);
    assert!(s.document.clip.keys().is_empty(), "nothing is keyed");
    assert!(!s.can_undo(), "nothing to undo");
    run(&mut s, "live.stop", json!({}));
    assert_eq!(s.document.pose.root, start);
    assert_eq!(run(&mut s, "live.show", json!({ "frame": 2 }))["root"][0], 2.0);
    assert_eq!(s.document.pose.root[0], 2.0);
    assert!(refused(&mut s, "live.show", json!({ "frame": 5 })).contains("frames 0 to 4"));
}

#[test]
fn a_new_take_replaces_the_last_recording() {
    let (_g, mut s) = setup(Fake::default());
    take(&mut s, 5);
    assert_eq!(live_commands::recorded(5).unwrap().root[0], 5.0);
    assert!(live_commands::recorded(6).is_none());
    run(&mut s, "live.start", json!({}));
    assert_eq!(live_commands::shown().frames, 1);
    assert!(live_commands::shown().running);
    assert!(live_commands::recorded(1).is_none());
}
