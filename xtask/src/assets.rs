//! `cargo xtask assets`: every file under `assets/` has a row in `ATTRIBUTION.md`, and
//! `assets/models/MANIFEST.json` matches the models on disk.
//!
//! A row covers a file when its first cell names the file (`assets/rig/limits.json`) or a folder
//! above it (`assets/library/`), in backticks.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::Value;

/// The paths the first cell of each `ATTRIBUTION.md` table row names.
pub fn attributed(attribution: &str) -> Vec<String> {
    attribution
        .lines()
        .filter_map(|l| l.trim().strip_prefix('|'))
        .filter_map(|l| l.split('|').next())
        .flat_map(|cell| {
            cell.split('`')
                .skip(1)
                .step_by(2)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|p| p.starts_with("assets/") || p.contains('/'))
        .collect()
}

fn covered(file: &str, rows: &[String]) -> bool {
    rows.iter()
        .any(|r| r == file || (r.ends_with('/') && file.starts_with(r.as_str())))
}

/// `MANIFEST.json`'s problems, given the files under `assets/models/`.
pub fn manifest(text: &str, model_files: &[String], root: &Path) -> Vec<String> {
    let at = "assets/models/MANIFEST.json";
    let m: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => return vec![format!("{at}: not JSON: {e}")],
    };
    let mut bad = Vec::new();
    if !ac_core::product::is_format(m["format"].as_str().unwrap_or_default(), "models") {
        bad.push(format!("{at}: format {} is not <product id>.models", m["format"]));
    }
    let models = m["models"].as_array().cloned().unwrap_or_default();
    if models.is_empty() {
        bad.push(format!("{at}: no models listed"));
    }
    let mut paths = BTreeSet::new();
    let mut defaults = 0;
    for (i, model) in models.iter().enumerate() {
        let tag = model["tag"].as_str().unwrap_or_default();
        let path = model["path"].as_str().unwrap_or_default();
        let name = format!("{at}: models[{i}] ({tag:?})");
        if tag.is_empty() || path.is_empty() {
            bad.push(format!("{name}: needs a tag and a path"));
            continue;
        }
        if path.contains("..") || path.starts_with('/') || path.contains('\\') {
            bad.push(format!(
                "{name}: path {path:?} must be a folder name under assets/models"
            ));
            continue;
        }
        if model["source"].as_str().is_none_or(str::is_empty) {
            bad.push(format!("{name}: needs a source (how it was trained, on what)"));
        }
        let Some(commercial) = model["commercial"].as_bool() else {
            bad.push(format!("{name}: needs \"commercial\": true or false"));
            continue;
        };
        defaults += usize::from(model["default"].as_bool() == Some(true));
        paths.insert(format!("assets/models/{path}/"));
        let card_path = root.join("assets/models").join(path).join("weights.json");
        let card: Value = match std::fs::read_to_string(&card_path)
            .map_err(|e| e.to_string())
            .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
        {
            Ok(v) => v,
            Err(e) => {
                bad.push(format!("{name}: {}: {e}", card_path.display()));
                continue;
            }
        };
        if !root
            .join("assets/models")
            .join(path)
            .join("weights.bin")
            .is_file()
        {
            bad.push(format!("{name}: no weights.bin next to its card"));
        }
        if card["card"]["commercial"].as_bool() != Some(commercial) {
            bad.push(format!(
                "{name}: \"commercial\": {commercial} but the card in weights.json says {}",
                card["card"]["commercial"]
            ));
        }
    }
    if defaults != 1 && !models.is_empty() {
        bad.push(format!("{at}: {defaults} default models, expected exactly one"));
    }
    for f in model_files {
        if f != at && !paths.iter().any(|p| f.starts_with(p.as_str())) {
            bad.push(format!("{f}: not part of a model listed in {at}"));
        }
    }
    bad
}

/// Check the assets. Returns the problems (empty: clean).
pub fn check(root: &Path) -> Result<Vec<String>, String> {
    let files = crate::repo::files(root)?;
    let attribution =
        std::fs::read_to_string(root.join("ATTRIBUTION.md")).map_err(|e| format!("ATTRIBUTION.md: {e}"))?;
    let rows = attributed(&attribution);
    let mut bad: Vec<String> = files
        .iter()
        .filter(|f| f.starts_with("assets/") && !covered(f, &rows))
        .map(|f| format!("{f}: no row in ATTRIBUTION.md (source and licence)"))
        .collect();
    let models: Vec<String> = files
        .iter()
        .filter(|f| f.starts_with("assets/models/"))
        .cloned()
        .collect();
    let manifest_text = std::fs::read_to_string(root.join("assets/models/MANIFEST.json"))
        .map_err(|e| format!("assets/models/MANIFEST.json: {e}"))?;
    bad.extend(manifest(&manifest_text, &models, root));
    Ok(bad)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn rows_cover_files_and_folders() {
        let md = "| Path | What |\n|---|---|\n| `assets/rig/a.json` | x |\n| `assets/library/` (clips with `\"origin\"`) | y |\nnot a row `assets/z`\n";
        let rows = attributed(md);
        assert!(covered("assets/rig/a.json", &rows));
        assert!(covered("assets/library/poses/x.json", &rows));
        assert!(!covered("assets/rig/b.json", &rows));
        assert!(!covered("assets/z", &rows));
    }

    #[test]
    fn a_model_off_the_manifest_is_found() {
        let root = std::env::temp_dir();
        let m = r#"{"format": "x.models", "models": []}"#;
        let bad = manifest(m, &["assets/models/stray/w.bin".into()], &root);
        assert!(bad.iter().any(|b| b.contains("format")));
        assert!(bad.iter().any(|b| b.contains("no models")));
        assert!(bad.iter().any(|b| b.contains("stray")));
    }
}
