//! Exporting several library animations at once, a `.glb` each, into one folder (the Export
//! dialog's "one file per animation"). Each file is what `clip.export` writes for that animation.

use std::collections::BTreeSet;
use std::path::PathBuf;

use ac_core::Document;
use ac_core::command::{self as cmd, CommandSpec, Kind as K, ParamSpec};
use serde_json::{Map, Value, json};

use crate::library::safe;

type P = Map<String, Value>;
type Out = Result<Value, String>;

/// Most animations one batch exports.
pub const MAX_BATCH: usize = 1000;

pub static SPECS: &[CommandSpec] =
    &[
        CommandSpec::new("clip.export_batch", "Export each to .glb", export_batch)
            .icon("export")
            .inputs(&[
                ParamSpec::new(
                    "ids",
                    K::TextList,
                    "the library animations to export, a file each",
                )
                .required(),
                ParamSpec::new("folder", K::Text, "the folder the files go in (made if missing)").required(),
                ParamSpec::new(
                    "profile",
                    K::Choice(&["full", "game", "body"]),
                    "which joints to keep",
                ),
                ParamSpec::new("bones", K::TextList, "exactly these joints instead of a profile"),
                ParamSpec::new("drop", K::TextList, "joints to leave out as well"),
                ParamSpec::new(
                    "root",
                    K::Choice(&["hips", "ground", "in_place"]),
                    "root motion bone: hips (pelvis only), ground (bone on the floor), in_place (ground bone with movement removed)",
                ),
                crate::export::TARGET,
            ])
            .writes()
            .outputs(crate::returns::CLIP_EXPORT_BATCH),
    ];

/// The file name for each id: `<name>.glb`, or `<group> - <name>.glb` when two share a name.
fn file_names(ids: &[String]) -> Result<Vec<String>, String> {
    let names: Vec<(String, String)> = ids
        .iter()
        .map(|id| {
            let (g, n) = id
                .split_once('/')
                .ok_or_else(|| format!("{id:?} is not <group>/<name>"))?;
            Ok((safe(g)?, safe(n)?))
        })
        .collect::<Result<_, String>>()?;
    let mut seen = BTreeSet::new();
    let shared: BTreeSet<String> = names
        .iter()
        .filter(|(_, n)| !seen.insert(n.to_lowercase()))
        .map(|(_, n)| n.to_lowercase())
        .collect();
    Ok(names
        .into_iter()
        .map(|(g, n)| {
            if shared.contains(&n.to_lowercase()) {
                format!("{g} - {n}.glb")
            } else {
                format!("{n}.glb")
            }
        })
        .collect())
}

/// Export each animation to its own file. One that can't be exported is listed in `failed` with
/// why, and the rest are still written; the command is refused only when none could be.
fn export_batch(doc: &mut Document, p: &P) -> Out {
    let ids = cmd::texts(p, "ids");
    if ids.is_empty() {
        return Err("pick at least one animation to export".into());
    }
    if ids.len() > MAX_BATCH {
        return Err(format!(
            "{} animations is more than {MAX_BATCH} at once",
            ids.len()
        ));
    }
    let unique: BTreeSet<&String> = ids.iter().collect();
    if unique.len() != ids.len() {
        return Err("an animation is listed twice".into());
    }
    let folder = PathBuf::from(cmd::text(p, "folder").unwrap_or_default());
    let names = file_names(&ids)?;
    let lib = crate::commands::anims(doc)?;
    if let Some(missing) = ids.iter().find(|id| !lib.exists(id)) {
        return Err(format!("no animation {missing:?} in the library"));
    }
    let mut files = Vec::new();
    let mut failed = Vec::new();
    for (id, name) in ids.iter().zip(&names) {
        let mut q = P::new();
        q.insert("path".into(), json!(folder.join(name).to_string_lossy()));
        q.insert("ids".into(), json!([id]));
        for k in ["profile", "bones", "drop", "root", "target"] {
            if let Some(v) = p.get(k) {
                q.insert(k.into(), v.clone());
            }
        }
        match crate::export::clip_export(doc, &q) {
            Ok(r) => files.push(json!({ "id": id, "file": name, "path": r["path"], "joints": r["joints"], "frames": r["frames"] })),
            Err(e) => failed.push(json!({ "id": id, "error": e })),
        }
    }
    if files.is_empty() {
        let first = failed
            .first()
            .map(|f| f["error"].as_str().unwrap_or_default().to_string());
        return Err(format!(
            "nothing exported: {}",
            first.unwrap_or_else(|| "no animations".into())
        ));
    }
    Ok(json!({ "folder": folder.to_string_lossy(), "files": files, "failed": failed }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_that_clash_take_their_group() {
        let ids = ["A/Walk".to_string(), "B/walk".to_string(), "B/Run".to_string()];
        assert_eq!(
            super::file_names(&ids),
            Ok(vec![
                "A - Walk.glb".to_string(),
                "B - walk.glb".to_string(),
                "Run.glb".to_string()
            ])
        );
    }
}
