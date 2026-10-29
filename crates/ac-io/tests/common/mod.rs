//! What the ac-io tests share: the repo and one session setup (`ac_io::setup`, as the app and the
//! CLI build theirs).
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use ac_core::Session;

pub fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A session on a fresh scratch library (`<tmp>/<id>-test-<name>-<pid>`) with the built-ins
/// installed. No joint limits and no AI model: these tests are about files.
pub fn session(name: &str) -> (Session, PathBuf) {
    let lib = std::env::temp_dir().join(format!(
        "{}-test-{name}-{}",
        ac_core::product::ID,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&lib);
    let built = ac_io::setup::SessionBuilder::new(&repo().join("assets"))
        .library(&lib)
        .limits(false)
        .strict()
        .build()
        .unwrap();
    assert!(
        built.installed.unwrap() > 50,
        "the built-in library was installed"
    );
    (built.session, lib)
}
