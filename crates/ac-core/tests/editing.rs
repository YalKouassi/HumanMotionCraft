//! The editor commands end to end, through the session (as the UI, CLI and MCP drive them).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use ac_core::Session;
use serde_json::{Value, json};

use common::session;

fn pos(s: &mut Session, bone: &str) -> [f64; 3] {
    let v = s.run("bone.world", &json!({ "bone": bone })).unwrap();
    common::arr::<3>(&v["position"])
}

#[test]
fn key_drag_key_and_play_back() {
    let mut s = session();
    s.run("key.set", &json!({})).unwrap();
    s.run("frame.set", &json!({ "frame": 10 })).unwrap();
    let rest = pos(&mut s, "wrist.L");
    let to = [rest[0] - 0.2, rest[1] - 0.15, rest[2] + 0.2];
    s.run("edit.begin", &json!({ "label": "drag" })).unwrap();
    for i in 1..=5 {
        let t = i as f64 / 5.0;
        let at = [rest[0] - 0.2 * t, rest[1] - 0.15 * t, rest[2] + 0.2 * t];
        s.run("pose.drag", &json!({ "bone": "wrist.L", "to": at }))
            .unwrap();
    }
    assert_eq!(s.run("edit.end", &json!({})).unwrap()["changed"], true);
    let got = pos(&mut s, "wrist.L");
    common::assert_close("drag", "wrist", &got, &to, 1e-6);
    s.run("key.set", &json!({})).unwrap();
    // halfway between a rest key and the raised key the wrist is somewhere in between
    s.run("frame.set", &json!({ "frame": 5 })).unwrap();
    let mid = pos(&mut s, "wrist.L");
    assert!(mid[2] > rest[2] + 0.05 && mid[2] < to[2] - 0.05, "{mid:?}");
    // the whole drag is one undo step: undo the key, then the drag
    s.run("edit.undo", &json!({})).unwrap();
    s.run("edit.undo", &json!({})).unwrap();
    let state = s.run("editor.state", &json!({})).unwrap();
    assert_eq!(state["keys"].as_array().unwrap().len(), 1);
}

#[test]
fn partial_key_keys_only_its_part() {
    let mut s = session();
    s.run("key.set", &json!({})).unwrap();
    s.run("frame.set", &json!({ "frame": 20 })).unwrap();
    let foot = pos(&mut s, "foot.R");
    s.run("pose.drag", &json!({ "bone": "wrist.L", "by": [0.0, -0.2, 0.2] }))
        .unwrap();
    s.run("pose.drag", &json!({ "bone": "foot.R", "by": [0.0, -0.2, 0.1] }))
        .unwrap();
    s.run("key.set", &json!({ "parts": ["Left arm"] })).unwrap();
    let state = s.run("editor.state", &json!({})).unwrap();
    let k = &state["keys"].as_array().unwrap()[1];
    assert_eq!(k["part"], "Left arm");
    // the right leg was not keyed: going back to frame 20 shows it at rest
    s.run("frame.set", &json!({ "frame": 0 })).unwrap();
    s.run("frame.set", &json!({ "frame": 20 })).unwrap();
    common::assert_close("leg", "foot.R", &pos(&mut s, "foot.R"), &foot, 1e-9);
}

#[test]
fn copy_paste_mirror_and_refusals() {
    let mut s = session();
    s.run("key.set", &json!({})).unwrap();
    s.run("pose.drag", &json!({ "bone": "wrist.L", "by": [0.1, -0.2, 0.3] }))
        .unwrap();
    s.run("pose.copy", &json!({ "parts": ["Left arm"] })).unwrap();
    s.run("pose.reset", &json!({})).unwrap();
    s.run("pose.paste", &json!({ "mirror": true })).unwrap();
    let l = {
        let mut t = session();
        t.run("pose.drag", &json!({ "bone": "wrist.L", "by": [0.1, -0.2, 0.3] }))
            .unwrap();
        pos(&mut t, "wrist.L")
    };
    let r = pos(&mut s, "wrist.R");
    common::assert_close("mirror", "wrist.R", &r, &[-l[0], l[1], l[2]], 1e-9);
    assert!(
        s.run("pose.paste", &json!({ "parts": ["No such part"] }))
            .is_err()
    );
    assert!(s.run("key.delete", &json!({})).is_err(), "the last key stays");
    assert!(
        s.run(
            "pose.drag",
            &json!({ "bone": "wrist.L", "to": [0, 0, 0], "by": [0, 0, 0] })
        )
        .is_err()
    );
}

#[test]
fn every_command_is_listed_with_a_label() {
    let s = session();
    let ids: Vec<&str> = s.commands().iter().map(|c| c.id).collect();
    for id in ["edit.undo", "frame.set", "key.set", "pose.drag", "editor.state"] {
        assert!(ids.contains(&id), "{id}");
    }
    for c in s.commands() {
        assert!(!c.label.is_empty(), "{}", c.id);
        let parts: Vec<&str> = c.id.split('.').collect();
        assert!(
            parts.len() == 2
                && c.id
                    .chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch == '.' || ch == '_'),
            "{}",
            c.id
        );
    }
    let _: Value = json!(null);
}
