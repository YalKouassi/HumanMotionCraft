//! The posefill.* commands over a fake engine (the helper itself needs the install and an NVIDIA
//! GPU): what they ask for, what they write, what they refuse, and that a prepared fill is not
//! made twice.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use ac_core::clip::KeySource;
use ac_core::{Document, Session, Skeleton, edits};
use ac_posefill::{Engine, Fill, Info, Request, Status, engine};
use serde_json::{Value, json};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The engine is one per process: the tests take turns.
static TURN: Mutex<()> = Mutex::new(());

/// What the fake answers with, by the first target's root z.
const NAN_Z: f64 = 9.0;
const SHORT_Z: f64 = 8.0;

/// Holds the last context pose, the root rising 1 cm a frame from it; records each request.
struct Fake {
    calls: Arc<AtomicUsize>,
    asked: Arc<Mutex<Vec<Request>>>,
    bones_off_by: usize,
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
        Info {
            plan: "built".into(),
            gpu: "fake".into(),
        }
    }
    fn load(&mut self, _: &mut dyn FnMut(&str)) -> Result<(), String> {
        Ok(())
    }
    fn fill(&mut self, req: &Request, say: &mut dyn FnMut(&str)) -> Result<Fill, String> {
        say("filling");
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.asked.lock().unwrap().push(req.clone());
        let a = req.context.last().unwrap();
        let b = req.targets.first().unwrap();
        let mut rot = a.rotations.clone();
        rot.resize(rot.len() + self.bones_off_by, [0.0, 0.0, 0.0, 1.0]);
        let frames = if b.root[2] == SHORT_Z {
            req.frames - 1
        } else {
            req.frames
        };
        let z = |i: usize| {
            if b.root[2] == NAN_Z {
                f64::NAN
            } else {
                a.root[2] + 0.01 * i as f64
            }
        };
        Ok(Fill {
            rotations: vec![rot; frames],
            roots: (0..req.frames).map(|i| [a.root[0], a.root[1], z(i)]).collect(),
            notes: vec!["a note from the model".into()],
            seconds: 0.5,
        })
    }
}

struct Setup {
    s: Session,
    calls: Arc<AtomicUsize>,
    asked: Arc<Mutex<Vec<Request>>>,
    _turn: MutexGuard<'static, ()>,
}

fn setup(bones_off_by: usize) -> Setup {
    let turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
    let text = std::fs::read_to_string(repo().join("assets/rig/default_rig.json")).unwrap();
    let mut doc = Document::new(Skeleton::from_json(&text).unwrap());
    doc.assets = Some(repo().join("assets"));
    let mut s = Session::new(doc);
    assert!(ac_posefill::register(&mut s).is_empty());
    let calls = Arc::new(AtomicUsize::new(0));
    let asked = Arc::new(Mutex::new(Vec::new()));
    engine::use_engine(Box::new(Fake {
        calls: calls.clone(),
        asked: asked.clone(),
        bones_off_by,
    }));
    Setup {
        s,
        calls,
        asked,
        _turn: turn,
    }
}

/// Keys of yours on `frames`, each with its root raised to `z`.
fn keyed(s: &mut Session, frames: &[(i64, f64)]) {
    for &(f, z) in frames {
        s.run("frame.set", &json!({ "frame": f })).unwrap();
        s.run("pose.set_root", &json!({ "root": [0.0, 0.0, z] })).unwrap();
        s.run("key.set", &json!({})).unwrap();
    }
}

/// (frame, manual, root z, model) of every key.
fn keys(s: &Session) -> Vec<(i64, bool, f64, String)> {
    s.document
        .clip
        .keys()
        .iter()
        .map(|k| (k.frame, k.source == KeySource::Manual, k.root[2], k.model.clone()))
        .collect()
}

fn refused(s: &mut Session, id: &str, p: Value) -> String {
    let before = s.document.clip.clone();
    let e = s.run(id, &p).expect_err("refused").to_string();
    assert_eq!(s.document.clip, before, "{id} {p} changed the clip");
    e
}

fn notes(v: &Value) -> Vec<String> {
    v["notes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|n| n.as_str().map(String::from))
        .collect()
}

#[test]
fn a_fill_writes_tagged_keys_between_two_keys_in_one_undo_step() {
    let mut t = setup(0);
    keyed(&mut t.s, &[(0, 1.0), (30, 2.0)]);
    t.s.run("frame.set", &json!({ "frame": 12 })).unwrap();
    let out = t.s.run("posefill.fill", &json!({})).unwrap();
    assert_eq!(
        (
            out["from"].as_i64(),
            out["to"].as_i64(),
            out["frames"].as_i64(),
            out["calls"].as_i64()
        ),
        (Some(0), Some(30), Some(29), Some(1))
    );
    assert_eq!(out["model"], edits::POSE_FILL);
    assert_eq!(out["seconds"], 0.5);
    let n = notes(&out);
    assert!(n.contains(&"a note from the model".to_string()), "{n:?}");
    assert!(
        n.iter()
            .any(|n| n.starts_with("key 0 has fewer than 3 frames before it")),
        "the short context is said: {n:?}"
    );
    let req = t.asked.lock().unwrap()[0].clone();
    assert_eq!((req.frames, req.fps), (31, 30.0));
    assert!(req.rig.ends_with("default_rig.json"));
    assert_eq!(req.context.len(), 4, "the first pose repeated to make 4");
    assert!(req.context.iter().all(|p| p.root[2] == 1.0));
    assert_eq!(req.targets.len(), 1, "nothing lies after key B");
    assert_eq!(req.targets[0].root[2], 2.0);
    let k = keys(&t.s);
    assert_eq!(k.len(), 31);
    assert!(k[0].1 && k[30].1);
    assert!(
        k[1..30]
            .iter()
            .all(|(_, manual, _, m)| !manual && m == edits::POSE_FILL && edits::is_generated(m))
    );
    // the fill's frames 1..=29, not its ends: frame 1 is key A's root plus 1 cm
    assert!((k[1].2 - 1.01).abs() < 1e-9);

    // one undo step takes every AI key away, and redo brings them back
    t.s.run("edit.undo", &json!({})).unwrap();
    let after: Vec<i64> = keys(&t.s).iter().map(|k| k.0).collect();
    assert_eq!(after, vec![0, 30]);
    t.s.run("edit.redo", &json!({})).unwrap();
    assert_eq!(keys(&t.s), k);

    // an edit beside it leaves it (ac-core's guard) and says to make it again here
    t.s.run("frame.set", &json!({ "frame": 30 })).unwrap();
    t.s.run("pose.drag", &json!({ "bone": "wrist.L", "by": [0.0, -0.1, 0.1] }))
        .unwrap();
    let set = t.s.run("key.set", &json!({})).unwrap();
    assert!(set["notes"].to_string().contains("posefill.fill"), "{set}");
    assert_eq!(keys(&t.s)[1..30], k[1..30]);
}

#[test]
fn a_fill_with_frames_before_and_after_takes_them_as_context_and_targets() {
    let mut t = setup(0);
    keyed(&mut t.s, &[(0, 1.0), (10, 1.5), (40, 2.0), (70, 3.0)]);
    let out =
        t.s.run("posefill.fill", &json!({ "from": 10, "to": 40 }))
            .unwrap();
    assert_eq!(out["calls"], 1);
    assert!(
        !notes(&out).iter().any(|n| n.contains("repeated")),
        "{:?}",
        notes(&out)
    );
    let req = t.asked.lock().unwrap()[0].clone();
    // frames 7, 8, 9, 10 before and on key A (the spline between 0 and 10 rises to 1.5)
    assert_eq!(req.context.len(), 4);
    assert!((req.context[3].root[2] - 1.5).abs() < 1e-9);
    assert!(req.context[0].root[2] < req.context[3].root[2]);
    // key B and the 3 frames after it, which the animation goes on through
    assert_eq!(req.targets.len(), 4);
    assert!((req.targets[0].root[2] - 2.0).abs() < 1e-9);
    assert!(req.targets[3].root[2] > 2.0);
}

#[test]
fn a_long_stretch_chains_through_the_keys_inside_it() {
    let mut t = setup(0);
    keyed(&mut t.s, &[(0, 1.0), (30, 2.0), (60, 3.0), (90, 4.0)]);
    let out = t.s.run("posefill.fill", &json!({ "from": 0, "to": 90 })).unwrap();
    assert_eq!(
        (
            out["from"].as_i64(),
            out["to"].as_i64(),
            out["frames"].as_i64(),
            out["calls"].as_i64()
        ),
        (Some(0), Some(90), Some(87), Some(3))
    );
    assert_eq!(out["seconds"], 1.5);
    let asked = t.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 3);
    // the second call's context is the first fill's last 4 frames (the fake rises 1 cm a frame
    // from key A: frames 27..=30 of it are 1.27, 1.28, 1.29, 1.30)
    let ctx: Vec<f64> = asked[1].context.iter().map(|p| p.root[2]).collect();
    assert!(
        ctx.iter()
            .zip([1.27, 1.28, 1.29, 1.30])
            .all(|(a, b)| (a - b).abs() < 1e-9),
        "{ctx:?}"
    );
    // a middle key is the one target; the last key gets no frames after it
    assert_eq!(asked[1].targets.len(), 1);
    assert_eq!(asked[2].targets.len(), 1);
    // your keys stay yours; every frame between is a tagged AI key
    let k = keys(&t.s);
    assert_eq!(k.len(), 91);
    for f in [0, 30, 60, 90] {
        assert!(k[f].1, "frame {f} stays yours");
    }
    assert!(
        k.iter()
            .filter(|k| !k.1)
            .all(|(_, _, _, m)| m == edits::POSE_FILL)
    );
    // one undo step
    t.s.run("edit.undo", &json!({})).unwrap();
    assert_eq!(keys(&t.s).len(), 4);
}

#[test]
fn gaps_outside_the_models_range_are_refused_with_the_limits() {
    let mut t = setup(0);
    keyed(&mut t.s, &[(0, 1.0), (10, 1.2), (30, 2.0), (120, 3.0)]);
    let e = refused(&mut t.s, "posefill.fill", json!({ "from": 0, "to": 10 }));
    assert!(
        e.contains("the gap 0-10 is 10 frames (0.33 s at 30 fps): 11 frames at the model's 30 fps")
            && e.contains("17 to 57 frames (0.6 to 1.90 s) at least")
            && e.contains("ai.fill"),
        "{e}"
    );
    let e = refused(&mut t.s, "posefill.fill", json!({ "from": 30, "to": 120 }));
    assert!(
        e.contains("the gap 30-120 is 90 frames (3.00 s at 30 fps): 91 frames at the model's 30 fps")
            && e.contains("at most")
            && e.contains("put a key inside the gap"),
        "{e}"
    );
    // a chain is refused at its first bad gap, before anything is made
    let e = refused(&mut t.s, "posefill.fill", json!({ "from": 0, "to": 120 }));
    assert!(e.contains("the gap 0-10"), "{e}");
    let e = refused(&mut t.s, "posefill.fill", json!({ "from": 10, "to": 120 }));
    assert!(e.contains("the gap 30-120"), "{e}");
    assert_eq!(t.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn the_animations_frame_rate_is_converted_to_the_models() {
    let mut t = setup(0);
    t.s.document.clip.fps = 60.0;
    keyed(&mut t.s, &[(0, 1.0), (60, 2.0), (180, 3.0)]);
    // 60 frames at 60 fps are 31 model frames: fine, and the model is asked at the clip's rate
    let out = t.s.run("posefill.fill", &json!({ "from": 0, "to": 60 })).unwrap();
    assert_eq!(out["frames"], 59);
    let req = t.asked.lock().unwrap()[0].clone();
    assert_eq!((req.frames, req.fps), (61, 60.0));
    // 120 frames at 60 fps are 61 model frames: over 58
    let e = refused(&mut t.s, "posefill.fill", json!({ "from": 60, "to": 180 }));
    assert!(
        e.contains("120 frames (2.00 s at 60 fps): 61 frames at the model's 30 fps")
            && e.contains("34 to 114 frames (0.6 to 1.90 s) at most"),
        "{e}"
    );
}

#[test]
fn what_is_not_a_gap_between_your_keys_is_refused() {
    let mut t = setup(0);
    keyed(&mut t.s, &[(0, 1.0), (30, 2.0)]);
    let e = refused(&mut t.s, "posefill.fill", json!({ "from": 0 }));
    assert!(e.contains("both"), "{e}");
    let e = refused(&mut t.s, "posefill.fill", json!({ "from": 0, "to": 5 }));
    assert!(e.contains("not both keys of yours"), "{e}");
    let e = refused(&mut t.s, "posefill.fill", json!({ "from": 30, "to": 0 }));
    assert!(e.contains("must come before"), "{e}");
    t.s.run("frame.set", &json!({ "frame": 40 })).unwrap();
    let e = refused(&mut t.s, "posefill.fill", json!({}));
    assert!(e.contains("playhead between two of your keys"), "{e}");
    assert_eq!(t.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn bad_helper_output_is_refused_and_nothing_is_written() {
    let mut t = setup(0);
    keyed(&mut t.s, &[(0, 1.0), (30, NAN_Z), (60, SHORT_Z)]);
    let e = refused(&mut t.s, "posefill.fill", json!({ "from": 0, "to": 30 }));
    assert!(e.contains("not finite"), "{e}");
    let e = refused(&mut t.s, "posefill.fill", json!({ "from": 30, "to": 60 }));
    assert!(
        e.contains("30 frames of rotations and 31 roots; 31 were asked for"),
        "{e}"
    );
    // a chain whose second call fails leaves the first's keys unwritten too
    let e = refused(&mut t.s, "posefill.fill", json!({ "from": 0, "to": 60 }));
    assert!(e.contains("not finite"), "{e}");
    assert_eq!(keys(&t.s).len(), 3);
}

#[test]
fn a_fill_for_another_rig_is_refused() {
    let mut t = setup(3);
    keyed(&mut t.s, &[(0, 1.0), (30, 2.0)]);
    let e = refused(&mut t.s, "posefill.fill", json!({ "from": 0, "to": 30 }));
    assert!(e.contains("104 bones"), "{e}");
}

#[test]
fn a_prepared_fill_is_made_once_and_a_changed_animation_is_noticed() {
    let mut t = setup(0);
    keyed(&mut t.s, &[(0, 1.0), (30, 2.0), (60, 3.0)]);
    let p = json!({ "from": 0, "to": 60 });
    let p = p.as_object().unwrap();
    assert!(ac_posefill::waits(&t.s.document, "posefill.fill", p));
    // the window: make it off the UI thread, then run the command
    ac_posefill::prepare(&t.s.document, "posefill.fill", p).unwrap();
    assert_eq!(t.calls.load(Ordering::SeqCst), 2);
    assert!(!ac_posefill::waits(&t.s.document, "posefill.fill", p));
    let first = t.s.run("posefill.fill", &Value::Object(p.clone())).unwrap();
    t.s.run("edit.undo", &json!({})).unwrap();
    let again = t.s.run("posefill.fill", &Value::Object(p.clone())).unwrap();
    assert_eq!(first, again);
    assert_eq!(
        t.calls.load(Ordering::SeqCst),
        2,
        "the same requests are not made twice"
    );
    // a key moved makes it another request
    t.s.run("edit.undo", &json!({})).unwrap();
    keyed(&mut t.s, &[(30, 2.5)]);
    assert!(ac_posefill::waits(&t.s.document, "posefill.fill", p));
    // a refused plan never waits
    assert!(!ac_posefill::waits(
        &t.s.document,
        "posefill.fill",
        json!({ "from": 0, "to": 5 }).as_object().unwrap()
    ));
}

#[test]
fn the_helper_says_what_it_runs() {
    let mut t = setup(0);
    let out = t.s.run("posefill.helper", &json!({})).unwrap();
    assert_eq!(
        (
            out["running"].as_bool(),
            out["loaded"].as_bool(),
            out["busy"].as_bool()
        ),
        (Some(true), Some(true), Some(false))
    );
    assert_eq!(out["model"], "fake");
    assert_eq!(out["plan"], "built");
    assert_eq!(out["gpu"], "fake");
    let e = refused(&mut t.s, "posefill.helper", json!({ "action": "fly" }));
    assert!(!e.is_empty());
}

#[test]
fn a_missing_install_says_how_to_install_it() {
    let dir = std::env::temp_dir().join(format!("posefill-none-{}", std::process::id()));
    let e = ac_posefill::install::read(&dir).unwrap_err();
    assert!(
        e.contains("pose fill is not installed")
            && e.contains("setup_posefill.py")
            && e.contains("NVIDIA GPU"),
        "{e}"
    );
    assert!(!ac_posefill::install::plan_built(&dir));
}

#[test]
fn a_helper_result_reads_as_a_fill() {
    let v = json!({ "rotations": [[[0, 0, 0, 1]], [[0, 0, 0.5, 0.5]]], "roots": [[0, 0, 1], [0, 0, 1.5]], "notes": ["n"], "seconds": 0.004 });
    let f = ac_posefill::helper::fill(&v).unwrap();
    assert_eq!(f.rotations.len(), 2);
    assert_eq!(f.roots[1], [0.0, 0.0, 1.5]);
    assert_eq!(f.notes, vec!["n".to_string()]);
    assert_eq!(f.seconds, 0.004);
    assert!(ac_posefill::helper::fill(&json!({ "rotations": [[[0, 0, 1]]], "roots": [[0, 0, 1]] })).is_err());
}

#[test]
fn the_licences_are_listed_for_the_panel() {
    let names: Vec<&str> = ac_posefill::setup::LICENCES.iter().map(|l| l.licence).collect();
    assert!(names.contains(&"NVIDIA Open Model Agreement"));
    assert!(names.contains(&"Apache License 2.0"));
    assert!(
        ac_posefill::setup::LICENCES
            .iter()
            .all(|l| l.url.starts_with("https://"))
    );
}
