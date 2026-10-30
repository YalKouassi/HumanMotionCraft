//! Nothing lands in the repo root by accident: a test or tool that writes a relative path (a
//! fixture's `{tmp}` left unsubstituted, a stale test binary run from the root) shows up here
//! instead of in a commit.
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

/// Everything the repo root may hold. Add a line here when a new top-level entry is meant.
const ALLOWED: &[&str] = &[
    ".cargo",
    ".claude",
    ".config",
    ".git",
    ".gitattributes",
    ".github",
    ".gitignore",
    ".mcp.json",
    "AGENTS.md",
    "ATTRIBUTION.md",
    "CLAUDE.md",
    "Cargo.lock",
    "Cargo.toml",
    "craft-app.json",
    "LICENSE",
    "NOTICE",
    "README.md",
    "ROADMAP.md",
    "assets",
    "book.toml",
    "crates",
    "docs",
    "fixtures",
    "roadmap",
    "rustfmt.toml",
    "services",
    "target",
    "tests",
    "tools",
    "training",
    "vendor",
    "xtask",
];

#[test]
fn the_repo_root_holds_only_what_it_should() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut stray: Vec<String> = std::fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| !ALLOWED.contains(&n.as_str()))
        .collect();
    stray.sort();
    assert!(
        stray.is_empty(),
        "stray entries in the repo root (written by a test or tool with a relative path?): {stray:?}. \
         Remove them, or add them to ALLOWED in {} if they are meant",
        file!()
    );
}
