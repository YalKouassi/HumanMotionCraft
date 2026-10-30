//! Undo and redo for every command: every fixture runs again, and around each undoable step that
//! changed the document, undo must give back the document before and redo the one after. Every
//! command has a fixture (`every_command_has_a_fixture`), so this reaches them all. Steps inside a
//! gesture undo with the gesture, when it ends.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use serde_json::{Value, json};

/// Undoable commands the sweep can't check on the document: `clip.group` moves a file in the library
/// (`library-clips` checks its effect), and `layer.plan` plans the same travel again
/// unless its waypoints moved (`travel-two-metres`).
const LEFT_OUT: &[&str] = &["clip.group", "layer.plan"];

fn sweep(assets: &std::path::Path, fixture: &Value) -> (Vec<String>, Vec<String>) {
    let name = fixture["name"].as_str().unwrap_or("(unnamed)");
    ac_textmotion::stand_in::use_stand_in();
    ac_community::fake::use_fake(&assets.join("rig").join("default_rig.json"));
    let lib = std::env::temp_dir().join(format!(
        "{}-undo-{}-{}",
        ac_core::product::ID,
        std::process::id(),
        ac_io::library::safe(name).unwrap_or_default()
    ));
    std::fs::remove_dir_all(&lib).ok();
    let tmp = lib.join("scratch");
    let mut s = ac_cli::session(assets, &lib).unwrap();
    std::fs::create_dir_all(&tmp).unwrap();
    let mut out = Vec::new();
    let mut swept = Vec::new();
    for (i, step) in fixture["script"].as_array().into_iter().flatten().enumerate() {
        let cmd = step["command"].as_str().unwrap_or_default();
        let params = ac_cli::with_tmp(&step.get("params").cloned().unwrap_or(json!({})), &tmp, assets);
        let before = s.document.clone();
        let undoable = s.spec(cmd).is_some_and(|c| c.undoable);
        if s.run(cmd, &params).is_err() || !undoable || s.in_gesture() || s.document == before {
            continue;
        }
        swept.push(cmd.to_string());
        let after = s.document.clone();
        if let Err(e) = s.run("edit.undo", &json!({})) {
            out.push(format!("{name} step {i} {cmd}: undo was refused: {e}"));
            continue;
        }
        if s.document != before {
            out.push(format!(
                "{name} step {i} {cmd}: undo did not restore the document"
            ));
        }
        if let Err(e) = s.run("edit.redo", &json!({})) {
            out.push(format!("{name} step {i} {cmd}: redo was refused: {e}"));
            continue;
        }
        if s.document != after {
            out.push(format!("{name} step {i} {cmd}: redo did not bring the edit back"));
        }
    }
    std::fs::remove_dir_all(&lib).ok();
    (out, swept)
}

#[test]
fn every_undoable_step_undoes_and_redoes() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let assets = repo.join("assets");
    let fixtures = ac_cli::load_fixtures(&repo.join("fixtures")).unwrap();
    assert!(!fixtures.is_empty());
    // no fixture keeps or clears a fill that exists
    let fills = json!({ "name": "undo-fills", "script": [
        { "command": "key.set" },
        { "command": "frame.set", "params": { "frame": 20 } },
        { "command": "pose.set_root", "params": { "root": [0.1, 0, 0.9450101] } },
        { "command": "key.set" },
        { "command": "ai.fill", "params": { "from": 0, "to": 20 } },
        { "command": "ai.clear", "params": { "from": 0, "to": 20 } },
        { "command": "ai.fill", "params": { "from": 0, "to": 20 } },
        { "command": "ai.keep", "params": { "from": 0, "to": 20 } },
    ] });
    let mut all: Vec<&Value> = fixtures.iter().map(|(_, f)| f).collect();
    all.push(&fills);
    let results = ac_cli::par_map(&all, |f| sweep(&assets, f));
    let failed: Vec<&String> = results.iter().flat_map(|(f, _)| f).collect();
    assert!(failed.is_empty(), "{failed:#?}");
    let swept: std::collections::BTreeSet<&str> =
        results.iter().flat_map(|(_, s)| s).map(String::as_str).collect();
    let lib = std::env::temp_dir().join(format!(
        "{}-undo-gate-{}",
        ac_core::product::ID,
        std::process::id()
    ));
    let s = ac_cli::session(&assets, &lib).unwrap();
    let left: Vec<&str> = s
        .commands()
        .iter()
        .filter(|c| c.undoable && !swept.contains(c.id))
        .map(|c| c.id)
        .collect();
    std::fs::remove_dir_all(&lib).ok();
    println!("swept {} commands; left out: {left:?}", swept.len());
    assert!(
        left.iter().all(|id| LEFT_OUT.contains(id)),
        "no fixture sweeps {left:?}"
    );
}
