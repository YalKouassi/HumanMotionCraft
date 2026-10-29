//! Every feature fixture passes.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

#[test]
fn every_fixture_passes() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixtures = ac_cli::load_fixtures(&repo.join("fixtures")).unwrap();
    assert!(!fixtures.is_empty());
    let failed: Vec<String> =
        ac_cli::par_map(&fixtures, |(_, f)| ac_cli::run_fixture(&repo.join("assets"), f))
            .into_iter()
            .filter(|o| !o.failures.is_empty())
            .map(|o| format!("{}: {}", o.name, o.failures.join("; ")))
            .collect();
    assert!(failed.is_empty(), "{failed:#?}");
}

/// docs/testing.md: every registered command is used by a fixture that checks an effect at or after
/// that step, and is refused (with a message, nothing changed) in some fixture's `errors`.
#[test]
fn every_command_has_a_fixture() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixtures = ac_cli::load_fixtures(&repo.join("fixtures")).unwrap();
    let mut used = std::collections::BTreeSet::new();
    let mut refused = std::collections::BTreeSet::new();
    for (_, f) in &fixtures {
        let steps = f["script"].as_array().cloned().unwrap_or_default();
        let checked_at_end = f["expect"].get("bones").is_some() || f["expect"].get("state").is_some();
        for (i, step) in steps.iter().enumerate() {
            let later = steps[i..].iter().any(|s| s.get("expect").is_some());
            if later || checked_at_end {
                used.insert(step["command"].as_str().unwrap_or_default().to_string());
            }
        }
        for e in f["errors"].as_array().into_iter().flatten() {
            refused.insert(e["command"].as_str().unwrap_or_default().to_string());
        }
    }
    let s = ac_cli::session(
        &repo.join("assets"),
        &std::env::temp_dir().join(format!("{}-gate-library", ac_core::product::ID)),
    )
    .unwrap();
    let ids: Vec<&str> = s.commands().iter().map(|c| c.id).collect();
    let unused: Vec<&&str> = ids.iter().filter(|id| !used.contains(**id)).collect();
    let unrefused: Vec<&&str> = ids.iter().filter(|id| !refused.contains(**id)).collect();
    assert!(
        unused.is_empty() && unrefused.is_empty(),
        "no fixture checks: {unused:?}\nno fixture refuses: {unrefused:?}"
    );
}

/// The session adds its change summary (`changed`) to every undoable result, so an undoable
/// command that declared an output of that name would have it overwritten (`feet.plant` once
/// did).
#[test]
fn no_undoable_command_declares_a_session_key() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let s = ac_cli::session(
        &repo.join("assets"),
        &std::env::temp_dir().join(format!("{}-gate-library", ac_core::product::ID)),
    )
    .unwrap();
    let clashes: Vec<String> = s
        .commands()
        .iter()
        .filter(|c| c.undoable)
        .flat_map(|c| {
            c.outputs
                .iter()
                .filter(|p| ac_core::outputs::SESSION_KEYS.contains(&p.name))
                .map(|p| format!("{}.{}", c.id, p.name))
        })
        .collect();
    assert!(clashes.is_empty(), "the session overwrites these: {clashes:?}");
}
