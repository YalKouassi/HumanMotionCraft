//! `fixtures/schema.json` is generated from the fixture types and the registry
//! (`ac fixtures --schema`); it fails here when either changed and the schema was not rewritten.
//! Every fixture names it in `"$schema"`, so editors check and complete fixtures with it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

#[test]
fn the_fixture_schema_is_up_to_date() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let want = ac_cli::fixtures_cmd::schema_text(&repo.join("assets")).unwrap();
    let have = std::fs::read_to_string(repo.join(ac_cli::fixture_spec::SCHEMA))
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        have == want,
        "{} is stale: run `cargo run -p ac-cli -- fixtures --schema` and commit it",
        ac_cli::fixture_spec::SCHEMA
    );
}

#[test]
fn every_fixture_names_the_schema() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let missing: Vec<String> = ac_cli::load_fixtures(&repo.join("fixtures"))
        .unwrap()
        .into_iter()
        .filter(|(_, fx)| fx["$schema"] != "./schema.json")
        .map(|(p, _)| p.display().to_string())
        .collect();
    assert!(
        missing.is_empty(),
        "add \"$schema\": \"./schema.json\" to {missing:?}"
    );
}
