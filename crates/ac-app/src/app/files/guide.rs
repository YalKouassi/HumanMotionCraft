//! What comes after the block-out wizard: play it, review it, save or export it. A small window that
//! offers the next step and never blocks: everything behind it still works, and it closes for good
//! with one tick (`.app-state.json`). It runs no command of its own beyond what its buttons name.

use super::super::App;
use super::super::ui_ids::Tag;

const STEPS: usize = 3;

pub struct Guide {
    step: usize,
    /// How the gaps were filled, as the wizard's `fill`: `ai`, `plain` or `none`.
    fill: String,
}

impl App {
    /// The wizard finished: offer the next step, unless asked not to.
    pub(super) fn open_guide(&mut self, fill: &str) {
        if !self.files.state.guide_off {
            self.files.guide = Some(Guide {
                step: 0,
                fill: fill.to_string(),
            });
        }
    }

    pub(super) fn guide_window(&mut self, ctx: &egui::Context) {
        let Some(mut g) = self.files.guide.take() else {
            return;
        };
        let mut open = true;
        let mut off = false;
        egui::Window::new(format!("Next: step {} of {STEPS}", g.step + 1))
            .id(egui::Id::unique("guide"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::LEFT_BOTTOM, [320.0, -190.0])
            .default_width(300.0)
            .show(ctx, |ui| {
                match g.step {
                    0 => {
                        ui.label(match g.fill.as_str() {
                            "ai" => "Your poses are keyed and the AI filled the gaps. Watch it.",
                            "plain" => "Your poses are keyed and plain interpolation filled the gaps. Watch it.",
                            _ => "Your poses are keyed, with nothing filled between them. Watch it.",
                        });
                        let label = if self.playing { "Pause" } else { "Play" };
                        if ui.button(label).tag("dialog.guide.play", "Play").clicked() {
                            self.toggle_play();
                        }
                    }
                    1 => {
                        ui.label("Check it for sliding feet, the ground, balance, joint range and pops. The answer is in the Review panel on the right.");
                        if ui
                            .button("Review animation")
                            .tag("dialog.guide.review", "Review animation")
                            .clicked()
                        {
                            self.playing = false;
                            self.review_run();
                        }
                    }
                    _ => {
                        ui.label("Keep it in your library, or export it as a .glb for a game engine.");
                        ui.horizontal(|ui| {
                            if ui.button("Save").tag("dialog.guide.save", "Save").clicked() {
                                self.save();
                            }
                            if ui.button("Export…").tag("dialog.guide.export", "Export…").clicked() {
                                self.playing = false;
                                self.open_export();
                            }
                        });
                    }
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if g.step > 0 && ui.button("Back").tag("dialog.guide.back", "Back").clicked() {
                        g.step -= 1;
                    }
                    if g.step + 1 < STEPS && ui.button("Next").tag("dialog.guide.next", "Next").clicked() {
                        g.step += 1;
                    }
                    if ui
                        .small_button("Don't show this again")
                        .tag("dialog.guide.off", "Don't show this again")
                        .clicked()
                    {
                        off = true;
                    }
                });
            });
        if off {
            self.files.state.guide_off = true;
            self.save_state();
        }
        if open && !off {
            self.files.guide = Some(g);
        }
    }
}
