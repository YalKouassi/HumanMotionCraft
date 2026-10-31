//! Exporting onto a character of the person's own (roadmap 146000): the character's skeleton is
//! read from its `.glb` ([`read`]), the landmarks are mapped onto its nodes
//! (`ac_core::character`), the mapping is kept in a file of our own format ([`save_text`],
//! [`load_text`]), and the `retarget.*` commands ([`commands`]) load, correct, check and save
//! it; `clip.export` with `target` writes the character's file with the animation on its own
//! nodes ([`export`]).
//!
//! The mapping file:
//!
//! ```text
//! {
//!   "character": "<the .glb as given, relative paths from the mapping file's folder>",
//!   "format": "<id>.retarget", "version": 1,
//!   "hands": false,
//!   "joints": { "hips": "Pelvis", "l_hip": "L_Thigh", ... }
//! }
//! ```
//!
//! Keys sorted, so the same mapping always gives the same bytes and a file loaded and saved
//! again is byte for byte the same.

pub mod commands;
pub mod export;
pub mod read;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ac_core::character::{Character, Loaded, Mapping, guess};
use ac_core::product;
use serde_json::{Map, Value, json};

type R<T> = Result<T, String>;

/// The format's kind: files say `format: "<product id>.retarget"`.
pub const KIND: &str = "retarget";
pub const VERSION: u64 = 1;
/// The file name ending the app's dialogs use.
pub const EXT: &str = ".retarget.json";

/// The mapping as a file's text.
pub fn save_text(loaded: &Loaded) -> R<String> {
    let mut d: BTreeMap<String, Value> = BTreeMap::new();
    d.insert("character".into(), json!(loaded.path.to_string_lossy()));
    d.insert("format".into(), json!(product::format(KIND)));
    d.insert("version".into(), json!(VERSION));
    d.insert("hands".into(), json!(loaded.mapping.hands));
    d.insert("joints".into(), json!(loaded.mapping.joints));
    let mut text =
        serde_json::to_string_pretty(&d).map_err(|e| format!("the mapping didn't serialise: {e}"))?;
    text.push('\n');
    Ok(text)
}

/// A mapping file's text: the character path as stored, and the mapping. Checked, not resolved.
pub fn load_text(text: &str) -> R<(PathBuf, Mapping)> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("not a mapping file: not JSON ({e})"))?;
    let d = v.as_object().ok_or("not a mapping file: not a JSON object")?;
    let format = d.get("format").and_then(Value::as_str).unwrap_or_default();
    if !product::is_format(format, KIND) {
        return Err(format!("not a retarget mapping file (format {format:?})"));
    }
    let version = d
        .get("version")
        .and_then(Value::as_u64)
        .ok_or("not a mapping file: no version")?;
    if version > VERSION {
        return Err(format!(
            "written by a newer version of {} (mapping version {version}; this one reads up to {VERSION})",
            product::NAME
        ));
    }
    let character = d
        .get("character")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or("the mapping names no character file")?;
    let mut joints = BTreeMap::new();
    for (k, v) in d
        .get("joints")
        .and_then(Value::as_object)
        .ok_or("joints: expected an object of landmark -> node name")?
    {
        if !Mapping::is_landmark(k) {
            return Err(format!("joints: {k:?} is not a landmark"));
        }
        let name = v
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("joints: {k:?} must name a node"))?;
        joints.insert(k.clone(), name.to_string());
    }
    let hands = match d.get("hands") {
        None => false,
        Some(h) => h.as_bool().ok_or("hands: expected true or false")?,
    };
    Ok((PathBuf::from(character), Mapping { joints, hands }))
}

/// Whether a file is a mapping file (by its text) rather than a .glb.
fn is_mapping_file(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json"))
}

/// Apply `joints` corrections to a mapping on `c`: every name must be a node, every key a landmark.
pub fn correct(c: &Character, m: &mut Mapping, joints: &BTreeMap<String, String>) -> R<Vec<String>> {
    let mut given = Vec::new();
    for (lm, name) in joints {
        if !Mapping::is_landmark(lm) {
            return Err(format!(
                "joints: {lm:?} is not a landmark; the landmarks are {}, and optionally {}",
                ac_core::character::REQUIRED.join(", "),
                ac_core::character::OPTIONAL.join(", ")
            ));
        }
        if name.is_empty() || name == "none" {
            m.joints.remove(lm);
        } else {
            c.node(name)
                .map_err(|e| format!("joints: {lm}: {e}; the character's nodes: {}", node_list(c)))?;
            m.joints.insert(lm.clone(), name.clone());
        }
        given.push(lm.clone());
    }
    Ok(given)
}

fn node_list(c: &Character) -> String {
    let mut names: Vec<&str> = c
        .nodes
        .iter()
        .map(|n| n.name.as_str())
        .filter(|n| !n.is_empty())
        .collect();
    let more = names.len().saturating_sub(60);
    names.truncate(60);
    format!(
        "{}{}",
        names.join(", "),
        if more > 0 {
            format!(", ... ({more} more)")
        } else {
            String::new()
        }
    )
}

/// What a load made: the character with its mapping, and the file's facts.
pub struct Target {
    pub loaded: Loaded,
    pub info: read::Info,
    /// The landmarks `joints` set.
    pub given: Vec<String>,
    /// The character file's bytes (the export writes them back with the animation).
    pub glb: Vec<u8>,
}

/// Load a character (`.glb`: the mapping is guessed) or a mapping file (its character is read),
/// with `joints` corrections and the hands switch on top.
pub fn load(path: &Path, joints: &BTreeMap<String, String>, hands: Option<bool>) -> R<Target> {
    let (character_path, stored, mapping) = if is_mapping_file(path) {
        let text =
            std::fs::read_to_string(path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
        let (stored, m) = load_text(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let resolved = if stored.is_absolute() {
            stored.clone()
        } else {
            path.parent().map_or(stored.clone(), |d| d.join(&stored))
        };
        (resolved, stored, Some(m))
    } else {
        // a .glb given by a relative path is kept absolute: a saved mapping names it from
        // anywhere, and the folder the command ran in means nothing later
        let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        (abs.clone(), abs, None)
    };
    if !character_path.is_file() {
        return Err(format!(
            "no character file at {} (a .glb)",
            character_path.display()
        ));
    }
    let size = std::fs::metadata(&character_path)
        .map_err(|e| format!("can't read {}: {e}", character_path.display()))?
        .len();
    if size > read::MAX_FILE_BYTES {
        return Err(format!(
            "{} is {size} bytes, larger than {}",
            character_path.display(),
            read::MAX_FILE_BYTES
        ));
    }
    let glb = std::fs::read(&character_path)
        .map_err(|e| format!("can't read {}: {e}", character_path.display()))?;
    let (character, info) =
        read::read_bytes(&glb).map_err(|e| format!("{}: {e}", character_path.display()))?;
    let mut notes = info.notes.clone();
    let mut mapping = match mapping {
        Some(m) => {
            // a mapping file's names must still be in the character
            for (lm, name) in &m.joints {
                character
                    .node(name)
                    .map_err(|e| format!("{}: {lm}: {e}", path.display()))?;
            }
            m
        }
        None => {
            let (m, g_notes) = guess(&character);
            notes.extend(g_notes);
            m
        }
    };
    let given = correct(&character, &mut mapping, joints)?;
    if let Some(h) = hands {
        mapping.hands = h;
    }
    let missing = mapping.missing();
    if !missing.is_empty() {
        notes.push(format!(
            "no node found for {}: name them with joints before checking or exporting",
            missing.join(", ")
        ));
    }
    Ok(Target {
        loaded: Loaded {
            path: stored,
            file: character_path,
            character,
            mapping,
            notes,
        },
        info,
        given,
        glb,
    })
}

/// The open animation as shown through the loaded character: the plan and the report (frames'
/// worlds, IK weights, the feet's numbers, the problems), as `retarget.check` works it out.
pub fn check_document(
    doc: &mut ac_core::Document,
) -> R<(ac_core::character::plan::Plan, ac_core::character::check::Report)> {
    commands::drive_document(doc)
}

/// The command result that describes a loaded character and its mapping.
pub fn report(
    sk: &ac_core::Skeleton,
    loaded: &Loaded,
    given: &[String],
    info: Option<&read::Info>,
) -> Map<String, Value> {
    let c = &loaded.character;
    let mut out = Map::new();
    out.insert("character".into(), json!(loaded.path.to_string_lossy()));
    out.insert("nodes".into(), json!(c.len()));
    out.insert("skin_joints".into(), json!(c.skin.len()));
    out.insert("mesh".into(), json!(c.has_mesh));
    out.insert("joints".into(), json!(loaded.mapping.joints));
    out.insert("given".into(), json!(given));
    out.insert("hands".into(), json!(loaded.mapping.hands));
    let missing = loaded.mapping.missing();
    out.insert("missing".into(), json!(missing));
    let mut notes = loaded.notes.clone();
    let names = |ix: &[usize]| -> Vec<String> {
        ix.iter()
            .filter_map(|&i| c.nodes.get(i).map(|n| n.name.clone()))
            .collect()
    };
    match ac_core::character::plan::Plan::new(sk, c, &loaded.mapping) {
        Ok(plan) => {
            out.insert("ready".into(), json!(true));
            out.insert("driven".into(), json!(plan.drives.len()));
            out.insert("undriven".into(), json!(names(&plan.undriven)));
            out.insert("outside".into(), json!(names(&plan.outside)));
            out.insert("scale".into(), json!(plan.scale));
            out.insert("floor".into(), json!(plan.floor));
            notes.extend(plan.notes);
        }
        Err(e) => {
            out.insert("ready".into(), json!(false));
            out.insert("driven".into(), json!(0));
            out.insert("undriven".into(), json!(Vec::<String>::new()));
            out.insert("outside".into(), json!(Vec::<String>::new()));
            out.insert("scale".into(), Value::Null);
            out.insert("floor".into(), Value::Null);
            notes.push(format!("not ready to export: {e}"));
        }
    }
    if let Some(i) = info {
        out.insert(
            "source".into(),
            json!({ "nodes": i.nodes, "skin_joints": i.skin_joints, "meshes": i.meshes, "animations": i.animations }),
        );
    }
    out.insert("notes".into(), json!(notes));
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn a_mapping_file_round_trips_and_a_wrong_one_is_refused() {
        let loaded = Loaded {
            path: PathBuf::from("hero.glb"),
            file: PathBuf::from("hero.glb"),
            character: Character::new(
                vec![ac_core::character::Node {
                    name: "Hips".into(),
                    parent: None,
                    translation: [0.0; 3],
                    rotation: ac_core::quat::IDENTITY,
                    scale: 1.0,
                }],
                vec![],
                vec![],
                false,
            )
            .unwrap(),
            mapping: Mapping {
                joints: [("hips".to_string(), "Hips".to_string())].into(),
                hands: true,
            },
            notes: vec![],
        };
        let text = save_text(&loaded).unwrap();
        let (p, m) = load_text(&text).unwrap();
        assert_eq!(p, PathBuf::from("hero.glb"));
        assert_eq!(m, loaded.mapping);
        assert!(
            load_text("{\"format\": \"x.clip\"}")
                .unwrap_err()
                .contains("not a retarget")
        );
        assert!(
            load_text(&text.replace("\"hips\"", "\"elbowz\""))
                .unwrap_err()
                .contains("not a landmark")
        );
        assert!(
            load_text(&text.replace("\"version\": 1", "\"version\": 9"))
                .unwrap_err()
                .contains("newer")
        );
    }
}
