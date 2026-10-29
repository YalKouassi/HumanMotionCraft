//! The undo history (Edit > Undo history): every step, oldest first, the current one marked and
//! the ones that can be redone after it. A click on a step goes back or forward to it: the right
//! number of `edit.undo` or `edit.redo` in one go.

use egui::RichText;
use serde_json::json;

use super::super::App;
use super::super::ui_ids::Tag;

/// The rows the window lists: the start, the undo steps, then the redo steps (next first), and
/// which row is the document as it is now.
pub fn rows(undo: &[&str], redo: &[&str]) -> (Vec<String>, usize) {
    let mut rows = vec!["Start".to_string()];
    rows.extend(undo.iter().map(|s| s.to_string()));
    rows.extend(redo.iter().map(|s| s.to_string()));
    (rows, undo.len())
}

/// How to reach row `to` from row `now`: undo (`true`) or redo, and how many times.
pub fn steps(now: usize, to: usize) -> (bool, usize) {
    if to < now {
        (true, now - to)
    } else {
        (false, to - now)
    }
}

impl App {
    /// Go back (or forward) to undo step `to` (a row of [`rows`]).
    pub(in crate::app) fn jump_history(&mut self, to: usize) {
        let now = self.session.history().len();
        let (undo, n) = steps(now, to);
        if n == 0 {
            return;
        }
        self.playing = false;
        let id = if undo { "edit.undo" } else { "edit.redo" };
        let mut done = 0;
        for _ in 0..n {
            match self.run(id, json!({})) {
                Some(r) if r["changed"].as_bool() == Some(true) => done += 1,
                _ => break,
            }
        }
        self.status = format!(
            "{} {done} step{}",
            if undo { "undid" } else { "redid" },
            if done == 1 { "" } else { "s" }
        );
    }

    pub(in crate::app) fn history_window(&mut self, ctx: &egui::Context) {
        if !self.shell.open.history {
            return;
        }
        let undo = self.session.history();
        let redo = self.session.redo_history();
        let (rows, now) = rows(&undo, &redo);
        let mut open = true;
        let mut go = None;
        egui::Window::new("Undo history")
            .open(&mut open)
            .default_size([300.0, 380.0])
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(format!(
                        "{} steps to undo, {} to redo · click one to go there",
                        now,
                        redo.len()
                    ))
                    .weak()
                    .small(),
                );
                ui.separator();
                let h = ui.text_style_height(&egui::TextStyle::Button) + 4.0;
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .stick_to_bottom(true)
                    .show_rows(ui, h, rows.len(), |ui, range| {
                        for i in range {
                            let Some(label) = rows.get(i) else { continue };
                            let text = if i == now {
                                RichText::new(format!("• {label}")).strong()
                            } else if i > now {
                                RichText::new(format!("   {label}")).weak().italics()
                            } else {
                                RichText::new(format!("   {label}"))
                            };
                            let hint = if i == now {
                                "The document as it is now".to_string()
                            } else {
                                let (undo, n) = steps(now, i);
                                format!(
                                    "{} {n} step{}",
                                    if undo { "Undo" } else { "Redo" },
                                    if n == 1 { "" } else { "s" }
                                )
                            };
                            if ui
                                .selectable_label(i == now, text)
                                .on_hover_text(hint)
                                .tag(format!("window.history.item.{i}"), label.as_str())
                                .clicked()
                            {
                                go = Some(i);
                            }
                        }
                    });
            });
        self.shell.open.history = open;
        if let Some(i) = go {
            self.jump_history(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_mark_now_and_steps_count_the_way() {
        let (r, now) = rows(&["Set key", "Move key 3→5"], &["Delete key"]);
        assert_eq!(r, vec!["Start", "Set key", "Move key 3→5", "Delete key"]);
        assert_eq!(now, 2);
        assert_eq!(steps(now, 0), (true, 2));
        assert_eq!(steps(now, 3), (false, 1));
        assert_eq!(steps(now, 2), (false, 0));
    }
}
