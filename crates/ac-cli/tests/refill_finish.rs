//! A key moved or copied beside a fill (roadmap 168000): each span it refills ends as a fresh
//! `ai.fill` with the same keys would. The two runs build their keys the same way and differ only
//! in when the fill happens. Short spans and a long one (anchors), with the real model.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use ac_core::Session;
use ac_core::clip::Key;
use serde_json::json;

/// How far a number may differ: a span next door is planted once more on the edited side, and IK
/// repeats its answer only to rounding.
const TOL: f64 = 1e-6;

type Span = (i64, i64);

fn session(name: &str) -> (Session, PathBuf) {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let library = std::env::temp_dir().join(format!("ac-refill-finish-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&library).unwrap();
    let built = ac_io::setup::SessionBuilder::new(&assets)
        .library(&library)
        .ai(ac_ai::shared(&assets))
        .strict()
        .build()
        .unwrap();
    (built.session, library)
}

/// A standing key on 0 in a new clip, with the walk's frame 8 on the clipboard.
fn start(s: &mut Session) {
    s.run("physics.set", &json!({ "on": false })).unwrap();
    s.run("ai.settings", &json!({ "ai_force": 1, "style": "any" }))
        .unwrap();
    s.run("clip.open", &json!({ "id": "Locomotion/Walk" })).unwrap();
    s.run("keys.copy", &json!({ "frames": [8] })).unwrap();
    s.run("pose.reset", &json!({})).unwrap();
    s.run("clip.new", &json!({ "name": "Refill", "group": "Mine" }))
        .unwrap();
}

/// The walk pose as a key on `f`, `y` hip heights along.
fn place(s: &mut Session, f: i64, y: f64) {
    s.run("keys.paste", &json!({ "at": f })).unwrap();
    s.run("pose.set_root", &json!({ "root": [0, y, 0.9027198] }))
        .unwrap();
    s.run("key.set", &json!({})).unwrap();
}

fn fill(s: &mut Session, spans: &[Span]) {
    for &(a, b) in spans {
        s.run("ai.fill", &json!({ "from": a, "to": b })).unwrap();
    }
}

fn keys(s: &Session) -> Vec<Key> {
    s.document.clip.keys().to_vec()
}

fn same(name: &str, x: &[Key], y: &[Key]) {
    within(name, x, y, TOL);
}

fn within(name: &str, x: &[Key], y: &[Key], tol: f32) {
    assert_eq!(x.len(), y.len(), "{name}: the number of keys");
    for (p, q) in x.iter().zip(y) {
        let f = p.frame;
        assert_eq!(
            (f, p.source, &p.model),
            (q.frame, q.source, &q.model),
            "{name}: frame {f}"
        );
        let root = p.root.iter().zip(&q.root).all(|(u, v)| (u - v).abs() < tol);
        let rots = p.rotations.len() == q.rotations.len()
            && p.rotations
                .iter()
                .zip(&q.rotations)
                .all(|(u, v)| u.iter().zip(v).all(|(c, d)| (c - d).abs() < tol));
        assert!(root && rots, "{name}: frame {f} differs");
    }
}

fn check(name: &str, setup: fn(&mut Session), first: &[Span], edit: (&str, i64, i64), then: &[Span]) {
    // copying into a span leaves the span before it alone, so its frames next to the shared key are
    // eased a second time; a fresh fill eases them once. Measured at most 0.0015 in a rotation.
    let tol = if name == "copy-short" { 2e-3 } else { TOL };
    let (cmd, from, to) = edit;
    let (mut s, dir) = session(name);
    start(&mut s);
    setup(&mut s);
    fill(&mut s, first);
    let before = keys(&s);
    s.run(cmd, &json!({ "from": from, "to": to })).unwrap();
    let edited = keys(&s);
    s.run("edit.undo", &json!({})).unwrap();
    same(&format!("{name}, one undo step"), &keys(&s), &before);

    let (mut s, fresh_dir) = session(&format!("{name}-fresh"));
    start(&mut s);
    setup(&mut s);
    s.run(cmd, &json!({ "from": from, "to": to })).unwrap();
    fill(&mut s, then);
    within(name, &edited, &keys(&s), tol);
    std::fs::remove_dir_all(dir).unwrap();
    std::fs::remove_dir_all(fresh_dir).unwrap();
}

fn short(s: &mut Session) {
    place(s, 20, -0.5);
    place(s, 40, -1.0);
}

fn long(s: &mut Session) {
    place(s, 90, -2.2993044);
}

#[test]
fn a_move_beside_short_fills_ends_as_a_fresh_fill() {
    check(
        "move-short",
        short,
        &[(0, 20), (20, 40)],
        ("key.move", 20, 25),
        &[(0, 25), (25, 40)],
    );
}

#[test]
fn a_copy_beside_short_fills_ends_as_a_fresh_fill() {
    check(
        "copy-short",
        short,
        &[(0, 20), (20, 40)],
        ("key.copy", 40, 30),
        &[(0, 20), (20, 30), (30, 40)],
    );
}

#[test]
fn a_move_of_a_long_fill_ends_as_a_fresh_fill() {
    check("move-long", long, &[(0, 90)], ("key.move", 90, 91), &[(0, 91)]);
}

#[test]
fn a_copy_into_a_long_fill_ends_as_a_fresh_fill() {
    check(
        "copy-long",
        long,
        &[(0, 90)],
        ("key.copy", 90, 40),
        &[(0, 40), (40, 90)],
    );
}
