//! The library's right-click menus, keyboard and small dialogs.
//!
//! - Animations: open, rename, duplicate, move to group, delete (`clip.rename/duplicate/group/
//!   delete`); built-ins: open and duplicate (your own copy).
//! - Poses: apply, apply to the selected joint's part or any part only (`pose.apply {parts}`),
//!   save the current pose, rename, move to group, delete (`pose.save/rename/group/delete`).
//! - Keys while the library has focus (it is clicked): arrows move the selection, Enter opens or
//!   applies, F2 renames, Delete deletes (asked first).

use serde_json::json;

use super::super::App;
use super::super::save::{Confirm, Switch};
use super::super::ui_ids::Tag;

/// The library's keyboard selection and its dialogs (part of the window's file state).
#[derive(Default)]
pub struct Nav {
    /// (an animation, id): the item the keys act on.
    pub sel: Option<(bool, String)>,
    /// The library was clicked last (its keys are live).
    pub focus: bool,
    /// The items on screen in order, as last drawn.
    pub order: Vec<(bool, String)>,
    /// Scroll the selection into view on the next draw.
    pub scroll: bool,
    pub dialog: Option<Dialog>,
    /// The open dialog's text field was given the keys.
    pub focused: bool,
}

/// A menu choice, run once the menu is closed.
type Action = Box<dyn FnOnce(&mut App)>;

fn choose(act: &mut Option<Action>, ui: &egui::Ui, f: Action) {
    *act = Some(f);
    ui.close();
}

pub enum Dialog {
    Rename {
        anims: bool,
        id: String,
        to: String,
    },
    Group {
        anims: bool,
        id: String,
        to: String,
    },
    SavePose {
        name: String,
        group: String,
        part: String,
    },
}

impl App {
    /// Select a library item (and give the library the keys).
    pub(in super::super) fn lib_select(&mut self, anims: bool, id: &str) {
        let nav = &mut self.files.nav;
        nav.sel = Some((anims, id.to_string()));
        nav.focus = true;
        nav.scroll = true;
    }

    /// A library key (`app.lib_*`, a context key while the library has the keyboard: up and
    /// down move the selection, Enter opens or applies, F2 renames, Delete deletes).
    pub(in super::super) fn library_key(&mut self, action: &str) {
        if matches!(action, "app.lib_prev" | "app.lib_next") {
            let down = action == "app.lib_next";
            let order = &self.files.nav.order;
            let at = self
                .files
                .nav
                .sel
                .as_ref()
                .and_then(|s| order.iter().position(|o| o == s));
            let next = match (at, down) {
                (None, true) => Some(0),
                (None, false) => order.len().checked_sub(1),
                (Some(i), true) => Some((i + 1).min(order.len().saturating_sub(1))),
                (Some(i), false) => Some(i.saturating_sub(1)),
            };
            if let Some((a, id)) = next.and_then(|i| order.get(i)).cloned() {
                self.lib_select(a, &id);
            }
            return;
        }
        let Some((anims, id)) = self.files.nav.sel.clone() else {
            self.status = "select a library item first".into();
            return;
        };
        match action {
            "app.lib_open" => self.lib_activate(anims, &id),
            "app.lib_rename" => self.lib_rename(anims, &id),
            "app.lib_delete" => self.lib_delete(anims, &id),
            _ => {}
        }
    }

    /// Open an animation, or apply a pose.
    pub(super) fn lib_activate(&mut self, anims: bool, id: &str) {
        if anims {
            if self.session.document.clip_id.as_deref() != Some(id) {
                self.switch(Switch::Open(id.to_string()));
            }
        } else if self.run("pose.apply", json!({ "id": id })).is_some() {
            self.status = format!("applied {id}");
        }
    }

    fn is_builtin_item(&self, anims: bool, id: &str) -> bool {
        let kind = if anims { "animations" } else { "poses" };
        self.library[kind]
            .as_array()
            .into_iter()
            .flatten()
            .find(|it| it["id"].as_str() == Some(id))
            .is_some_and(|it| it["builtin"].as_bool() == Some(true))
    }

    fn lib_rename(&mut self, anims: bool, id: &str) {
        if self.is_builtin_item(anims, id) {
            self.status = format!("{id} is a built-in: duplicate it to have your own to rename");
            return;
        }
        let to = id.split_once('/').map_or(id, |x| x.1).to_string();
        self.files.nav.dialog = Some(Dialog::Rename {
            anims,
            id: id.to_string(),
            to,
        });
    }

    fn lib_delete(&mut self, anims: bool, id: &str) {
        if self.is_builtin_item(anims, id) {
            self.status = format!("{id} is a built-in and stays in the library");
            return;
        }
        self.confirm = Some(if anims {
            Confirm::Delete(id.to_string())
        } else {
            Confirm::DeletePose(id.to_string())
        });
    }

    /// The groups an item can move to.
    fn groups(&self) -> Vec<String> {
        self.library["groups"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|g| g.as_str().map(str::to_string))
            .collect()
    }

    /// The part the selected joint is in (`Left arm`, ...), for "Apply to … only".
    fn selected_part(&self) -> Option<&'static str> {
        let b = self.selected?;
        ac_core::parts::rows(&self.session.document.skeleton, Some(&[b]))
            .into_iter()
            .next()
    }

    /// An item's right-click menu.
    pub(super) fn item_menu(&mut self, r: &egui::Response, anims: bool, id: &str) {
        let builtin = self.is_builtin_item(anims, id);
        let groups = self.groups();
        let part = self.selected_part();
        let custom: Vec<String> = self.session.document.custom_parts.keys().cloned().collect();
        let mut act: Option<Action> = None;
        r.context_menu(|ui| {
            let id = id.to_string();
            if anims {
                if ui.button("Open").tag("library.item_menu.open", "Open").clicked() {
                    let id = id.clone();
                    choose(&mut act, ui, Box::new(move |a| a.lib_activate(true, &id)));
                }
            } else {
                if ui
                    .button("Apply")
                    .tag("library.item_menu.apply", "Apply")
                    .clicked()
                {
                    let id = id.clone();
                    choose(&mut act, ui, Box::new(move |a| a.lib_activate(false, &id)));
                }
                if let Some(part) = part
                    && ui
                        .button(format!("Apply to the {} only", part.to_lowercase()))
                        .on_hover_text("The selected joint's part; the rest of the body stays as it is")
                        .tag(
                            "library.item_menu.apply_selected_part",
                            format!("Apply to the {} only", part.to_lowercase()),
                        )
                        .clicked()
                {
                    let id = id.clone();
                    choose(&mut act, ui, Box::new(move |a| a.apply_parts(&id, part)));
                }
                ui.menu_button("Apply to part only", |ui| {
                    let all = ac_core::parts::PRESETS
                        .iter()
                        .skip(1)
                        .map(|p| p.to_string())
                        .chain(custom);
                    for p in all {
                        if ui
                            .button(&p)
                            .tag(format!("library.item_menu.apply_part.{p}"), p.as_str())
                            .clicked()
                        {
                            let id = id.clone();
                            choose(&mut act, ui, Box::new(move |a| a.apply_parts(&id, &p)));
                        }
                    }
                })
                .response
                .tag("library.item_menu.apply_part", "Apply to part only");
                ui.separator();
                if ui
                    .button("Save current pose…")
                    .tag("library.item_menu.save_pose", "Save current pose…")
                    .clicked()
                {
                    choose(&mut act, ui, Box::new(|a| a.open_save_pose()));
                }
            }
            if builtin {
                ui.separator();
                let what = if anims { "Duplicate as my own" } else { "Duplicate" };
                if anims && ui.button(what).tag("library.item_menu.duplicate", what).clicked() {
                    let id = id.clone();
                    choose(&mut act, ui, Box::new(move |a| a.lib_duplicate(&id)));
                }
                ui.label(egui::RichText::new("Built-in: kept as it ships").weak().small());
                return;
            }
            ui.separator();
            if ui
                .button("Rename…")
                .on_hover_text("F2")
                .tag("library.item_menu.rename", "Rename…")
                .clicked()
            {
                let id = id.clone();
                choose(&mut act, ui, Box::new(move |a| a.lib_rename(anims, &id)));
            }
            if anims
                && ui
                    .button("Duplicate")
                    .tag("library.item_menu.duplicate", "Duplicate")
                    .clicked()
            {
                let id = id.clone();
                choose(&mut act, ui, Box::new(move |a| a.lib_duplicate(&id)));
            }
            ui.menu_button("Move to group", |ui| {
                let here = id.split_once('/').map_or("", |x| x.0).to_string();
                for g in groups.iter().filter(|g| **g != here) {
                    if ui
                        .button(g)
                        .tag(format!("library.item_menu.group.{g}"), g.as_str())
                        .clicked()
                    {
                        let (id, g) = (id.clone(), g.clone());
                        choose(&mut act, ui, Box::new(move |a| a.lib_group(anims, &id, &g)));
                    }
                }
                ui.separator();
                if ui
                    .button("New group…")
                    .tag("library.item_menu.new_group", "New group…")
                    .clicked()
                {
                    let id = id.clone();
                    choose(
                        &mut act,
                        ui,
                        Box::new(move |a| {
                            a.files.nav.dialog = Some(Dialog::Group {
                                anims,
                                id,
                                to: String::new(),
                            })
                        }),
                    );
                }
            })
            .response
            .tag("library.item_menu.group", "Move to group");
            ui.separator();
            if ui
                .button("Delete…")
                .on_hover_text("Delete")
                .tag("library.item_menu.delete", "Delete…")
                .clicked()
            {
                let id = id.clone();
                choose(&mut act, ui, Box::new(move |a| a.lib_delete(anims, &id)));
            }
        });
        if let Some(f) = act {
            f(self);
        }
    }

    fn apply_parts(&mut self, id: &str, part: &str) {
        if self
            .run("pose.apply", json!({ "id": id, "parts": [part] }))
            .is_some()
        {
            self.status = format!("applied {id} to the {} only", part.to_lowercase());
        }
    }

    fn lib_duplicate(&mut self, id: &str) {
        if let Some(r) = self.run("clip.duplicate", json!({ "id": id })) {
            let new = r["id"].as_str().unwrap_or_default().to_string();
            self.status = format!("duplicated {id} as {new}");
            self.refresh_library();
            self.lib_tab = super::Tab::Animations;
            self.lib_select(true, &new);
        }
    }

    fn lib_group(&mut self, anims: bool, id: &str, group: &str) {
        let cmd = if anims { "clip.group" } else { "pose.group" };
        if let Some(r) = self.run(cmd, json!({ "id": id, "group": group })) {
            let new = r["id"].as_str().unwrap_or_default().to_string();
            self.status = format!("moved {id} to {new}");
            self.refresh_library();
            self.lib_select(anims, &new);
        }
    }

    pub(super) fn open_save_pose(&mut self) {
        self.files.nav.dialog = Some(Dialog::SavePose {
            name: "My pose".into(),
            group: "My poses".into(),
            part: ac_core::parts::WHOLE_BODY.into(),
        });
    }

    /// The rename, new group and save pose dialogs.
    pub(in super::super) fn library_dialogs(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.files.nav.dialog.take() else {
            return;
        };
        let mut ok = false;
        let mut cancel = false;
        // the text field takes the keys when the dialog opens (only then: the others stay usable)
        let focused = std::mem::replace(&mut self.files.nav.focused, true);
        let parts: Vec<String> = ac_core::parts::PRESETS
            .iter()
            .map(|p| p.to_string())
            .chain(self.session.document.custom_parts.keys().cloned())
            .collect();
        let modal = egui::Modal::new(egui::Id::unique("library-dialog")).show(ctx, |ui| {
            ui.set_max_width(340.0);
            let text = match &mut d {
                Dialog::Rename { id, to, .. } => {
                    ui.heading(format!(
                        "Rename “{}”",
                        id.split_once('/').map_or(id.as_str(), |x| x.1)
                    ));
                    to
                }
                Dialog::Group { to, .. } => {
                    ui.heading("Move to a new group");
                    to
                }
                Dialog::SavePose { name, group, part } => {
                    ui.heading("Save the current pose");
                    ui.horizontal(|ui| {
                        ui.label("Group");
                        ui.text_edit_singleline(group)
                            .tag("dialog.library.group", "Group");
                    });
                    ui.horizontal(|ui| {
                        ui.label("Part");
                        egui::ComboBox::from_id_salt("save-pose-part")
                            .selected_text(part.clone())
                            .show_ui(ui, |ui| {
                                for p in &parts {
                                    ui.selectable_value(part, p.clone(), p.as_str())
                                        .tag(format!("dialog.library.part.{p}"), p.as_str());
                                }
                            })
                            .response
                            .tag("dialog.library.part", "Part");
                    });
                    ui.label("Name");
                    name
                }
            };
            let r = ui
                .add(egui::TextEdit::singleline(text).desired_width(f32::INFINITY))
                .tag("dialog.library.name", "Name");
            if !focused {
                r.request_focus();
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                ok = true;
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("OK").tag("dialog.library.ok", "OK").clicked() {
                    ok = true;
                }
                if ui
                    .button("Cancel")
                    .tag("dialog.library.cancel", "Cancel")
                    .clicked()
                {
                    cancel = true;
                }
            });
        });
        if modal.should_close() {
            cancel = true;
        }
        if ok {
            self.finish_dialog(d);
        } else if !cancel {
            self.files.nav.dialog = Some(d);
            return;
        }
        self.files.nav.focused = false;
    }

    fn finish_dialog(&mut self, d: Dialog) {
        match d {
            Dialog::Rename { anims, id, to } => {
                let cmd = if anims { "clip.rename" } else { "pose.rename" };
                if let Some(r) = self.run(cmd, json!({ "id": id, "to": to.trim() })) {
                    let new = r["id"].as_str().unwrap_or_default().to_string();
                    self.status = format!("renamed {id} to {new}");
                    self.refresh_library();
                    self.lib_select(anims, &new);
                }
            }
            Dialog::Group { anims, id, to } => self.lib_group(anims, &id, to.trim()),
            Dialog::SavePose { name, group, part } => {
                let mut p = json!({ "name": name.trim(), "group": group.trim() });
                if part != ac_core::parts::WHOLE_BODY {
                    p["parts"] = json!([part]);
                }
                if let Some(r) = self.run("pose.save", p) {
                    let id = r["id"].as_str().unwrap_or_default().to_string();
                    self.status = format!("saved the pose as {id}");
                    self.refresh_library();
                    self.lib_tab = super::Tab::Poses;
                    self.lib_select(false, &id);
                }
            }
        }
    }
}
