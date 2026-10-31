//! Every command survives hostile input; refusals change nothing; undo/redo round-trip.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use ac_core::{CoreError, Kind};
use serde_json::{Map, Value, json};

use common::session;

fn hostile(kind: Kind, range: Option<(f64, f64)>) -> Vec<Value> {
    let mut v = vec![
        json!(null),
        json!(0),
        json!(-1),
        json!(1e300),
        json!(-1e300),
        json!(""),
        json!("x".repeat(5000)),
        json!(true),
        json!([]),
        json!({}),
        json!([f64::MAX, 0, 0]),
        json!([1e300, 1e300, 1e300, 1e300]),
        json!([0, 0, 0, 0]),
        json!([1, 2]),
        json!(["a", "b", "c"]),
        json!("no.such.bone"),
        json!([0, 0, 0, 1, 5]),
    ];
    if let Some((lo, hi)) = range {
        v.extend([json!(lo), json!(hi), json!(lo - 1e-9), json!(hi + 1e-9)]);
    }
    if let Kind::Choice(words) = kind {
        v.extend(words.iter().map(|w| json!(w)));
        v.push(json!("not-a-choice"));
    }
    v
}

fn good(kind: Kind) -> Value {
    match kind {
        Kind::Number | Kind::Integer => json!(0),
        Kind::Bool => json!(false),
        Kind::Text => json!("x"),
        Kind::Bone => json!("wrist.L"),
        Kind::Choice(words) => json!(words.first().copied().unwrap_or("")),
        Kind::Vec3 => json!([0.0, 0.0, 0.0]),
        Kind::Quat => json!([0.0, 0.0, 0.0, 1.0]),
        Kind::TextList => json!([]),
        Kind::IntList => json!([]),
        Kind::Points => json!([]),
        Kind::Offsets => json!({}),
    }
}

#[test]
fn every_command_survives_its_extremes() {
    let mut s = session();
    let specs: Vec<_> = s.commands().iter().map(|c| (c.id, c.inputs)).collect();
    assert!(!specs.is_empty());
    for (id, inputs) in specs {
        let base: Map<String, Value> = inputs
            .iter()
            .filter(|p| p.required)
            .map(|p| (p.name.to_string(), good(p.kind)))
            .collect();
        let mut with_unknown = base.clone();
        with_unknown.insert("unknown_key".into(), json!(1));
        let mut tries = vec![json!("not an object"), json!([1, 2]), Value::Object(with_unknown)];
        for p in inputs {
            if p.required {
                let mut m = base.clone();
                m.remove(p.name);
                tries.push(Value::Object(m));
            }
            for h in hostile(p.kind, p.range) {
                let mut m = base.clone();
                m.insert(p.name.to_string(), h);
                tries.push(Value::Object(m));
            }
        }
        for params in tries {
            let before = s.document.clone();
            match s.run(id, &params) {
                Ok(_) => {
                    let d = &s.document;
                    let (r, p) = d.skeleton.fk(&d.pose.rotations, d.pose.root).unwrap();
                    let finite = r
                        .iter()
                        .flatten()
                        .flatten()
                        .chain(p.iter().flatten())
                        .all(|x| x.is_finite());
                    assert!(finite, "{id} {params}: accepted but left a non-finite pose");
                }
                Err(e) => {
                    assert!(
                        !e.to_string().trim().is_empty(),
                        "{id} {params}: refused without a message"
                    );
                    assert!(
                        s.document == before,
                        "{id} {params}: refused but changed the document"
                    );
                    assert!(
                        !matches!(e, CoreError::Panicked { .. }),
                        "{id} {params}: panicked: {e}"
                    );
                }
            }
        }
    }
}

#[test]
fn undo_and_redo_round_trip() {
    let mut s = session();
    let q = json!([0.0, 0.0, 0.38268343236508984, 0.9238795325112867]);
    s.run("pose.set_bone", &json!({ "bone": "wrist.L", "rotation": q }))
        .unwrap();
    s.run("pose.set_root", &json!({ "root": [0.1, 0.0, 0.0] }))
        .unwrap();
    let edited = s.document.clone();
    assert_eq!(s.run("edit.undo", &json!({})).unwrap()["changed"], true);
    assert_eq!(s.run("edit.undo", &json!({})).unwrap()["changed"], true);
    assert_eq!(s.document.pose, ac_core::document::standing(&s.document.skeleton));
    assert_eq!(s.run("edit.undo", &json!({})).unwrap()["changed"], false);
    s.run("edit.redo", &json!({})).unwrap();
    s.run("edit.redo", &json!({})).unwrap();
    assert!(s.document == edited);
}

#[test]
fn queries_do_not_enter_the_undo_history() {
    let mut s = session();
    s.run("rig.info", &json!({})).unwrap();
    s.run("bone.world", &json!({ "bone": "head" })).unwrap();
    assert!(!s.can_undo());
}

#[test]
fn bone_world_reports_rest_heads() {
    // At rest every rotation is the identity, so with root offset 0 each head sits at its baked
    // rest position (a new document stands on the floor: its root offset lifts it).
    let mut s = session();
    s.run("pose.set_root", &json!({ "root": [0.0, 0.0, 0.0] }))
        .unwrap();
    let sk = s.document.skeleton.clone();
    let i = sk.bone("wrist.L").unwrap();
    let got = s.run("bone.world", &json!({ "bone": "wrist.L" })).unwrap();
    let pos: Vec<f64> = got["position"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect();
    common::assert_close("rest", "wrist.L", &pos, &sk.heads[i], 1e-12);
}

#[test]
fn a_preset_alias_is_refused_listing_the_three() {
    let mut s = session();
    for name in ["preserve", "natural", "follow_style"] {
        s.run("ai.settings", &json!({ "preset": name })).unwrap();
    }
    for alias in ["preservemotion", "natural movement", "style", "Follow-Style"] {
        let e = s
            .run("ai.settings", &json!({ "preset": alias }))
            .unwrap_err()
            .to_string();
        for name in ["preserve", "natural", "follow_style"] {
            assert!(e.contains(name), "{alias}: {e}");
        }
    }
}
