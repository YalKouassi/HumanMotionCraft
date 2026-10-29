//! `docs/commands.md` is the registry, written out: it fails here when a command, input or output
//! changed and the reference was not regenerated (`ac commands --md`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

#[test]
fn the_command_reference_is_up_to_date() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let s = ac_cli::session(
        &repo.join("assets"),
        &std::env::temp_dir().join(format!(
            "{}-commands-doc-{}",
            ac_core::product::ID,
            std::process::id()
        )),
    )
    .unwrap();
    let want = ac_cli::cmdref::markdown(s.commands()).unwrap();
    let have = std::fs::read_to_string(repo.join(ac_cli::cmdref::DOC))
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        have == want,
        "{} is stale: run `cargo run -p ac-cli -- commands --md` and commit it",
        ac_cli::cmdref::DOC
    );
    for c in s.commands() {
        assert!(want.contains(&format!("### `{}`", c.id)), "{} missing", c.id);
    }
}
