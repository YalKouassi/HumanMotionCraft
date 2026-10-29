//! The text.* commands over a fake engine (the helper itself needs the install): what they ask
//! for, what they write, what they refuse, and that a prepared motion is not made twice.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use ac_core::clip::KeySource;
use ac_core::{Document, Session, Skeleton, edits};
use ac_textmotion::{Engine, Motion, Request, Status, engine};
use serde_json::{Value, json};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The engine is one per process: the tests take turns.
static TURN: Mutex<()> = Mutex::new(());

/// Makes the start pose (or rest) with the root rising 1 cm a frame, and records each request.
struct Fake {
    calls: Arc<AtomicUsize>,
    asked: Arc<Mutex<Vec<Request>>>,
    bones: usize,
}

impl Engine for Fake {
    fn status(&self) -> Status {
        Status {
            running: true,
            loaded: true,
            model: "fake".into(),
        }
    }
    fn load(&mut self, _: &mut dyn FnMut(&str)) -> Result<(), String> {
        Ok(())
    }
    fn generate(&mut self, req: &Request, say: &mut dyn FnMut(&str)) -> Result<Motion, String> {
        say("generating");
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.asked.lock().unwrap().push(req.clone());
        let (rot, root) = match &req.start {
            Some(h) => (h.rotations.clone(), h.root),
            None => (vec![[0.0, 0.0, 0.0, 1.0]; self.bones], [0.0, 0.0, 1.0]),
        };
        Ok(Motion {
            rotations: vec![rot; req.frames],
            roots: (0..req.frames)
                .map(|i| [root[0], root[1], root[2] + 0.01 * i as f64])
                .collect(),
            notes: vec!["a note from the model".into()],
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
    let bones = doc.skeleton.names.len() + bones_off_by;
    let mut s = Session::new(doc);
    assert!(ac_textmotion::register(&mut s).is_empty());
    let calls = Arc::new(AtomicUsize::new(0));
    let asked = Arc::new(Mutex::new(Vec::new()));
    engine::use_engine(Box::new(Fake {
        calls: calls.clone(),
        asked: asked.clone(),
        bones,
    }));
    Setup {
        s,
        calls,
        asked,
        _turn: turn,
    }
}

fn keyed(s: &mut Session, frames: &[i64]) {
    for &f in frames {
        s.run("frame.set", &json!({ "frame": f })).unwrap();
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

#[test]
fn generate_on_an_empty_animation_writes_keys_at_both_ends_and_text_keys_between() {
    let mut t = setup(0);
    let out =
        t.s.run(
            "text.generate",
            &json!({ "prompt": "  A person\nwaves.  ", "seconds": 2 }),
        )
        .unwrap();
    // 2 s at 30 fps: frames 0..=60, keys of yours on 0 and 60, 59 AI keys between
    assert_eq!(
        (out["from"].as_i64(), out["to"].as_i64(), out["frames"].as_i64()),
        (Some(0), Some(60), Some(59))
    );
    assert_eq!(out["model"], "text A person waves.");
    let notes: Vec<&str> = out["notes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(notes.contains(&"a note from the model"), "{notes:?}");
    assert!(notes.iter().any(|n| n.contains("no keys")), "{notes:?}");
    let k = keys(&t.s);
    assert_eq!(k.len(), 61);
    assert!(k[0].1 && k[60].1);
    assert!(
        k[1..60]
            .iter()
            .all(|(_, manual, _, m)| !manual && edits::is_generated(m))
    );
    assert!(
        (k[60].2 - (1.0 + 0.6)).abs() < 1e-9,
        "the last key is the motion's last frame"
    );
    let req = &t.asked.lock().unwrap()[0];
    assert_eq!((req.frames, req.fps, req.seed), (61, 30.0, 1));
    assert!(req.start.is_none() && req.end.is_none());
    assert!(req.rig.ends_with("default_rig.json"));
}

#[test]
fn generate_after_keys_starts_from_the_pose_there_and_never_overwrites_a_key() {
    let mut t = setup(0);
    keyed(&mut t.s, &[0, 30]);
    let z = t.s.document.clip.keys()[1].root[2];
    // a key lies in 0..=120: refused, nothing changes, nothing is made
    let e = refused(
        &mut t.s,
        "text.generate",
        json!({ "prompt": "A person jumps.", "at": 0 }),
    );
    assert!(
        e.contains("a key lies on frame 30, where the motion would go (0-120)") && e.contains("text.fill"),
        "{e}"
    );
    assert_eq!(t.calls.load(Ordering::SeqCst), 0);

    let out =
        t.s.run(
            "text.generate",
            &json!({ "prompt": "A person jumps.", "at": 30, "seconds": 1, "seed": 7 }),
        )
        .unwrap();
    assert_eq!(
        (out["from"].as_i64(), out["to"].as_i64(), out["frames"].as_i64()),
        (Some(30), Some(60), Some(29))
    );
    let req = &t.asked.lock().unwrap()[0];
    assert_eq!(req.seed, 7);
    assert_eq!(
        req.start.as_ref().unwrap().root[2],
        z,
        "starts from the pose on frame 30"
    );
    let k = keys(&t.s);
    let at30 = k.iter().find(|k| k.0 == 30).unwrap();
    assert!(at30.1 && at30.3.is_empty(), "your key on 30 stays yours");
    assert!(k.iter().find(|k| k.0 == 60).unwrap().1);
}

#[test]
fn fill_writes_text_keys_between_two_keys_held_to_both() {
    let mut t = setup(0);
    keyed(&mut t.s, &[0, 30]);
    t.s.run("frame.set", &json!({ "frame": 12 })).unwrap();
    let out =
        t.s.run("text.fill", &json!({ "prompt": "A person waves." }))
            .unwrap();
    assert_eq!(
        (out["from"].as_i64(), out["to"].as_i64(), out["frames"].as_i64()),
        (Some(0), Some(30), Some(29))
    );
    let req = t.asked.lock().unwrap()[0].clone();
    assert_eq!(req.frames, 31);
    assert!(req.start.is_some() && req.end.is_some());
    let k = keys(&t.s);
    assert_eq!(k.len(), 31);
    assert!(
        k[1..30]
            .iter()
            .all(|(_, manual, _, m)| !manual && m == "text A person waves.")
    );
    // the motion's frames 1..=29, not its ends: frame 1 is the start's root plus 1 cm
    assert!((k[1].2 - (req.start.as_ref().unwrap().root[2] + 0.01)).abs() < 1e-9);

    // an edit beside it leaves it (ac-core's guard) and says to make it again here
    t.s.run("frame.set", &json!({ "frame": 30 })).unwrap();
    t.s.run("pose.drag", &json!({ "bone": "wrist.L", "by": [0.0, -0.1, 0.1] }))
        .unwrap();
    let set = t.s.run("key.set", &json!({})).unwrap();
    assert!(set["notes"].to_string().contains("text.fill"), "{set}");
    assert_eq!(keys(&t.s)[1..30], k[1..30]);
}

#[test]
fn fill_refuses_what_it_cannot_take() {
    let mut t = setup(0);
    keyed(&mut t.s, &[0, 1, 400]);
    let e = refused(
        &mut t.s,
        "text.fill",
        json!({ "prompt": "A person waves.", "from": 1, "to": 400 }),
    );
    assert!(e.contains("13.3 s") && e.contains("10.0 s"), "{e}");
    let e = refused(
        &mut t.s,
        "text.fill",
        json!({ "prompt": "A person waves.", "from": 0, "to": 1 }),
    );
    assert!(e.contains("no frames between"), "{e}");
    let e = refused(
        &mut t.s,
        "text.fill",
        json!({ "prompt": "A person waves.", "from": 0, "to": 400 }),
    );
    assert!(e.contains("not neighbouring"), "{e}");
    let e = refused(
        &mut t.s,
        "text.fill",
        json!({ "prompt": "A person waves.", "from": 0 }),
    );
    assert!(e.contains("both"), "{e}");
    let e = refused(
        &mut t.s,
        "text.fill",
        json!({ "prompt": " \n ", "from": 0, "to": 1 }),
    );
    assert!(e.contains("empty"), "{e}");
    assert_eq!(t.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn motion_for_another_rig_is_refused() {
    let mut t = setup(3);
    let e = refused(&mut t.s, "text.generate", json!({ "prompt": "A person waves." }));
    assert!(e.contains("not for this rig"), "{e}");
}

#[test]
fn a_prepared_motion_is_made_once() {
    let mut t = setup(0);
    keyed(&mut t.s, &[0, 30]);
    let p = json!({ "prompt": "A person waves.", "from": 0, "to": 30 });
    let p = p.as_object().unwrap();
    // the window: make it off the UI thread, then run the command
    ac_textmotion::prepare(&t.s.document, "text.fill", p).unwrap();
    assert_eq!(t.calls.load(Ordering::SeqCst), 1);
    let first = t.s.run("text.fill", &Value::Object(p.clone())).unwrap();
    t.s.run("edit.undo", &json!({})).unwrap();
    let again = t.s.run("text.fill", &Value::Object(p.clone())).unwrap();
    assert_eq!(first, again);
    assert_eq!(
        t.calls.load(Ordering::SeqCst),
        1,
        "the same request is not made twice"
    );
    // another seed is another request
    let mut q = p.clone();
    q.insert("seed".into(), json!(2));
    t.s.run("edit.undo", &json!({})).unwrap();
    t.s.run("text.fill", &Value::Object(q)).unwrap();
    assert_eq!(t.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn the_helper_says_what_it_runs() {
    let mut t = setup(0);
    let out = t.s.run("text.helper", &json!({})).unwrap();
    assert_eq!(
        (
            out["running"].as_bool(),
            out["loaded"].as_bool(),
            out["busy"].as_bool()
        ),
        (Some(true), Some(true), Some(false))
    );
    assert_eq!(out["model"], "fake");
}

#[test]
fn a_missing_install_says_how_to_install_it() {
    let dir = std::env::temp_dir().join(format!("textmotion-none-{}", std::process::id()));
    let e = ac_textmotion::install::read(&dir).unwrap_err();
    assert!(
        e.contains("not installed") && e.contains("setup_textmotion.py"),
        "{e}"
    );
}

#[test]
fn a_helper_result_reads_as_motion() {
    let v = json!({ "rotations": [[[0, 0, 0, 1]], [[0, 0, 0.5, 0.5]]], "roots": [[0, 0, 1], [0, 0, 1.5]], "notes": ["n"] });
    let m = ac_textmotion::helper::motion(&v).unwrap();
    assert_eq!(m.rotations.len(), 2);
    assert_eq!(m.roots[1], [0.0, 0.0, 1.5]);
    assert_eq!(m.notes, vec!["n".to_string()]);
    assert!(
        ac_textmotion::helper::motion(&json!({ "rotations": [[[0, 0, 1]]], "roots": [[0, 0, 1]] })).is_err()
    );
}
