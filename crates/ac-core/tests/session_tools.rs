//! The session's agent tools: structured errors with suggestions and fixes, undo labels from the
//! spec, change summaries, batches, dry runs and the history's limits.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use ac_core::session::MAX_UNDO;
use ac_core::{CoreError, Document, Session};
use serde_json::json;

fn session() -> Session {
    Session::new(Document::new(common::skeleton()))
}

#[test]
fn unknown_commands_suggest_the_closest_ids() {
    let mut s = session();
    let e = s.run("key.sett", &json!({})).unwrap_err();
    match &e {
        CoreError::UnknownCommand { suggestions, .. } => assert_eq!(suggestions, &["key.set"]),
        other => panic!("{other}"),
    }
    assert_eq!(e.to_json()["kind"], "unknown_command");
    assert!(e.to_string().contains("did you mean \"key.set\""), "{e}");
}

#[test]
fn unknown_bones_and_parameters_suggest_names() {
    let mut s = session();
    let e = s
        .run(
            "pose.set_bone",
            &json!({ "bone": "wrist.l", "rotation": [0, 0, 0, 1] }),
        )
        .unwrap_err();
    match &e {
        CoreError::Refused { message, hint, .. } => {
            assert_eq!(message, "no bone named \"wrist.l\" in the rig");
            assert!(hint.as_deref().unwrap().contains("\"wrist.L\""), "{hint:?}");
        }
        other => panic!("{other}"),
    }
    let e = s.run("bone.world", &json!({ "bones": "head" })).unwrap_err();
    assert!(e.to_string().contains("did you mean \"bone\"?"), "{e}");
}

#[test]
fn refusals_caused_by_state_carry_a_fix() {
    let mut s = session();
    let e = s.run("clip.sample", &json!({})).unwrap_err();
    assert_eq!(
        e.to_json()["hint"],
        "pose the body and run key.set, or open one with clip.open"
    );
    let e = s.run("pose.paste", &json!({})).unwrap_err();
    assert_eq!(e.to_json()["hint"], "run pose.copy first");
}

#[test]
fn undo_labels_come_from_the_spec() {
    let mut s = session();
    s.run("key.set", &json!({})).unwrap();
    s.run("key.move", &json!({ "from": 0, "to": 5 })).unwrap();
    assert_eq!(s.undo_label(), Some("Move key 0→5"));
    s.run("edit.begin", &json!({ "label": "Wiggle" })).unwrap();
    s.run("pose.set_root", &json!({ "root": [0, 0, 1] })).unwrap();
    s.run("edit.end", &json!({})).unwrap();
    assert_eq!(s.undo_label(), Some("Wiggle"));
    assert_eq!(s.history_json()["steps"][2]["command"], "edit.begin");
}

#[test]
fn undoable_results_carry_a_change_summary() {
    let mut s = session();
    let r = s.run("key.set", &json!({})).unwrap();
    assert_eq!(r["changed"]["keys"], json!([{ "layer": "Base", "added": [0] }]));
    // nothing changed: an empty summary, and no undo step
    let r = s.run("pins.clear", &json!({})).unwrap();
    assert_eq!(r["changed"], json!({}));
    assert_eq!(s.history().len(), 1);
}

#[test]
fn a_failed_atomic_batch_changes_nothing_and_says_which_step() {
    let mut s = session();
    s.run("key.set", &json!({})).unwrap();
    let before = s.document.clone();
    let e = s
        .run(
            "edit.batch",
            &json!({ "commands": [
                { "id": "frame.set", "params": { "frame": 7 } },
                { "id": "key.set" },
                { "id": "key.move", "params": { "from": 40, "to": 41 } },
            ] }),
        )
        .unwrap_err();
    match &e {
        CoreError::Step { step, id, error, .. } => {
            assert_eq!((*step, id.as_str()), (2, "key.move"));
            assert!(matches!(**error, CoreError::Refused { .. }));
        }
        other => panic!("{other}"),
    }
    assert_eq!(e.to_json()["step"], 2);
    assert!(s.document == before);
    assert_eq!(s.history(), vec!["Set key"]);
    assert!(!s.in_gesture());
}

#[test]
fn a_batch_is_one_undo_step_with_a_default_label() {
    let mut s = session();
    s.run("edit.batch", &json!({ "commands": [{ "id": "key.set" }] }))
        .unwrap();
    assert_eq!(s.undo_label(), Some("Set key"));
    s.run(
        "edit.batch",
        &json!({ "commands": [{ "id": "frame.set", "params": { "frame": 3 } }, { "id": "key.set" }] }),
    )
    .unwrap();
    assert_eq!(s.undo_label(), Some("2 commands"));
    s.run("edit.undo", &json!({})).unwrap();
    assert_eq!(s.document.clip.keys().len(), 1);
    assert_eq!(s.document.frame, 0.0);
}

#[test]
fn a_dry_run_touches_neither_the_document_nor_the_history() {
    let mut s = session();
    s.run("key.set", &json!({})).unwrap();
    let before = s.document.clone();
    let r = s
        .run(
            "edit.dry_run",
            &json!({ "id": "pose.set_root", "params": { "root": [0, 0, 2] } }),
        )
        .unwrap();
    assert_eq!(r["changed"]["pose"]["root"], true);
    assert_eq!(r["undo_label"], "Set root offset");
    assert!(s.document == before);
    assert_eq!(s.history().len(), 1);
    // a query makes no step
    let r = s.run("edit.dry_run", &json!({ "id": "rig.info" })).unwrap();
    assert_eq!(
        (r["changed"].clone(), r["undo_label"].clone()),
        (json!({}), json!(null))
    );
}

#[test]
fn history_indices_stay_put_past_the_undo_limit() {
    let mut s = session();
    s.run("edit.checkpoint", &json!({ "name": "zero" })).unwrap();
    for i in 0..MAX_UNDO + 2 {
        s.run("pose.set_root", &json!({ "root": [0, 0, i as f64 * 1e-3] }))
            .unwrap();
    }
    let h = s.history_json();
    assert_eq!(
        (h["first"].clone(), h["current"].clone()),
        (json!(2), json!(MAX_UNDO + 2))
    );
    // the checkpoint fell off with the steps under it
    assert_eq!(h["checkpoints"], json!([]));
    assert!(s.run("edit.revert_to", &json!({ "index": 1 })).is_err());
    let r = s.run("edit.revert_to", &json!({ "index": 2 })).unwrap();
    assert_eq!(r["undone"], MAX_UNDO);
    assert!(!s.can_undo());
}

#[test]
fn session_info_names_what_the_session_runs_with() {
    let s = session();
    let i = s.info();
    assert_eq!(i["rig_hash"].as_str().unwrap().len(), 16);
    assert!(i["limits_hash"].is_null() && i["model"].is_null() && i["seed"].is_null());
    // the same rig hashes the same
    assert_eq!(session().info()["rig_hash"], i["rig_hash"]);
}
