//! Moving the whole animation (`clip.move`): pins come along, other layers are reported, layers
//! that can't be moved are refused.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use ac_core::Session;
use ac_core::clip::{Layer, LayerMode};
use serde_json::json;

fn session() -> Session {
    let mut s = common::session();
    s.document.clip.physics = false; // `physics.set` lives in ac-io
    s.run("key.set", &json!({})).unwrap();
    s
}

fn pos(s: &mut Session, bone: &str) -> [f64; 3] {
    let v = s.run("bone.world", &json!({ "bone": bone })).unwrap();
    common::arr::<3>(&v["position"])
}

#[test]
fn pins_move_with_the_body_and_hold_there() {
    let mut s = session();
    s.run("pin.set", &json!({ "bone": "foot.L", "on": true }))
        .unwrap();
    s.run("clip.move", &json!({ "by": [0.5, -1.0, 0.0], "turn": 30 }))
        .unwrap();
    let foot = pos(&mut s, "foot.L");
    let pin = s.document.pins["foot.L"];
    common::assert_close("pin", "foot.L", &pin, &foot, 1e-9);
    // a reach afterwards keeps the foot at its new place, not the old one
    s.run(
        "pose.reach",
        &json!({ "bone": "wrist.R", "by": [0.0, -0.1, 0.1] }),
    )
    .unwrap();
    common::assert_close("reach", "foot.L", &pos(&mut s, "foot.L"), &foot, 2e-3);
}

#[test]
fn another_layer_that_places_the_body_is_reported() {
    let mut s = session();
    s.document.clip.layers.push(Layer::keys("dodge", Vec::new()));
    let v = s.run("clip.move", &json!({ "by": [1.0, 0.0, 0.0] })).unwrap();
    let notes = v["notes"].as_array().unwrap();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].as_str().unwrap().contains("dodge"), "{notes:?}");
}

#[test]
fn an_additive_layer_is_refused_and_nothing_moves() {
    let mut s = session();
    let mut add = Layer::keys("lean", Vec::new());
    add.mode = LayerMode::Additive;
    s.document.clip.layers.push(add);
    s.document.clip.edit = 1;
    let before = s.document.clip.clone();
    let err = s
        .run("clip.move", &json!({ "by": [1.0, 0.0, 0.0] }))
        .unwrap_err()
        .to_string();
    assert!(err.contains("lean"), "{err}");
    assert_eq!(s.document.clip, before);
}
