//! The first-run welcome (after the built-in library is installed) and a short, dismissible tips
//! tour. Both are remembered as seen (`.app-state.json`); File → Welcome… shows the welcome again.

use serde_json::json;

use super::super::App;
use super::super::library::Tab;
use super::super::save::Switch;
use super::super::ui_ids::Tag;

/// A built-in walk to open from the welcome.
const WALK: &str = "Locomotion/Walk";

/// (title, text)
const TIPS: &[(&str, &str)] = &[
    (
        "Pin a joint",
        "Right-click a joint in the viewport to pin it. Dragging other joints then keeps it where it is: pin the feet and drag the hips to crouch.",
    ),
    (
        "The IK tool",
        "Pick IK in the viewport's toolbar, then drag a hand or foot: it goes where you drag it and the limb bends to reach.",
    ),
    (
        "Preview fill",
        "Key two poses a few frames apart, then press Preview fill in the panel on the right: the AI's in-between plays before you keep it (Accept) or drop it.",
    ),
];

impl App {
    pub(super) fn welcome_dialog(&mut self, ctx: &egui::Context) {
        if !self.files.welcome {
            return;
        }
        let mut choice = None;
        let mut tips = !self.files.state.tips_seen;
        let modal = egui::Modal::new(egui::Id::unique("welcome")).show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.heading(format!("Welcome to {}", ac_core::product::NAME));
            ui.label("The built-in library is installed: animations to watch and learn from, and poses to build your own with.");
            ui.add_space(8.0);
            let big = |ui: &mut egui::Ui, id: &str, title: &str, sub: &str| {
                ui.add_sized(
                    [ui.available_width(), 44.0],
                    egui::Button::new(egui::RichText::new(format!("{title}\n{sub}")).size(13.0)),
                )
                .tag(format!("dialog.welcome.{id}"), title)
                .clicked()
            };
            if big(ui, "walk", "Open a built-in walk", "Play it, scrub it, see how its keys are made") {
                choice = Some("walk");
            }
            if big(ui, "block", "Block out from poses", "Pick poses, place them on frames, let the AI fill between") {
                choice = Some("block");
            }
            if big(ui, "blank", "Start blank", "A new animation of your own") {
                choice = Some("blank");
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.checkbox(&mut tips, "Show me three quick tips").tag("dialog.welcome.tips", "Show me three quick tips");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Close").tag("dialog.welcome.close", "Close").clicked() {
                        choice = Some("close");
                    }
                });
            });
        });
        if choice.is_none() && modal.should_close() {
            choice = Some("close");
        }
        let Some(choice) = choice else { return };
        self.files.welcome = false;
        self.files.state.welcome_seen = true;
        match choice {
            "walk" => {
                self.lib_tab = Tab::Builtin;
                self.switch(Switch::Open(WALK.into()));
            }
            "block" => self.open_block(),
            "blank" => {
                self.lib_tab = Tab::Animations;
                self.switch(Switch::New(
                    json!({ "name": "Untitled", "group": "My animations" }),
                ));
            }
            _ => {}
        }
        if tips {
            self.files.tips = Some(0);
        } else {
            self.files.state.tips_seen = true;
        }
        self.save_state();
    }

    pub(super) fn tips_window(&mut self, ctx: &egui::Context) {
        let Some(page) = self.files.tips else { return };
        let Some((title, text)) = TIPS.get(page) else {
            self.files.tips = None;
            return;
        };
        let mut next = Some(page);
        let mut open = true;
        egui::Window::new(format!("Tip {} of {}: {title}", page + 1, TIPS.len()))
            .id(egui::Id::unique("tips"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::RIGHT_BOTTOM, [-290.0, -200.0])
            .default_width(300.0)
            .show(ctx, |ui| {
                ui.label(*text);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if page > 0 && ui.button("Back").tag("dialog.tips.back", "Back").clicked() {
                        next = Some(page - 1);
                    }
                    if page + 1 < TIPS.len() {
                        if ui.button("Next").tag("dialog.tips.next", "Next").clicked() {
                            next = Some(page + 1);
                        }
                    } else if ui.button("Done").tag("dialog.tips.done", "Done").clicked() {
                        next = None;
                    }
                    if ui
                        .small_button("Skip tips")
                        .tag("dialog.tips.skip", "Skip tips")
                        .clicked()
                    {
                        next = None;
                    }
                });
            });
        if !open {
            next = None;
        }
        self.files.tips = next;
        if next.is_none() {
            self.files.state.tips_seen = true;
            self.save_state();
        }
    }
}
