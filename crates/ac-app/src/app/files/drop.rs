//! Getting files in: drag-and-drop on the window, File → Import file… (Ctrl+I), and File → Recent.
//!
//! A dropped `.clip.json` or `.pose.json` goes into the library's Imported group
//! (`library.add_file`); a dropped folder imports like Import library → From a folder
//! (`library.import`). One animation dropped alone is opened; a pose is shown under Poses.

use std::path::PathBuf;

use serde_json::{Value, json};

use super::super::App;
use super::super::library::Tab;
use super::super::save::Switch;
use super::super::ui_ids::Tag;

/// Most files taken from one drop.
const MAX_DROP: usize = 1000;

impl App {
    /// Files dropped on the window this frame (and whether some are hovering, for the overlay).
    pub(super) fn take_drops(&mut self, ctx: &egui::Context) {
        let (hovering, dropped): (bool, Vec<PathBuf>) = ctx.input(|i| {
            (
                !i.raw.hovered_files.is_empty(),
                i.raw
                    .dropped_files
                    .iter()
                    .map(|f| f.path().to_path_buf())
                    .filter(|p| !p.as_os_str().is_empty())
                    .take(MAX_DROP)
                    .collect(),
            )
        });
        self.files.dropping = hovering;
        if !dropped.is_empty() {
            self.import_paths(&dropped);
        }
    }

    pub(in crate::app) fn import_file_dialog(&mut self) {
        if let Some(files) = rfd::FileDialog::new()
            .set_title("Import animations and poses")
            .add_filter("Animations and poses (*.clip.json, *.pose.json)", &["json"])
            .pick_files()
        {
            self.import_paths(&files);
        }
    }

    /// Import files and folders; say what came in and what didn't.
    pub(super) fn import_paths(&mut self, paths: &[PathBuf]) {
        let mut anims = Vec::new();
        let mut poses = Vec::new();
        let mut problems = Vec::new();
        let mut folders = 0;
        for p in paths {
            let shown = p
                .file_name()
                .map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().to_string());
            if p.is_dir() {
                match self
                    .session
                    .run("library.import", &json!({ "from": p.to_string_lossy() }))
                {
                    Ok(r) => {
                        folders += 1;
                        let ids = |v: &Value| -> Vec<String> {
                            v["copied"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(|x| x.as_str().map(str::to_string))
                                .collect()
                        };
                        anims.extend(ids(&r["animations"]));
                        poses.extend(ids(&r["poses"]));
                        for s in r["skipped"].as_array().into_iter().flatten() {
                            problems.push(format!(
                                "{shown}/{}: {}",
                                s["file"].as_str().unwrap_or_default(),
                                s["error"].as_str().unwrap_or_default()
                            ));
                        }
                    }
                    Err(e) => problems.push(format!("{shown}: {e}")),
                }
                continue;
            }
            match self
                .session
                .run("library.add_file", &json!({ "path": p.to_string_lossy() }))
            {
                Ok(r) => {
                    let id = r["id"].as_str().unwrap_or_default().to_string();
                    if r["kind"] == "pose" {
                        poses.push(id);
                    } else {
                        anims.push(id);
                    }
                }
                Err(e) => problems.push(format!("{shown}: {e}")),
            }
        }
        self.refresh_library();
        let mut said = Vec::new();
        if !anims.is_empty() {
            said.push(count(anims.len(), "animation"));
        }
        if !poses.is_empty() {
            said.push(count(poses.len(), "pose"));
        }
        let mut status = if said.is_empty() {
            "nothing imported".to_string()
        } else {
            let from = if folders > 0 {
                format!(" (from {})", count(folders, "folder"))
            } else {
                String::new()
            };
            format!("imported {}{from}", said.join(" and "))
        };
        if !problems.is_empty() {
            status.push_str(&format!("; not imported: {}", problems.join("; ")));
        }
        self.status = status;
        match (anims.as_slice(), poses.as_slice()) {
            ([one], []) => {
                self.lib_reveal(Tab::Animations);
                self.switch(Switch::Open(one.clone()));
            }
            ([], [one, ..]) => {
                self.lib_reveal(Tab::Poses);
                self.lib_select(false, one);
            }
            (_, _) if !anims.is_empty() => self.lib_reveal(Tab::Animations),
            _ => {}
        }
    }

    /// File → Recent: the animations opened last (a missing one is greyed, saying why).
    pub(in crate::app) fn recent_menu(&mut self, ui: &mut egui::Ui) {
        let recent = self.files.state.recent.clone();
        ui.add_enabled_ui(!recent.is_empty(), |ui| {
            let menu = ui.menu_button("Recent", |ui| {
                for id in &recent {
                    let there = self.library["animations"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(|it| it["id"].as_str() == Some(id.as_str())));
                    if ui
                        .add_enabled(there, egui::Button::new(id.as_str()))
                        .on_disabled_hover_text("No longer in the library")
                        .tag(format!("menu.File.Recent.{id}"), id.as_str())
                        .clicked()
                    {
                        self.lib_reveal(Tab::Animations);
                        if self.session.document.clip_id.as_deref() != Some(id.as_str()) {
                            self.switch(Switch::Open(id.clone()));
                        }
                        ui.close();
                    }
                }
                ui.separator();
                if ui
                    .button("Clear recent")
                    .tag("menu.File.Recent.clear", "Clear recent")
                    .clicked()
                {
                    self.files.state.recent.clear();
                    self.save_state();
                    ui.close();
                }
            });
            menu.response.tag("menu.File.Recent", "Recent");
        });
    }

    /// "Drop to import" while files hover over the window.
    pub(super) fn drop_overlay(&mut self, ctx: &egui::Context) {
        if !self.files.dropping {
            return;
        }
        let screen = ctx.content_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::unique("drop"),
        ));
        painter.rect_filled(screen, 0.0, egui::Color32::from_black_alpha(160));
        painter.text(
            screen.center(),
            egui::Align2::CENTER_CENTER,
            "Drop animations (.clip.json), poses (.pose.json) or folders to import them",
            egui::FontId::proportional(20.0),
            egui::Color32::WHITE,
        );
    }
}

fn count(n: usize, what: &str) -> String {
    format!("{n} {what}{}", if n == 1 { "" } else { "s" })
}
