//! `cargo xtask docs`: what `AGENTS.md` points at exists, so agents aren't sent to crates and
//! files that were planned and never made.
//!
//! Checked: every backticked repo path (starting with a top-level folder of the repo or naming a
//! root file such as `ROADMAP.md`) exists, unless git ignores it (`vendor/craft-libs`,
//! `training/data/`: local-only by design) or it is a pattern (`<name>`, `{tmp}`, `*`); and every
//! crate named in the "Layering:" paragraph is a folder under `crates/`.

use std::path::Path;

/// The files whose cited paths are checked.
pub const DOCS: &[&str] = &["AGENTS.md"];

/// Backticked spans of `text`.
fn spans(text: &str) -> impl Iterator<Item = &str> {
    text.split('`').skip(1).step_by(2)
}

/// Whether `span` reads as a repo path: under a top-level folder that exists, or a root `*.md`.
fn repo_path<'a>(span: &'a str, top: &[String]) -> Option<&'a str> {
    let p = span.split_whitespace().next()?;
    let p = p.trim_end_matches([',', '.', ':', ')']);
    if p.contains(['<', '>', '{', '}', '*', '%', '$']) || p.starts_with("http") {
        return None;
    }
    let first = p.split('/').next()?;
    let root_md = !p.contains('/') && p.ends_with(".md") && p.chars().next()?.is_ascii_uppercase();
    (root_md || (p.contains('/') && top.iter().any(|t| t == first))).then_some(p)
}

/// The problems in one document.
pub fn check_doc(root: &Path, doc: &str, text: &str) -> Vec<String> {
    let top: Vec<String> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != "target" && n != ".git")
        .chain(["vendor".to_string()])
        .collect();
    let mut bad = Vec::new();
    for span in spans(text) {
        let Some(p) = repo_path(span, &top) else { continue };
        if !root.join(p).exists() && !crate::repo::ignored(root, p) {
            bad.push(format!("{doc}: `{p}` does not exist"));
        }
    }
    if let Some(layering) = text
        .split("\n\n")
        .find(|para| para.trim_start().starts_with("Layering:"))
    {
        for name in spans(layering).filter(|s| s.starts_with("ac-")) {
            if !root.join("crates").join(name).is_dir() {
                bad.push(format!(
                    "{doc}: the layering names `{name}`, which is not a crate"
                ));
            }
        }
    }
    bad.sort();
    bad.dedup();
    bad
}

/// Check every document in [`DOCS`].
pub fn check(root: &Path) -> Result<Vec<String>, String> {
    let mut bad = Vec::new();
    for doc in DOCS {
        let text = std::fs::read_to_string(root.join(doc)).map_err(|e| format!("{doc}: {e}"))?;
        bad.extend(check_doc(root, doc, &text));
    }
    Ok(bad)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// The real AGENTS.md cites only what exists.
    #[test]
    fn every_path_agents_md_cites_exists() {
        let bad = check(&crate::repo::root()).unwrap();
        assert!(bad.is_empty(), "{bad:#?}");
    }

    #[test]
    fn made_up_paths_and_crates_are_found() {
        let root = crate::repo::root();
        let text = "See `crates/ac-nothing` and `NOPE.md`, not `<repo>/x` or `{tmp}/y`.\n\nLayering: `ac-core` → `ac-ghost`.\n";
        let bad = check_doc(&root, "t.md", text);
        assert_eq!(bad.len(), 3, "{bad:#?}");
    }
}
