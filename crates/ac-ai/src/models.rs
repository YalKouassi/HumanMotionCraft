//! Where models come from (`ac_core::ai::ModelSource`, roadmap 155000): the app's own models
//! folder, the bundled `<assets>/models`, and any folder `ai.models` was given, in that order (the
//! first model with a tag wins). Each model's card is read and checked against the rig and, for
//! an add-on, against its base (roadmap 154000); one that fails is listed with the reason, never
//! loaded (Rule 1: the command that asked for it says so and uses the base instead).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ac_core::ai::{Inbetweener, ModelBase, ModelCard, ModelSource};
use ac_core::rig::Skeleton;

use crate::Model;

/// Most model folders scanned in one listing (the person's folders are hostile input too).
const MAX_FOLDERS: usize = 16;

/// Most models listed from one folder.
const MAX_PER_FOLDER: usize = 64;

/// The models a session can load.
#[derive(Debug, Clone)]
pub struct Models {
    bundled: PathBuf,
    /// The app's own models folder (`ac_io::library::app_models_dir`), when the front end wants
    /// it looked at: the app does, automated runs and the CLI keep to the bundled ones.
    installed: Option<PathBuf>,
}

/// One model folder found, before its card is read.
struct Found {
    dir: PathBuf,
    source: &'static str,
}

impl Models {
    /// The bundled `<assets>/models` only (the CLI, fixtures, MCP: reproducible).
    pub fn bundled(assets: &Path) -> Models {
        Models {
            bundled: assets.join("models"),
            installed: None,
        }
    }

    /// The installed models under `installed` first, then the bundled ones (the app).
    pub fn with_installed(assets: &Path, installed: &Path) -> Models {
        Models {
            bundled: assets.join("models"),
            installed: Some(installed.to_path_buf()),
        }
    }

    /// The folders looked in, in resolution order, each with its source name.
    fn roots<'a>(&'a self, folders: &'a [PathBuf]) -> Vec<(&'a Path, &'static str)> {
        let mut roots: Vec<(&Path, &'static str)> = Vec::new();
        if let Some(own) = &self.installed {
            roots.push((own.as_path(), "installed"));
        }
        roots.push((self.bundled.as_path(), "bundled"));
        for f in folders.iter().take(MAX_FOLDERS) {
            roots.push((f.as_path(), "folder"));
        }
        roots
    }

    /// Every model folder under the roots (`<root>/<tag>/weights.json`), by name within a root.
    fn found(&self, folders: &[PathBuf]) -> Vec<Found> {
        let mut out = Vec::new();
        for (root, source) in self.roots(folders) {
            let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.join("weights.json").is_file())
                .collect();
            dirs.sort();
            out.extend(
                dirs.into_iter()
                    .take(MAX_PER_FOLDER)
                    .map(|dir| Found { dir, source }),
            );
        }
        out
    }

    /// The folder a fresh session's model comes from: the first root with a model, its
    /// [`crate::DEFAULT_TAG`] else its first by name (as `shared_in` picks it).
    fn default_dir(&self, folders: &[PathBuf]) -> Option<PathBuf> {
        self.roots(folders)
            .into_iter()
            .filter(|(root, _)| crate::has_model(root))
            .find_map(|(root, _)| crate::default_dir(root).ok())
    }

    /// The cards, and for each model that loads its model, in resolution order.
    fn listed(&self, sk: &Skeleton, folders: &[PathBuf]) -> Vec<(ModelCard, Option<Arc<Model>>)> {
        let default = self.default_dir(folders);
        let loaded: Vec<(Found, Result<Arc<Model>, String>)> = self
            .found(folders)
            .into_iter()
            .map(|f| {
                let m = crate::shared_model(&f.dir);
                (f, m)
            })
            .collect();
        // the first model with a tag wins; later ones with the same tag are hidden by it
        let mut taken: BTreeMap<String, PathBuf> = BTreeMap::new();
        let mut out = Vec::with_capacity(loaded.len());
        for (f, m) in &loaded {
            let is_default = default.as_deref() == Some(f.dir.as_path());
            let mut card = match m {
                Ok(m) => card_of(m, &f.dir, f.source, is_default),
                Err(e) => broken(&f.dir, f.source, e),
            };
            let Ok(m) = m else {
                out.push((card, None));
                continue;
            };
            let mut why: Vec<String> = Vec::new();
            match taken.get(&card.tag) {
                Some(other) => why.push(format!(
                    "the tag {:?} is taken by the model under {}",
                    card.tag,
                    other.display()
                )),
                None => {
                    taken.insert(card.tag.clone(), f.dir.clone());
                }
            }
            let missing: Vec<&str> = card
                .bones
                .iter()
                .filter(|b| sk.bone(b).is_none())
                .map(String::as_str)
                .collect();
            if !missing.is_empty() {
                why.push(format!("bones the rig doesn't have: {}", missing.join(", ")));
            }
            if let Some(base) = &card.base {
                why.extend(base_check(&card, base, &loaded));
            }
            if !why.is_empty() {
                card.loads = false;
                card.refused = Some(why.join("; "));
            }
            let ok = card_ok(&card);
            out.push((card, ok.then(|| m.clone())));
        }
        out
    }
}

fn card_ok(card: &ModelCard) -> bool {
    card.loads && card.refused.is_none()
}

/// Why an add-on doesn't fit its base: the base isn't here, its weights changed since, or the
/// add-on's bones, context or gap differ from it (what the app expects of an add-on is what its
/// base has).
fn base_check(
    card: &ModelCard,
    base: &ModelBase,
    loaded: &[(Found, Result<Arc<Model>, String>)],
) -> Vec<String> {
    let Some(b) = loaded
        .iter()
        .filter_map(|(_, m)| m.as_ref().ok())
        .find(|m| m.info().tag == base.tag)
    else {
        return vec![format!(
            "an add-on to {:?}, which isn't here (ai.models lists what is)",
            base.tag
        )];
    };
    let mut why = Vec::new();
    if b.hash_hex() != base.hash {
        why.push(format!(
            "an add-on made for {} with hash {}, not the {} here (ai.models shows its hash)",
            base.tag, base.hash, base.tag
        ));
    }
    let bi = b.info();
    if bi.bones != card.bones {
        why.push(format!(
            "its bones differ from its base {} ({} against {})",
            base.tag,
            card.bones.len(),
            bi.bones.len()
        ));
    }
    if bi.context != card.context || bi.max_gap != card.max_gap || bi.context_after != card.context_after {
        why.push(format!(
            "its window differs from its base {}: context {} / {}, gap {} / {}, after {} / {}",
            base.tag,
            card.context,
            bi.context,
            card.max_gap,
            bi.max_gap,
            card.context_after,
            bi.context_after
        ));
    }
    why
}

/// The card of a model that loaded (the checks come after).
fn card_of(m: &Model, dir: &Path, source: &'static str, default: bool) -> ModelCard {
    let info = m.info();
    ModelCard {
        tag: info.tag.clone(),
        name: m.name().to_string(),
        hash: m.hash_hex().to_string(),
        bones: info.bones.clone(),
        context: info.context,
        max_gap: info.max_gap,
        context_after: info.context_after,
        multi_key: info.multi_key,
        styles: info.styles.clone(),
        given: info.given,
        commercial: info.commercial,
        warning: info.warning.clone(),
        base: m.base().map(|(tag, hash)| ModelBase {
            tag: tag.to_string(),
            hash: hash.to_string(),
        }),
        path: dir.to_string_lossy().into_owned(),
        source: source.to_string(),
        default,
        loads: true,
        refused: None,
    }
}

/// The card of a model that doesn't load: its folder's name for a tag and the reason.
fn broken(dir: &Path, source: &'static str, why: &str) -> ModelCard {
    ModelCard {
        tag: dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        name: String::new(),
        hash: String::new(),
        bones: Vec::new(),
        context: 0,
        max_gap: 0,
        context_after: 0,
        multi_key: false,
        styles: Vec::new(),
        given: false,
        commercial: false,
        warning: String::new(),
        base: None,
        path: dir.to_string_lossy().into_owned(),
        source: source.to_string(),
        default: false,
        loads: false,
        refused: Some(why.to_string()),
    }
}

impl ModelSource for Models {
    fn cards(&self, sk: &Skeleton, folders: &[PathBuf]) -> Vec<ModelCard> {
        self.listed(sk, folders).into_iter().map(|(c, _)| c).collect()
    }

    fn load(&self, sk: &Skeleton, folders: &[PathBuf], tag: &str) -> Result<Arc<dyn Inbetweener>, String> {
        let listed = self.listed(sk, folders);
        let mut tags: Vec<String> = Vec::new();
        for (card, model) in listed {
            if card.tag == tag {
                return match (model, card.refused) {
                    (Some(m), None) => Ok(m),
                    (_, Some(why)) => Err(format!("model {tag}: {why}")),
                    (None, None) => Err(format!("model {tag}: did not load")),
                };
            }
            if !tags.contains(&card.tag) {
                tags.push(card.tag);
            }
        }
        let known: Vec<&str> = tags.iter().map(String::as_str).collect();
        let hint = ac_core::suggest::did_you_mean(&ac_core::suggest::closest(tag, known.iter().copied()));
        Err(ac_core::error::hinted(
            format!("no model {tag:?}"),
            if hint.is_empty() {
                format!("the models here: {}", tags.join(", "))
            } else {
                format!("{hint} the models here: {}", tags.join(", "))
            },
        ))
    }
}
