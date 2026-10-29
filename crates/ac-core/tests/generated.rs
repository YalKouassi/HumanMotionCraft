//! Motion made from text (AI keys tagged `edits::GENERATED`) is never cleared or refilled by an
//! edit next to it: the edit leaves it and says so. Without such keys nothing changes.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use ac_core::Session;
use ac_core::clip::{Interp, Key, KeySource};
use ac_core::edits;
use serde_json::{Value, json};

/// Keys on 0 and 30 and AI keys made from text on 1..=29 (the rest pose, lifted 1 cm a frame so
/// they can be told apart).
fn with_generated() -> Session {
    let mut s = common::session();
    s.run("key.set", &json!({})).unwrap();
    s.run("frame.set", &json!({ "frame": 30 })).unwrap();
    s.run("key.set", &json!({})).unwrap();
    let doc = &mut s.document;
    let bones = doc.skeleton.names.len();
    let root = doc.clip.keys()[0].root;
    for f in 1..30 {
        let mut k = Key::new(
            f,
            vec![[0.0, 0.0, 0.0, 1.0]; bones],
            [root[0], root[1], root[2] + 0.01 * f as f64],
            Interp::Smooth,
        );
        k.source = KeySource::Ai;
        k.model = "text A person waves.".into();
        doc.clip.set_key(k).unwrap();
    }
    s
}

fn generated(s: &Session) -> Vec<(i64, f64)> {
    s.document
        .clip
        .keys()
        .iter()
        .filter(|k| k.source == KeySource::Ai && edits::is_generated(&k.model))
        .map(|k| (k.frame, k.root[2]))
        .collect()
}

fn notes(v: &Value) -> Vec<String> {
    v["notes"]
        .as_array()
        .map(|a| a.iter().filter_map(|n| n.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

#[test]
fn the_tag_is_text_and_a_prompt() {
    assert!(edits::is_generated("text"));
    assert!(edits::is_generated("text A person waves."));
    assert!(!edits::is_generated("textured"));
    assert!(!edits::is_generated("parts walk"));
    assert!(!edits::is_generated(""));
}

#[test]
fn setting_a_key_beside_it_leaves_it_and_says_so() {
    let mut s = with_generated();
    let before = generated(&s);
    assert_eq!(before.len(), 29);
    // frame 30 is keyed again (changed): the span 0-30 was made for the old key
    s.run("pose.drag", &json!({ "bone": "wrist.L", "by": [0.0, -0.1, 0.1] }))
        .unwrap();
    let out = s.run("key.set", &json!({})).unwrap();
    assert_eq!(generated(&s), before);
    let n = notes(&out);
    assert!(
        n.iter()
            .any(|t| t.starts_with("0-30:") && t.contains("text.fill")),
        "{n:?}"
    );
}

#[test]
fn moving_or_copying_a_key_beside_it_leaves_it_and_says_so() {
    let mut s = with_generated();
    let before = generated(&s);
    let out = s.run("key.move", &json!({ "from": 30, "to": 34 })).unwrap();
    assert_eq!(generated(&s), before);
    assert!(notes(&out).iter().any(|t| t.starts_with("0-34:")), "{out}");

    // a key copied into the middle splits the span; the frames made from text on both sides stay
    let mut s = with_generated();
    let out = s.run("key.copy", &json!({ "from": 0, "to": 15 })).unwrap();
    let after = generated(&s);
    assert_eq!(after.len(), 28, "every frame but the new key's stays");
    assert!(after.iter().all(|&(f, _)| f != 15));
    let n = notes(&out);
    assert!(
        n.iter().any(|t| t.starts_with("0-15:")) && n.iter().any(|t| t.starts_with("15-30:")),
        "{n:?}"
    );
}

#[test]
fn an_explicit_clear_still_clears_it() {
    let mut s = with_generated();
    let out = s.run("ai.clear", &json!({ "from": 0, "to": 30 })).unwrap();
    assert_eq!(out["cleared"], 29);
    assert!(generated(&s).is_empty());
}

#[test]
fn without_generated_keys_a_key_set_says_nothing_new() {
    let mut s = common::session();
    s.run("key.set", &json!({})).unwrap();
    s.run("frame.set", &json!({ "frame": 30 })).unwrap();
    let out = s.run("key.set", &json!({})).unwrap();
    assert!(out.get("notes").is_none(), "{out}");
}

#[test]
fn a_key_set_again_as_it_was_says_nothing() {
    let mut s = with_generated();
    s.run("frame.set", &json!({ "frame": 30 })).unwrap();
    let out = s.run("key.set", &json!({})).unwrap();
    assert!(out.get("notes").is_none(), "{out}");
    assert_eq!(generated(&s).len(), 29);
}
