//! The menu bar, generated: every registered command goes in a menu by its id's area (`key.set`
//! in Key, `layer.add` in Layer), with its label and its shortcut (the user's own, from the
//! keymap); the app actions add the window's own items. craft-menu lays the menus out the
//! platform's way (Settings ends Edit and Quit ends File on Windows) and draws them.
//!
//! An item is greyed when its command would be refused and that is cheap to know (nothing to
//! undo, no preview to keep). A command with required inputs reads "…" and opens its form.
//!
//! **A new command area** lands in the menu [`menu_of`] gives it; add its prefix to [`AREAS`] to
//! place it, otherwise it goes in Edit > More.

use craft_menu::{
    Gate, ItemKind, ItemState, LayoutOptions, MenuBarUi, MenuRole, Row, Target, apply_keymap, apply_state,
    from_items, layout,
};

use super::super::App;
use super::super::ui_ids::Tag;
use super::actions::{self, ACTIONS, Show};

/// The menus, in order.
pub const MENUS: &[&str] = &[
    "File", "Edit", "Key", "Pose", "Layer", "Physics", "AI", "View", "Help",
];

/// Command areas (the id's part before the dot) and the menu each goes in.
const AREAS: &[(&str, &str)] = &[
    ("clip", "File"),
    ("library", "File"),
    ("export", "File"),
    ("project", "File"),
    ("autosave", "File"),
    ("edit", "Edit"),
    ("frame", "Key"),
    ("key", "Key"),
    ("keys", "Key"),
    ("anim", "Key"),
    ("timeline", "Key"),
    ("pose", "Pose"),
    ("pin", "Pose"),
    ("pins", "Pose"),
    ("parts", "Pose"),
    ("feet", "Pose"),
    ("bone", "Pose"),
    ("rig", "Pose"),
    ("ik", "Pose"),
    ("layer", "Layer"),
    ("layers", "Layer"),
    ("surface", "Layer"),
    ("physics", "Physics"),
    ("ai", "AI"),
    ("text", "AI"),
    ("posefill", "AI"),
    ("live", "AI"),
    ("editor", "View"),
    ("camera", "View"),
    ("view", "View"),
    ("onion", "View"),
];

/// Commands of an area that belong elsewhere.
const BY_ID: &[(&str, &str)] = &[
    ("clip.loop", "Key"),
    ("clip.curves", "Key"),
    ("clip.contacts", "Physics"),
    ("clip.range", "Pose"),
    ("clip.sample", "Key"),
    ("clip.move", "Pose"),
];

/// The menu a command goes in.
pub fn menu_of(id: &str) -> &'static str {
    if let Some((_, m)) = BY_ID.iter().find(|(i, _)| *i == id) {
        return m;
    }
    let area = id.split_once('.').map_or(id, |(a, _)| a);
    AREAS.iter().find(|(a, _)| *a == area).map_or("Edit", |(_, m)| m)
}

/// Whether an area has its own place (or falls into Edit > More).
fn placed(id: &str) -> bool {
    let area = id.split_once('.').map_or(id, |(a, _)| a);
    BY_ID.iter().any(|(i, _)| *i == id) || AREAS.iter().any(|(a, _)| *a == area)
}

fn sep(menu: &str) -> Row {
    Row {
        path: vec![menu.to_string()],
        command: "---".into(),
        label: "---".into(),
        shortcut: None,
        enabled: true,
        kind: ItemKind::Plain,
    }
}

/// The rows of the bar: commands in registry order, then the app actions, per menu.
pub fn rows(session: &ac_core::Session) -> Vec<Row> {
    let mut rows = Vec::new();
    for menu in MENUS {
        let before = rows.len();
        for spec in session.commands() {
            if actions::internal(spec.id) || menu_of(spec.id) != *menu {
                continue;
            }
            let needs = spec.inputs.iter().any(|p| p.required);
            let label = if needs {
                format!("{}…", spec.label)
            } else {
                spec.label.to_string()
            };
            let mut path = vec![menu.to_string()];
            if !placed(spec.id) {
                path.push("More".into());
            } else if spec.id.starts_with("library.") {
                path.push("Library".into());
            }
            rows.push(Row {
                path,
                command: spec.id.into(),
                label,
                shortcut: None,
                enabled: true,
                kind: ItemKind::Plain,
            });
        }
        let mut last: Option<Show> = None;
        for a in ACTIONS.iter().filter(|a| a.menu == *menu) {
            // a separator before the actions, and around each group of radio items
            if rows.len() > before
                && (last.is_none() || (last == Some(Show::Radio)) != (a.show == Show::Radio))
            {
                rows.push(sep(menu));
            }
            last = Some(a.show);
            rows.push(Row {
                path: [Some(*menu), a.sub]
                    .into_iter()
                    .flatten()
                    .map(str::to_string)
                    .collect(),
                command: a.id.into(),
                label: a.label.to_string(),
                shortcut: None,
                enabled: true,
                kind: match a.show {
                    Show::Plain => ItemKind::Plain,
                    Show::Check => ItemKind::Check(false),
                    Show::Radio => ItemKind::Radio(false),
                },
            });
        }
    }
    rows
}

impl App {
    /// Whether a command or action can run now, where that is cheap to know (otherwise yes).
    pub(in crate::app) fn enabled(&self, id: &str) -> bool {
        let doc = &self.session.document;
        let now = doc.frame.round() as i64;
        let key_here = || doc.clip.keys().iter().any(|k| k.frame == now);
        match id {
            "edit.undo" => self.session.can_undo(),
            "edit.redo" => self.session.can_redo(),
            "clip.save" => doc.unsaved(),
            "ai.accept" | "ai.discard" => doc.live_preview().is_some(),
            "ai.fill" | "ai.preview" => doc.ai.is_some(),
            "pose.paste" => doc.clipboard.is_some(),
            "pins.clear" => !doc.pins.is_empty(),
            "key.delete" => key_here(),
            "keys.delete" | "app.delete_keys" => !self.timeline.selected.is_empty() || key_here(),
            "app.key_prev" => doc.clip.keys().iter().any(|k| k.frame < now),
            "app.key_next" => doc.clip.keys().iter().any(|k| k.frame > now),
            "app.frame_prev" => now > 0,
            _ => true,
        }
    }

    /// The menu bar (and the open animation's name after it).
    pub(in crate::app) fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let mut bar = from_items(MENUS, &rows(&self.session));
        for (title, role) in [
            ("File", MenuRole::File),
            ("Edit", MenuRole::Edit),
            ("View", MenuRole::View),
            ("Help", MenuRole::Help),
        ] {
            bar.tag_menu(title, role);
        }
        for a in ACTIONS {
            if let Some(role) = a.role {
                bar.tag_item(&a.id.into(), role);
            }
        }
        apply_keymap(&mut bar, &self.shell.keys.keymap);
        apply_state(&mut bar, |c| ItemState {
            enabled: self.enabled(&c.id),
            checked: self.action_on(&c.id),
        });
        let laid = layout(
            &bar,
            Target::current(),
            &LayoutOptions::new(ac_core::product::NAME),
        );
        let gate = Gate {
            modal: self.confirm.is_some(),
            recording_shortcut: self.recording_shortcut(),
            text_editing: ui.ctx().egui_wants_keyboard_input(),
        };
        let mut chosen = Vec::new();
        ui.horizontal(|ui| {
            chosen = MenuBarUi::new(&laid.bar)
                .id(egui::Id::unique("menu-bar"))
                .gate(gate)
                .shrink_to_fit(f32::INFINITY)
                .show(ui);
            // File > Recent lists the library's own animations, so it is drawn here (`app/files`)
            self.recent_menu(ui);
            ui.separator();
            let name = self
                .session
                .document
                .clip_id
                .clone()
                .unwrap_or_else(|| self.session.document.clip.name.clone());
            ui.label(egui::RichText::new(name).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mode = self.shell.prefs.get().layout.ui_mode;
                let is_simple = mode == super::super::prefs::UiMode::Simple;
                let btn_text = if is_simple {
                    "Mode: Simple"
                } else {
                    "Mode: Advanced"
                };
                let tooltip = if is_simple {
                    "Simple mode (beginners): essential panels only. Click to switch to Advanced mode."
                } else {
                    "Advanced mode: all tool panels available. Click to switch to Simple mode."
                };
                if ui
                    .button(egui::RichText::new(btn_text).small())
                    .tag("window.menu.ui_mode", btn_text)
                    .on_hover_text(tooltip)
                    .clicked()
                {
                    let next = if is_simple {
                        super::super::prefs::UiMode::Advanced
                    } else {
                        super::super::prefs::UiMode::Simple
                    };
                    self.set_ui_mode(next, ui.ctx());
                }
            });
        });
        let ctx = ui.ctx().clone();
        for c in chosen {
            self.invoke(&c, &ctx);
        }
    }
}
