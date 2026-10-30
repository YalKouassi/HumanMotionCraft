//! What the library got from the community: `<library>/.community.json`, the clip each installed
//! item came from and who to credit for it. The item's own `origin` (`community:<id>`) says the
//! same in the file; the record keeps the credit, which a clip file has no place for.
//!
//! Here, under `ac-io` rather than `ac-community`, because `library.sync` writes it too: a built-in that moved to the
//! community stays in the library as a community install (`builtins.rs`).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

type R<T> = Result<T, String>;

pub const RECORD: &str = ".community.json";
/// Where `community.fetch` unpacks clips to play, beside the items (a dot folder is not an item).
pub const CACHE: &str = ".community-cache";
/// The group installs go into.
pub const GROUP: &str = "Community";
/// `origin` of an installed clip: this, then the clip's id.
pub const ORIGIN: &str = "community:";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Install {
    pub clip: String,
    pub title: String,
    pub author: String,
    pub licence: String,
    pub credit: String,
}

/// By library id.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Record {
    pub installs: BTreeMap<String, Install>,
}

impl Record {
    /// The record in `root` (empty when there is none yet).
    pub fn load(root: &Path) -> R<Record> {
        match std::fs::read_to_string(root.join(RECORD)) {
            Ok(t) => serde_json::from_str(&t).map_err(|e| format!("{RECORD}: {e}")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Record::default()),
            Err(e) => Err(format!("can't read {RECORD}: {e}")),
        }
    }

    pub fn save(&self, root: &Path) -> R<()> {
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        craft_write(&root.join(RECORD), &text)
    }

    /// The library id holding community clip `clip`, if it's still in the library.
    pub fn installed(&self, clip: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
        self.installs
            .iter()
            .find(|(id, i)| i.clip == clip && exists(id))
            .map(|(id, _)| id.clone())
    }
}

fn craft_write(path: &Path, text: &str) -> R<()> {
    craft_atomic::atomic_write(path, text.as_bytes())
        .map_err(|e| format!("can't write {}: {e}", path.display()))
}
