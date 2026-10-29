//! The command palette (Ctrl+Shift+P): fuzzy search over every registered command and app action
//! (label, id, input docs), each with its shortcut. Enter runs the chosen one (a command with
//! required inputs opens its form); Ctrl+Enter, or the chosen row's "Options…", opens the form first to set
//! its options; Esc closes.

use egui::{Key, Modifiers, RichText};

use super::super::App;
use super::super::ui_ids::Tag;
use super::actions::{self, ACTIONS};
use super::fuzzy;

/// Most rows shown.
const MAX_ROWS: usize = 60;

pub struct PaletteState {
    pub query: String,
    selected: usize,
    /// Give the search field the keyboard on the first frame.
    focus: bool,
}

/// One searchable thing.
#[derive(Clone, Debug)]
pub struct Entry {
    pub id: &'static str,
    pub label: &'static str,
    /// Input docs (commands) or the action's note.
    pub doc: String,
    pub menu: &'static str,
    /// It has inputs: a form can set them.
    pub has_inputs: bool,
    /// It has required inputs: running it opens the form.
    pub needs_inputs: bool,
}

/// Every command (but the gesture brackets) and app action.
pub fn entries(session: &ac_core::Session) -> Vec<Entry> {
    let mut out: Vec<Entry> = session
        .commands()
        .iter()
        .filter(|s| !actions::internal(s.id))
        .map(|s| Entry {
            id: s.id,
            label: s.label,
            doc: s
                .inputs
                .iter()
                .map(|p| format!("{}: {}", p.name, p.doc))
                .collect::<Vec<_>>()
                .join("; "),
            menu: super::menus::menu_of(s.id),
            has_inputs: !s.inputs.is_empty(),
            needs_inputs: s.inputs.iter().any(|p| p.required),
        })
        .collect();
    out.extend(ACTIONS.iter().map(|a| Entry {
        id: a.id,
        label: a.label,
        doc: a.doc.to_string(),
        menu: a.menu,
        has_inputs: false,
        needs_inputs: false,
    }));
    out
}

/// The entries matching `query`, best first (all of them, in menu order, for an empty query).
pub fn search(all: &[Entry], query: &str) -> Vec<Entry> {
    if query.trim().is_empty() {
        let mut v = all.to_vec();
        v.sort_by_key(|e| {
            super::menus::MENUS
                .iter()
                .position(|m| *m == e.menu)
                .unwrap_or(usize::MAX)
        });
        return v;
    }
    let mut scored: Vec<(i32, &Entry)> = all
        .iter()
        .filter_map(|e| {
            fuzzy::score_entry(query, &[(e.label, 100), (e.id, 90), (e.doc.as_str(), 40)]).map(|s| (s, e))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.label.cmp(b.1.label)));
    scored.into_iter().map(|(_, e)| e.clone()).collect()
}

impl App {
    pub(in crate::app) fn open_palette(&mut self, query: &str) {
        self.shell.palette = Some(PaletteState {
            query: query.to_string(),
            selected: 0,
            focus: true,
        });
    }

    pub(in crate::app) fn palette_window(&mut self, ctx: &egui::Context) {
        let Some(state) = self.shell.palette.as_mut() else {
            return;
        };
        // the list's own keys, taken before the search field sees them
        let (down, up, enter, form, close) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::COMMAND, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        let found = search(&entries(&self.session), &state.query);
        let shown = found.len().min(MAX_ROWS);
        if down {
            state.selected = (state.selected + 1).min(shown.saturating_sub(1));
        }
        if up {
            state.selected = state.selected.saturating_sub(1);
        }
        let mut chosen: Option<(Entry, bool)> = None;
        let shortcuts: Vec<String> = found
            .iter()
            .take(MAX_ROWS)
            .map(|e| self.shortcut_text(e.id))
            .collect();
        let Some(state) = self.shell.palette.as_mut() else {
            return;
        };
        let mut open = !close;
        let shown_at = egui::Window::new("Command palette")
            .id(egui::Id::unique("command-palette"))
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 48.0])
            .fixed_size([600.0, 420.0])
            .show(ctx, |ui| {
                let r = ui
                    .add(
                        egui::TextEdit::singleline(&mut state.query)
                            .hint_text("Type a command…")
                            .desired_width(f32::INFINITY),
                    )
                    .tag("window.palette.search", "Type a command…");
                if state.focus {
                    r.request_focus();
                    state.focus = false;
                }
                if r.changed() {
                    state.selected = 0;
                }
                ui.label(
                    RichText::new(format!(
                        "{} found · Enter runs · Ctrl+Enter sets its options first · Esc closes",
                        found.len()
                    ))
                    .weak()
                    .small(),
                );
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    for (i, e) in found.iter().take(MAX_ROWS).enumerate() {
                        let selected = i == state.selected;
                        ui.horizontal(|ui| {
                            let label = if e.needs_inputs {
                                format!("{}…", e.label)
                            } else {
                                e.label.to_string()
                            };
                            let row = ui
                                .selectable_label(selected, RichText::new(label.clone()).strong())
                                .tag(format!("window.palette.item.{}", e.id), label);
                            if selected && (down || up) {
                                row.scroll_to_me(None);
                            }
                            ui.label(RichText::new(format!("{} · {}", e.menu, e.id)).weak().small());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                // only on the chosen row, so the list stays quiet
                                if e.has_inputs
                                    && selected
                                    && ui
                                        .small_button("Options…")
                                        .on_hover_text("Set its options first")
                                        .tag(format!("window.palette.options.{}", e.id), "Options…")
                                        .clicked()
                                {
                                    chosen = Some((e.clone(), true));
                                }
                                if let Some(s) = shortcuts.get(i).filter(|s| !s.is_empty()) {
                                    ui.label(RichText::new(s).monospace());
                                }
                            });
                            let row = if e.doc.is_empty() {
                                row
                            } else {
                                row.on_hover_text(&e.doc)
                            };
                            if row.clicked() {
                                chosen = Some((e.clone(), false));
                            }
                        });
                    }
                });
            })
            .map(|r| r.response.rect);
        if (enter || form)
            && chosen.is_none()
            && let Some(e) = found.get(state.selected)
        {
            chosen = Some((e.clone(), form));
        }
        if let Some((e, with_form)) = chosen {
            self.shell.palette = None;
            match self.session.spec(e.id) {
                Some(spec) if with_form && e.has_inputs => self.open_form(spec, &serde_json::json!({})),
                _ => self.invoke(&e.id.into(), ctx),
            }
        } else {
            // a click elsewhere closes it
            let outside = ctx.input(|i| {
                i.pointer.any_click()
                    && i.pointer
                        .interact_pos()
                        .is_some_and(|p| shown_at.is_some_and(|r| !r.contains(p)))
            });
            open &= !outside;
        }
        if !open {
            self.shell.palette = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(id: &'static str, label: &'static str) -> Entry {
        Entry {
            id,
            label,
            doc: String::new(),
            menu: "Edit",
            has_inputs: false,
            needs_inputs: false,
        }
    }

    #[test]
    fn the_best_match_comes_first() {
        let all = vec![
            e("key.set", "Set key"),
            e("edit.undo", "Undo"),
            e("key.move", "Move key"),
        ];
        let found = search(&all, "undo");
        assert_eq!(found.first().map(|e| e.id), Some("edit.undo"));
        let found = search(&all, "key mo");
        assert_eq!(found.first().map(|e| e.id), Some("key.move"));
        assert_eq!(search(&all, "").len(), 3);
        assert!(search(&all, "zzz").is_empty());
    }
}
