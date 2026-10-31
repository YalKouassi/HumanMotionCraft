//! What the window remembers between runs about files: recent animations, the last one open, the
//! export folders and choices, and whether the welcome and tips were seen.
//!
//! The window works on an [`AppState`]; it is kept in the preferences' `files` section
//! (`prefs.rs`: the person's choices once, the recent list per library folder). Older builds kept
//! it beside the library (`<library>/.app-state.json`); that file is read once, the first time
//! the window sees the library, and moved in ([`AppState::merge_older`]). Every field is
//! `#[serde(default)]`: an older or newer file still reads.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::super::prefs::{FilesPrefs, LibraryPrefs, library_key};

/// The file, under the library folder.
pub const FILE: &str = ".app-state.json";
/// Recent animations kept.
pub const MAX_RECENT: usize = 10;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppState {
    /// Library ids of the animations opened last, newest first.
    pub recent: Vec<String>,
    /// The animation open when the window was last used (reopened at launch).
    pub last_open: Option<String>,
    /// Where .glb files were exported last.
    pub export_dir: Option<String>,
    /// The export's joints: full, game, body or bones.
    pub export_profile: Option<String>,
    /// The bone list (comma-separated) for the "bones" profile.
    pub export_bones: String,
    /// The export's root motion bone: hips, ground or in_place.
    pub export_root: Option<String>,
    /// Where preview videos and contact sheets went last.
    pub preview_dir: Option<String>,
    /// mp4 or gif.
    pub preview_format: Option<String>,
    /// Where a project file was opened or saved last.
    pub project_dir: Option<String>,
    /// The first-run welcome was shown.
    pub welcome_seen: bool,
    /// The tips tour was finished or dismissed.
    pub tips_seen: bool,
    /// The next-step window after a block-out was asked not to show again.
    pub guide_off: bool,
}

impl AppState {
    pub fn path(library: &Path) -> PathBuf {
        library.join(FILE)
    }

    /// The state kept for `library`: `Ok(None)` when there is none yet (a first run).
    pub fn load(library: &Path) -> Result<Option<AppState>, String> {
        let p = AppState::path(library);
        if !p.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| format!("{}: {e}", p.display()))
    }

    /// Take what an older build kept: its recent list and last animation, and its choices
    /// where this state has none (a welcome or tips seen stays seen).
    pub fn merge_older(&mut self, old: AppState) {
        if self.recent.is_empty() {
            self.recent = old.recent;
            self.recent.truncate(MAX_RECENT);
        }
        self.last_open = self.last_open.take().or(old.last_open);
        self.export_dir = self.export_dir.take().or(old.export_dir);
        self.export_profile = self.export_profile.take().or(old.export_profile);
        if self.export_bones.is_empty() {
            self.export_bones = old.export_bones;
        }
        self.export_root = self.export_root.take().or(old.export_root);
        self.preview_dir = self.preview_dir.take().or(old.preview_dir);
        self.preview_format = self.preview_format.take().or(old.preview_format);
        self.project_dir = self.project_dir.take().or(old.project_dir);
        self.welcome_seen |= old.welcome_seen;
        self.tips_seen |= old.tips_seen;
    }

    /// `id` was opened: first in the recent list, and the one to reopen.
    pub fn opened(&mut self, id: &str) {
        self.recent.retain(|r| r != id);
        self.recent.insert(0, id.to_string());
        self.recent.truncate(MAX_RECENT);
        self.last_open = Some(id.to_string());
    }
}

impl FilesPrefs {
    /// Whether the preferences hold anything for this library folder.
    pub fn knows(&self, library: &Path) -> bool {
        self.libraries.contains_key(&library_key(library))
    }

    /// The window's state for this library: the person's choices and the library's own list.
    pub fn state_for(&self, library: &Path) -> AppState {
        let lib = self
            .libraries
            .get(&library_key(library))
            .cloned()
            .unwrap_or_default();
        AppState {
            recent: lib.recent,
            last_open: lib.last_open,
            export_dir: self.export_dir.clone(),
            export_profile: self.export_profile.clone(),
            export_bones: self.export_bones.clone(),
            export_root: self.export_root.clone(),
            preview_dir: self.preview_dir.clone(),
            preview_format: self.preview_format.clone(),
            project_dir: self.project_dir.clone(),
            welcome_seen: self.welcome_seen,
            tips_seen: self.tips_seen,
            guide_off: self.guide_off,
        }
    }

    /// Keep the window's state: the choices, and the recent list under this library.
    pub fn keep(&mut self, library: &Path, s: &AppState) {
        self.export_dir = s.export_dir.clone();
        self.export_profile = s.export_profile.clone();
        self.export_bones = s.export_bones.clone();
        self.export_root = s.export_root.clone();
        self.preview_dir = s.preview_dir.clone();
        self.preview_format = s.preview_format.clone();
        self.project_dir = s.project_dir.clone();
        self.welcome_seen = s.welcome_seen;
        self.tips_seen = s.tips_seen;
        self.guide_off = s.guide_off;
        self.libraries.insert(
            library_key(library),
            LibraryPrefs {
                recent: s.recent.clone(),
                last_open: s.last_open.clone(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn recent_keeps_the_newest_first_once_each() {
        let mut s = AppState::default();
        for id in ["A/a", "A/b", "A/a"] {
            s.opened(id);
        }
        assert_eq!(s.recent, ["A/a", "A/b"]);
        for i in 0..20 {
            s.opened(&format!("B/{i}"));
        }
        assert_eq!(s.recent.len(), MAX_RECENT);
        assert_eq!(s.last_open.as_deref(), Some("B/19"));
    }

    #[test]
    fn recent_lists_are_per_library_and_choices_are_shared() {
        let mut p = FilesPrefs::default();
        let (a, b) = (Path::new("lib-a-not-there"), Path::new("lib-b-not-there"));
        let mut s = p.state_for(a);
        s.opened("A/one");
        s.export_dir = Some("out".into());
        s.welcome_seen = true;
        p.keep(a, &s);
        let other = p.state_for(b);
        assert!(other.recent.is_empty() && other.last_open.is_none());
        assert_eq!(other.export_dir.as_deref(), Some("out"));
        assert!(other.welcome_seen && p.knows(a) && !p.knows(b));
        assert_eq!(p.state_for(a).recent, ["A/one"]);
    }

    #[test]
    fn an_older_file_moves_in_without_losing_newer_choices() {
        let mut s = AppState {
            export_dir: Some("new".into()),
            ..AppState::default()
        };
        let old: AppState = serde_json::from_str(
            r#"{"recent": ["A/x", "A/y"], "last_open": "A/x", "export_dir": "old", "preview_format": "gif", "tips_seen": true}"#,
        )
        .unwrap();
        s.merge_older(old);
        assert_eq!(s.recent, ["A/x", "A/y"]);
        assert_eq!(s.last_open.as_deref(), Some("A/x"));
        assert_eq!(s.export_dir.as_deref(), Some("new"));
        assert_eq!(s.preview_format.as_deref(), Some("gif"));
        assert!(s.tips_seen && !s.welcome_seen);
    }

    #[test]
    fn the_export_root_is_remembered_and_an_older_file_has_none() {
        let mut p = FilesPrefs::default();
        let lib = Path::new("lib-not-there");
        let mut s = p.state_for(lib);
        s.export_root = Some("ground".into());
        p.keep(lib, &s);
        let saved = serde_json::to_string(&p).unwrap();
        let back: FilesPrefs = serde_json::from_str(&saved).unwrap();
        assert_eq!(back.state_for(lib).export_root.as_deref(), Some("ground"));
        let old: FilesPrefs = serde_json::from_str(r#"{"export_profile": "game"}"#).unwrap();
        assert_eq!(old.export_profile.as_deref(), Some("game"));
        assert_eq!(old.export_root, None);
    }

    #[test]
    fn unknown_and_missing_fields_still_read() {
        let s: AppState = serde_json::from_str(r#"{"welcome_seen": true, "later": 3}"#).unwrap();
        assert!(s.welcome_seen && s.recent.is_empty());
    }
}
