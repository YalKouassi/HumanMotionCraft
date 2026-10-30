//! Project files in the window: File > Open project…, Save project and Save project as… (the
//! `project.open` / `project.save` commands behind native file dialogs), a dropped
//! `.project.json`, and the window title, which names the project while one is open.

use std::path::{Path, PathBuf};

use serde_json::json;

use super::super::App;
use super::super::save::Switch;

/// The filter the dialogs show.
const FILTER: &str = "Projects (*.project.json)";

/// A project file's name without its ending: `hop.project.json` is "hop".
pub fn stem(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    name.strip_suffix(ac_io::project::EXT)
        .map(str::to_string)
        .unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or(name)
        })
}

/// Whether a path names a project file (by its ending; the file's `format` decides on opening).
pub fn is_project_file(path: &Path) -> bool {
    path.to_string_lossy()
        .to_ascii_lowercase()
        .ends_with(ac_io::project::EXT)
}

impl App {
    /// The project file the document is, if any.
    pub(in crate::app) fn project_path(&self) -> Option<PathBuf> {
        self.session.document.project.as_ref().map(|p| p.path.clone())
    }

    /// Where the dialogs start: beside the open project, else where one was last, else Documents.
    fn project_start_dir(&self) -> PathBuf {
        self.project_path()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .or_else(|| self.files.state.project_dir.clone().map(PathBuf::from))
            .unwrap_or_else(super::documents)
    }

    fn remember_project_dir(&mut self, path: &Path) {
        if let Some(d) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            self.files.state.project_dir = Some(d.to_string_lossy().to_string());
            self.save_state();
        }
    }

    /// File > Open project…: pick a file, then open it (asking first when the open animation has
    /// unsaved changes).
    pub(in crate::app) fn open_project_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Open project")
            .add_filter(FILTER, &["json"])
            .set_directory(self.project_start_dir())
            .pick_file()
        else {
            return;
        };
        self.open_project(&path);
    }

    /// Open a project file, asking first when the open animation has unsaved changes.
    pub(in crate::app) fn open_project(&mut self, path: &Path) {
        self.switch(Switch::Project(path.to_path_buf()));
    }

    /// `project.open`, once any unsaved work is settled ([`App::switch`]).
    pub(in crate::app) fn do_open_project(&mut self, path: &Path) {
        let Some(r) = self.run("project.open", json!({ "path": path.to_string_lossy() })) else {
            return;
        };
        let mut status = format!(
            "opened the project {} ({} clips)",
            stem(path),
            1 + r["links"].as_array().map_or(0, Vec::len)
        );
        if r["migrated"] == json!(true) {
            status.push_str(&format!(
                "; it was a version {} file, brought up to date (saving writes the current version)",
                r["version"]
            ));
        }
        for note in r["notes"].as_array().into_iter().flatten() {
            if let Some(n) = note.as_str() {
                status.push_str("; ");
                status.push_str(n);
            }
        }
        self.status = status;
        self.remember_project_dir(path);
        self.timeline.selected.clear();
        self.refresh_library();
    }

    /// File > Save project as…: pick where, then save. Whether it was saved.
    pub(in crate::app) fn save_project_as_dialog(&mut self) -> bool {
        let doc = &self.session.document;
        let name = self.project_path().map(|p| stem(&p)).unwrap_or_else(|| {
            if doc.clip.name.is_empty() {
                "untitled".to_string()
            } else {
                doc.clip.name.clone()
            }
        });
        let Some(mut path) = rfd::FileDialog::new()
            .set_title("Save project")
            .add_filter(FILTER, &["json"])
            .set_directory(self.project_start_dir())
            .set_file_name(format!("{name}{}", ac_io::project::EXT))
            .save_file()
        else {
            return false;
        };
        if !path.to_string_lossy().to_ascii_lowercase().ends_with(".json") {
            path = PathBuf::from(format!("{}{}", path.display(), ac_io::project::EXT));
        }
        self.save_project(Some(path))
    }

    /// Save the project to `to`, or to its own file (`None`; asks where when it has none yet).
    /// Whether it was saved; a refusal is in the status bar.
    pub(in crate::app) fn save_project(&mut self, to: Option<PathBuf>) -> bool {
        let params = match &to {
            Some(p) => json!({ "path": p.to_string_lossy() }),
            None if self.session.document.project.is_some() => json!({}),
            None => return self.save_project_as_dialog(),
        };
        let Some(r) = self.run("project.save", params) else {
            return false;
        };
        let path = PathBuf::from(r["path"].as_str().unwrap_or_default());
        self.status = format!(
            "saved the project {} ({} clips, {} KB)",
            stem(&path),
            r["clips"],
            r["bytes"].as_u64().unwrap_or(0) / 1024
        );
        self.remember_project_dir(&path);
        // the file has the work now: no recovery file for it
        self.clear_recovery();
        true
    }

    /// The window title names the project while one is open.
    pub(super) fn project_title(&mut self, ctx: &egui::Context) {
        let title = match self.project_path() {
            Some(p) => format!("{} – {}", stem(&p), ac_core::product::NAME),
            None => ac_core::product::NAME.to_string(),
        };
        if self.files.title.as_deref() != Some(title.as_str()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.files.title = Some(title);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stems_drop_the_project_ending() {
        assert_eq!(stem(Path::new("C:/work/hop.project.json")), "hop");
        assert_eq!(stem(Path::new("hop.json")), "hop");
        assert_eq!(stem(Path::new("plain")), "plain");
        assert!(is_project_file(Path::new("x/Hop.Project.JSON")));
        assert!(!is_project_file(Path::new("x/walk.clip.json")));
    }
}
