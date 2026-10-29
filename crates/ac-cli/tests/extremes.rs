//! Every registered command (core, library, layers, export) survives hostile input: a refusal says
//! why, changes nothing and never panics; an acceptance leaves a finite pose.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use ac_core::{CoreError, Kind};
use serde_json::{Map, Value, json};

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
        json!([[1e300, 0], [0, 0]]),
        json!({ "L0": [1e300, 0] }),
        json!("../../outside"),
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

fn good(name: &str, kind: Kind) -> Value {
    match (name, kind) {
        ("path", _) => json!("scratch.glb"),
        ("id", _) => json!("Locomotion/Walk"),
        (_, Kind::Number | Kind::Integer) => json!(0),
        (_, Kind::Bool) => json!(false),
        (_, Kind::Text) => json!("x"),
        (_, Kind::Bone) => json!("wrist.L"),
        (_, Kind::Choice(words)) => json!(words.first().copied().unwrap_or("")),
        (_, Kind::Vec3) => json!([0.0, 0.0, 0.0]),
        (_, Kind::Quat) => json!([0.0, 0.0, 0.0, 1.0]),
        (_, Kind::TextList | Kind::IntList | Kind::Points) => json!([]),
        (_, Kind::Offsets) => json!({}),
    }
}

#[test]
fn every_command_survives_its_extremes() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    // file commands write relative paths here, never into the repo (one test in this binary)
    let work = std::env::temp_dir().join(format!(
        "{}-extremes-{}",
        ac_core::product::ID,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    std::env::set_current_dir(&work).unwrap();
    let mut s = ac_cli::session(&repo.join("assets"), &work.join("library")).unwrap();
    // something to work on: two keys and a layer
    s.run("clip.new", &json!({ "name": "Extremes" })).unwrap();
    s.run("frame.set", &json!({ "frame": 30 })).unwrap();
    s.run("key.set", &json!({})).unwrap();
    s.run("layer.add", &json!({ "name": "Top" })).unwrap();
    let mut specs: Vec<_> = s.commands().iter().map(|c| (c.id, c.inputs)).collect();
    // the ones that take the clip or library away run last
    specs.sort_by_key(|(id, _)| {
        matches!(
            *id,
            "clip.delete" | "library.install" | "session.reset" | "edit.revert_to"
        )
    });
    for (id, inputs) in specs {
        let base: Map<String, Value> = inputs
            .iter()
            .filter(|p| p.required)
            .map(|p| (p.name.to_string(), good(p.name, p.kind)))
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
    std::env::set_current_dir(std::env::temp_dir()).unwrap();
    let _ = std::fs::remove_dir_all(&work);
}
