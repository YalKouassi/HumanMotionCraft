//! Visual goldens: every fixture view, rendered on the software adapter (WARP) at a small size,
//! matches `tests/visual/<fixture>-<view>.png` within a pixel tolerance. A failure leaves the
//! actual picture and a diff under `target/visual-diff/`. After checking a changed picture by
//! eye, bless with `AC_BLESS_VISUAL=1 cargo test -p ac-cli --test visual` (or
//! `ac fixtures --golden-check --bless-visual`).
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

#[test]
fn every_fixture_view_matches_its_golden() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let (assets, goldens, diffs) = (
        repo.join("assets"),
        repo.join("tests/visual"),
        repo.join("target/visual-diff"),
    );
    let bless = std::env::var_os("AC_BLESS_VISUAL").is_some_and(|v| v == "1");
    let fixtures = ac_cli::load_fixtures(&repo.join("fixtures")).unwrap();
    let views: usize = fixtures
        .iter()
        .map(|(_, fx)| fx["views"].as_array().map_or(0, Vec::len))
        .sum();
    let failures: Vec<String> = ac_cli::par_map(&fixtures, |(_, fx)| {
        ac_cli::golden_check(&assets, fx, &goldens, &diffs, bless)
            .unwrap_or_else(|e| vec![format!("{}: {e}", fx["name"])])
    })
    .into_iter()
    .flatten()
    .collect();
    assert!(views > 0, "no fixture has views");
    assert!(
        failures.is_empty(),
        "{} of {views} views differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
