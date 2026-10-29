//! Saving and deleting the open animation: the timeline's Save button (it reads "Unsaved changes"
//! while the library doesn't hold the edits), the prompt before switching away from unsaved work,
//! and the delete confirmation.
//!
//! Built-ins are never saved over (`clip.save` refuses them): saving one saves your own copy.

use egui::{Color32, RichText};
use serde_json::{Value, json};

use super::ui_ids::Tag;
use super::{App, library};

/// Switching to another animation (asked first when the open one has unsaved changes).
pub enum Switch {
    Open(String),
    New(Value),
    /// Quit the app (the window's close, File > Exit).
    Quit,
}

/// A question on screen.
pub enum Confirm {
    /// The open animation has unsaved changes: save before this switch?
    Switch(Switch),
    /// Delete this library animation?
    Delete(String),
    /// Delete this library pose?
    DeletePose(String),
}

impl App {
    /// The library's animation `id` is a built-in (`Some(true)`), your own (`Some(false)`), or
    /// not in the list (`None`).
    fn builtin(&self, id: &str) -> Option<bool> {
        self.library["animations"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|it| it["id"].as_str() == Some(id))
            .map(|it| it["builtin"].as_bool() == Some(true))
    }

    /// `id` is one of your own animations in the library (not a built-in).
    pub(super) fn is_mine(&self, id: &str) -> bool {
        self.builtin(id) == Some(false)
    }

    /// The open animation is one of the built-ins (saving makes your own copy).
    fn open_is_builtin(&self) -> bool {
        let doc = &self.session.document;
        doc.clip_id.as_deref().and_then(|id| self.builtin(id)) == Some(true)
    }

    /// The open animation's name as a person reads it.
    fn open_name(&self) -> String {
        let doc = &self.session.document;
        match &doc.clip_id {
            Some(id) => id.split_once('/').map_or(id.as_str(), |x| x.1).to_string(),
            None => doc.clip.name.clone(),
        }
    }

    /// Save the open animation (a built-in: your own copy). Whether it was saved; a refusal is in
    /// the status bar.
    pub(super) fn save(&mut self) -> bool {
        if !self.session.document.unsaved() {
            self.status = "nothing to save: the library has every change".into();
            return true;
        }
        let builtin = self.open_is_builtin();
        let (id, what) = if builtin {
            ("clip.save_as", "saved your own copy:")
        } else {
            ("clip.save", "saved")
        };
        let before = self.session.document.clip_id.clone();
        let Some(r) = self.run(id, json!({})) else {
            return false;
        };
        self.status = format!("{what} {}", r["id"].as_str().unwrap_or_default());
        if builtin {
            self.lib_tab = library::Tab::Animations;
        }
        // the library has the work now: its recovery file goes (under the id it was kept by,
        // when saving gave it another)
        self.clear_recovery();
        if before != self.session.document.clip_id {
            let file = ac_io::recovery::file_name(before.as_deref());
            let kept = self
                .session
                .document
                .library
                .as_ref()
                .is_some_and(|l| l.join(ac_io::recovery::FOLDER).join(&file).is_file());
            if kept {
                self.run("recovery.clear", json!({ "file": file }));
            }
        }
        self.refresh_library();
        true
    }

    /// The timeline's Save button: "Unsaved changes" (highlighted) while there are any.
    pub(super) fn save_button(&mut self, ui: &mut egui::Ui) {
        let unsaved = self.session.document.unsaved();
        let builtin = self.open_is_builtin();
        let r = if unsaved {
            ui.add(
                egui::Button::new(RichText::new("Unsaved changes").color(Color32::BLACK))
                    .fill(self.palette().unsaved),
            )
            .on_hover_text(if builtin {
                "A built-in: Save keeps your own copy under Animations (Ctrl+S)"
            } else {
                "Save (Ctrl+S)"
            })
        } else {
            ui.add_enabled(false, egui::Button::new("Saved"))
                .on_disabled_hover_text("The library has every change")
        };
        if r.tag("timeline.save", if unsaved { "Unsaved changes" } else { "Saved" })
            .clicked()
        {
            self.save();
        }
    }

    /// Switch animation, asking first when the open one has unsaved changes.
    pub(super) fn switch(&mut self, to: Switch) {
        if self.session.document.unsaved() {
            self.confirm = Some(Confirm::Switch(to));
        } else {
            self.do_switch(to);
        }
    }

    fn do_switch(&mut self, to: Switch) {
        match to {
            Switch::Open(id) => {
                self.run("clip.open", json!({ "id": id }));
            }
            Switch::New(params) => {
                self.run("clip.new", params);
            }
            Switch::Quit => {
                self.shell.quit_ok = true;
                self.shell.close_now = true;
                return;
            }
        }
        self.timeline.selected.clear();
        self.refresh_library();
    }

    pub(super) fn delete_animation(&mut self, id: &str) {
        if self.session.document.clip_id.as_deref() == Some(id) {
            // deleted on purpose: its unsaved work isn't offered back later
            self.clear_recovery();
        }
        if self.run("clip.delete", json!({ "id": id })).is_some() {
            self.status = format!("deleted {id} (Ctrl+Z brings it back to save again)");
            self.refresh_library();
        }
    }

    /// The question on screen, if any.
    pub(super) fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(confirm) = self.confirm.take() else {
            return;
        };
        let builtin = self.open_is_builtin();
        let name = self.open_name();
        let mut answer = None;
        let modal = egui::Modal::new(egui::Id::unique("confirm")).show(ctx, |ui| {
            ui.set_max_width(360.0);
            match &confirm {
                Confirm::Switch(_) => {
                    ui.heading(format!("Save changes to “{name}”?"));
                    ui.label(if builtin {
                        "It's a built-in: saving keeps your own copy under Animations."
                    } else {
                        "Your changes to this animation aren't saved."
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let save = if builtin { "Save as copy" } else { "Save" };
                        if ui.button(save).tag("dialog.confirm.save", save).clicked() {
                            answer = Some("save");
                        }
                        if ui
                            .button("Don't save")
                            .tag("dialog.confirm.discard", "Don't save")
                            .clicked()
                        {
                            answer = Some("discard");
                        }
                        if ui
                            .button("Cancel")
                            .tag("dialog.confirm.cancel", "Cancel")
                            .clicked()
                        {
                            answer = Some("cancel");
                        }
                    });
                }
                Confirm::DeletePose(id) => {
                    let short = id.split_once('/').map_or(id.as_str(), |x| x.1);
                    ui.heading(format!("Delete the pose “{short}”?"));
                    ui.label("Its file is removed from your library.");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui
                            .button("Delete")
                            .tag("dialog.confirm.delete", "Delete")
                            .clicked()
                        {
                            answer = Some("delete");
                        }
                        if ui
                            .button("Cancel")
                            .tag("dialog.confirm.cancel", "Cancel")
                            .clicked()
                        {
                            answer = Some("cancel");
                        }
                    });
                }
                Confirm::Delete(id) => {
                    let short = id.split_once('/').map_or(id.as_str(), |x| x.1);
                    ui.heading(format!("Delete “{short}”?"));
                    ui.label("Its file is removed from your library.");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui
                            .button("Delete")
                            .tag("dialog.confirm.delete", "Delete")
                            .clicked()
                        {
                            answer = Some("delete");
                        }
                        if ui
                            .button("Cancel")
                            .tag("dialog.confirm.cancel", "Cancel")
                            .clicked()
                        {
                            answer = Some("cancel");
                        }
                    });
                }
            }
        });
        if answer.is_none() && modal.should_close() {
            answer = Some("cancel");
        }
        match (answer, confirm) {
            (None, confirm) => self.confirm = Some(confirm),
            (Some("save"), Confirm::Switch(to)) => {
                // a refused save keeps the work open (the status bar says why)
                if self.save() {
                    self.do_switch(to);
                }
            }
            (Some("discard"), Confirm::Switch(to)) => {
                // the changes are dropped on purpose: no recovery file for them
                self.clear_recovery();
                self.do_switch(to);
            }
            (Some("delete"), Confirm::DeletePose(id)) => {
                if self.run("pose.delete", json!({ "id": id })).is_some() {
                    self.status = format!("deleted the pose {id}");
                    self.refresh_library();
                }
            }
            (Some("delete"), Confirm::Delete(id)) => self.delete_animation(&id),
            _ => {}
        }
    }
}
