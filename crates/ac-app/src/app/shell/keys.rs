//! The keymap: every registered command's `shortcut` and every app action's keys in one
//! craft-keymap table, the user's own bindings on top (saved in the preferences), dispatched each
//! frame; and Help > Keyboard shortcuts, craft-keymap's dialog, which lists them all and records
//! new ones.
//!
//! A spec's shortcut text says `Ctrl` for the command key (`Ctrl+S`): it is read as craft-keymap's
//! `Cmd`, which is Ctrl off macOS and ⌘ on it. Plain keys (K, arrows, Space, Delete) wait while a
//! text field has the keyboard; chords with Ctrl still run. A key bound in a context (the
//! viewport, the timeline, the library: `scoped.rs`) runs there ahead of its application-wide
//! binding, so every key press is handled here, once.

use craft_keymap::egui::Dispatcher;
use craft_keymap::ui::{Area, DialogState, English, FixedRow, Options, Outcome, ShortcutsDialog};
use craft_keymap::{Active, Bindable, Binding, Keymap, Overrides, Platform, Profiles, Scope};

use super::super::prefs::ShortcutSets;
use super::super::{App, ui_ids};
use super::actions::{self, ACTIONS, Show};
use super::{Level, menus, scoped};

/// More default keys for commands than their spec's one shortcut.
const EXTRA_DEFAULTS: &[(&str, &str)] = &[("edit.redo", "Cmd+Shift+Z")];

pub struct Keys {
    pub keymap: Keymap,
    /// The user's named sets (the dialog's Set menu: new, rename, import, export).
    pub sets: Profiles,
    dispatcher: Dispatcher,
    /// Help > Keyboard shortcuts, while open.
    pub dialog: Option<DialogState>,
}

/// A spec's shortcut text in craft-keymap's portable form (`Ctrl` is the command key).
pub fn portable(text: &str) -> String {
    text.split('+')
        .map(|part| {
            if part.eq_ignore_ascii_case("ctrl") {
                "Cmd"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// Every bindable thing: the registry's commands (but the gesture brackets), then the app actions.
/// Shortcut texts that don't parse are left unbound and listed.
pub fn catalog(session: &ac_core::Session) -> (Vec<Bindable>, Vec<String>) {
    let mut out = Vec::new();
    let mut notes = Vec::new();
    let mut bind = |b: Bindable, text: &str, what: &str| match Binding::parse(&portable(text)) {
        Ok(k) => b.with_default(k),
        Err(e) => {
            notes.push(format!(
                "shortcut {text:?} of {what} doesn't parse ({e}); it is left unbound"
            ));
            b
        }
    };
    for spec in session.commands() {
        if actions::internal(spec.id) {
            continue;
        }
        let menu = menus::menu_of(spec.id);
        let mut b = Bindable::new(spec.id, spec.label, Scope::app())
            .with_path(vec![menu.to_string(), spec.label.to_string()]);
        if let Some(s) = spec.shortcut {
            b = bind(b, s, spec.id);
        }
        for (id, s) in EXTRA_DEFAULTS {
            if *id == spec.id {
                b = bind(b, s, spec.id);
            }
        }
        out.push(b);
    }
    for a in ACTIONS {
        let path = [Some(a.menu), a.sub, Some(a.label)]
            .into_iter()
            .flatten()
            .map(str::to_string)
            .collect();
        let mut b = Bindable::new(a.id, a.label, Scope::app()).with_path(path);
        if a.show == Show::Check {
            b = b.toggle();
        }
        for k in a.keys {
            b = bind(b, k, a.id);
        }
        out.push(b);
    }
    (out, notes)
}

impl Keys {
    pub fn new(session: &ac_core::Session, overrides: Overrides, mut sets: Profiles) -> (Keys, Vec<String>) {
        let (catalog, mut notes) = catalog(session);
        let keymap = Keymap::new(catalog, scoped::scopes(), Platform::current()).with_overrides(overrides);
        for p in keymap.problems() {
            notes.push(format!("saved shortcuts: {p:?} (left as it was)"));
        }
        for p in sets.problems() {
            notes.push(format!("saved shortcut sets: {p:?} (left out)"));
        }
        sets.reconcile(&builtins());
        let keys = Keys {
            keymap,
            sets,
            // held arrows step on; actions that mustn't repeat are filtered (see dispatch_keys)
            dispatcher: Dispatcher::new().with_repeat(true).with_clipboard_events(true),
            dialog: None,
        };
        (keys, notes)
    }
}

/// The app's own set: the default keys (a user's edits make a set of theirs).
pub fn builtins() -> Vec<(&'static str, Overrides)> {
    vec![("Default", Overrides::new())]
}

/// Whether holding a key runs this again.
fn repeats(id: &str) -> bool {
    actions::action(id).is_some_and(|a| a.repeat) || matches!(id, "edit.undo" | "edit.redo")
}

fn menu_area(b: &Bindable, menu: &str) -> bool {
    b.paths.first().and_then(|p| p.first()).is_some_and(|m| m == menu)
}

/// The Show entries of the shortcuts dialog: one per menu.
fn areas() -> Vec<Area> {
    vec![
        Area {
            id: "file",
            label: "File",
            rows: |b| menu_area(b, "File"),
        },
        Area {
            id: "edit",
            label: "Edit",
            rows: |b| menu_area(b, "Edit"),
        },
        Area {
            id: "key",
            label: "Key",
            rows: |b| menu_area(b, "Key"),
        },
        Area {
            id: "pose",
            label: "Pose",
            rows: |b| menu_area(b, "Pose"),
        },
        Area {
            id: "layer",
            label: "Layer",
            rows: |b| menu_area(b, "Layer"),
        },
        Area {
            id: "physics",
            label: "Physics",
            rows: |b| menu_area(b, "Physics"),
        },
        Area {
            id: "ai",
            label: "AI",
            rows: |b| menu_area(b, "AI"),
        },
        Area {
            id: "view",
            label: "View",
            rows: |b| menu_area(b, "View"),
        },
        Area {
            id: "help",
            label: "Help",
            rows: |b| menu_area(b, "Help"),
        },
    ]
}

/// Mouse gestures, listed beside the keys (they aren't rebindable), and when each context of a
/// context key is on.
fn fixed() -> Vec<FixedRow> {
    let row = |group: &str, label: &str, keys: &str| FixedRow {
        group: group.into(),
        label: label.into(),
        keys: keys.into(),
        chord: None,
    };
    let contexts = scoped::CONTEXTS
        .iter()
        .map(|(name, when)| row("Where a key works: Key (Context)", &format!("{name}: {when}"), ""));
    contexts
        .chain([
            row(
                "Viewport",
                "Pose: drag a joint (with the Drag or IK tool)",
                "Drag",
            ),
            row("Viewport", "Pin or unpin a joint", "Right-click"),
            row(
                "Viewport",
                "Slide the body on this frame and its key (Move tool)",
                "Drag",
            ),
            row(
                "Viewport",
                "Turn the body on this frame (Move tool)",
                "Shift+Drag",
            ),
            row(
                "Viewport",
                "Slide or turn the whole animation (Move tool)",
                "Alt+Drag, Alt+Shift+Drag",
            ),
            row("Timeline", "Scrub; drag keys; box-select", "Drag"),
            row("Timeline", "Zoom about the pointer", "Wheel"),
            row("Timeline", "Pan", "Shift+Wheel or middle-drag"),
            row("Timeline", "Snapping off while dragging", "Alt"),
        ])
        .collect()
}

impl App {
    /// This frame's key presses: shortcuts and app actions. Nothing runs while the Save question,
    /// or the shortcuts dialog, is up.
    pub(in crate::app) fn dispatch_keys(&mut self, ctx: &egui::Context) {
        if self.confirm.is_some() || self.shell.keys.dialog.is_some() {
            return;
        }
        // only a text field keeps the keys: a focused button or the focused timeline doesn't
        let typing = ctx.text_edit_focused();
        let repeat_only = ctx.input(|i| {
            let (mut held, mut fresh) = (false, false);
            for e in &i.events {
                if let egui::Event::Key {
                    pressed: true,
                    repeat,
                    ..
                } = e
                {
                    if *repeat {
                        held = true;
                    } else {
                        fresh = true;
                    }
                }
            }
            held && !fresh
        });
        let contexts = self.contexts();
        let active = Active {
            panel: None,
            contexts: &contexts,
            typing,
        };
        let keys = &mut self.shell.keys;
        let hits = ctx.input_mut(|i| {
            keys.dispatcher.dispatch_filtered(i, &keys.keymap, &active, |h| {
                !repeat_only || repeats(&h.command.id)
            })
        });
        for h in hits {
            self.invoke(&h.command, ctx);
        }
    }

    /// The shortcut a command or action has now (the user's own included), as shown on this
    /// platform; empty when it has none.
    pub(in crate::app) fn shortcut_text(&self, id: &str) -> String {
        let km = &self.shell.keys.keymap;
        km.bindings(&id.into())
            .iter()
            .find(|b| !b.trigger.double)
            .map(|b| b.trigger.display(km.platform()))
            .unwrap_or_default()
    }

    pub(in crate::app) fn open_shortcuts(&mut self) {
        if self.shell.keys.dialog.is_none() {
            self.shell.keys.dialog = Some(DialogState::new());
        }
    }

    /// Whether the shortcuts dialog is recording a chord (the menus stand still meanwhile).
    pub(in crate::app) fn recording_shortcut(&self) -> bool {
        self.shell
            .keys
            .dialog
            .as_ref()
            .is_some_and(DialogState::is_recording)
    }

    /// Help > Keyboard shortcuts: every command and action with its keys; edits are saved as
    /// they're made (and undone by Cancel).
    pub(in crate::app) fn shortcuts_window(&mut self, ctx: &egui::Context) {
        let Keys {
            keymap, sets, dialog, ..
        } = &mut self.shell.keys;
        let Some(state) = dialog.as_mut() else {
            return;
        };
        let options = Options::default().with(|o| {
            o.title = "Keyboard shortcuts";
            o.areas = areas();
            o.fixed = fixed();
        });
        let builtins = builtins();
        let out = ShortcutsDialog::new(keymap, &English)
            .sets(sets, &builtins)
            .options(&options)
            .show(ctx, state);
        let open = state.is_open();
        match out {
            Some(Outcome::Ok | Outcome::Changed | Outcome::Cancel) => self.keep_shortcuts(),
            Some(Outcome::Summary { text, .. }) => {
                ctx.copy_text(text);
                self.status = "copied the shortcut list to the clipboard".into();
            }
            Some(Outcome::Export { name, json }) => self.export_shortcuts(&name, &json),
            Some(Outcome::Import { .. }) => self.import_shortcuts(&options),
            Some(Outcome::ExportAs { format, .. }) => {
                // no other formats are offered (Options::export_formats is empty)
                self.shell.messages.push(
                    Level::Warning,
                    format!("Keyboard shortcuts: can't export as {format}, only as JSON"),
                );
            }
            _ => {}
        }
        if !open {
            self.shell.keys.dialog = None;
        }
    }
}

/// Biggest shortcuts file read (a set is a few KB; craft-keymap refuses past its own limit too).
const MAX_SHORTCUTS_FILE: u64 = 1 << 20;

impl App {
    /// The dialog's Export: the set as craft-keymap JSON, to a file the user picks.
    fn export_shortcuts(&mut self, name: &str, json: &str) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Export keyboard shortcuts")
            .add_filter("Keyboard shortcuts (JSON)", &["json"])
            .set_file_name(format!("{} shortcuts.json", ui_ids::part(name)))
            .save_file()
        else {
            return;
        };
        match std::fs::write(&path, json) {
            Ok(()) => self.status = format!("exported the shortcuts to {}", path.display()),
            Err(e) => self.shell.messages.push(
                Level::Error,
                format!("Keyboard shortcuts: couldn't write {}: {e}", path.display()),
            ),
        }
    }

    /// The dialog's Import: a craft-keymap JSON set from a file the user picks replaces the
    /// shortcuts (Cancel in the dialog takes it back); a bad file is refused in the dialog.
    fn import_shortcuts(&mut self, options: &Options) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Import keyboard shortcuts")
            .add_filter("Keyboard shortcuts (JSON)", &["json"])
            .pick_file()
        else {
            return;
        };
        let text = match std::fs::metadata(&path) {
            Ok(m) if m.len() > MAX_SHORTCUTS_FILE => Err(format!(
                "it is {} KB, too big for a shortcuts file",
                m.len() / 1024
            )),
            _ => std::fs::read_to_string(&path).map_err(|e| e.to_string()),
        };
        let text = match text {
            Ok(t) => t,
            Err(e) => {
                self.shell.messages.push(
                    Level::Error,
                    format!("Keyboard shortcuts: couldn't read {}: {e}", path.display()),
                );
                return;
            }
        };
        let Keys {
            keymap, sets, dialog, ..
        } = &mut self.shell.keys;
        let Some(state) = dialog.as_mut() else {
            return;
        };
        let builtins = builtins();
        let imported = ShortcutsDialog::new(keymap, &English)
            .sets(sets, &builtins)
            .options(options)
            .import_json(state, &text);
        if let Ok(name) = imported {
            self.keep_shortcuts();
            self.status = format!("imported the shortcut set {name:?} from {}", path.display());
        }
    }

    /// The keys in use and the sets go into the prefs.
    fn keep_shortcuts(&mut self) {
        let overrides = self.shell.keys.keymap.overrides().clone();
        let sets = self.shell.keys.sets.clone();
        self.shell.prefs.update(|p| {
            p.shortcuts = overrides;
            p.shortcut_sets = ShortcutSets(sets);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_in_a_spec_is_the_command_key() {
        assert_eq!(portable("Ctrl+S"), "Cmd+S");
        assert_eq!(portable("Shift+K"), "Shift+K");
        assert_eq!(portable("ctrl+shift+z"), "Cmd+shift+z");
        for a in ACTIONS {
            for k in a.keys {
                assert!(Binding::parse(&portable(k)).is_ok(), "{} {k}", a.id);
            }
        }
    }
}
