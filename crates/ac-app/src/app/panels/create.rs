//! Create motion: one section asking what the person wants to do (fill between their poses,
//! describe it, steer it live), showing only that job's controls, and the AI models window, which
//! holds the setup each job's model needs: installs, licences, loading and the sample clips.

use crate::app::App;
use crate::app::shell::DockPanel;
use crate::app::ui_ids::Tag;

/// What the person wants to do.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Fill,
    Describe,
    Steer,
}

/// The section's window state.
#[derive(Default)]
pub struct CreateState {
    pub mode: Mode,
    /// The AI models window is open.
    pub models_open: bool,
}

/// One line of plain help under a heading.
pub(in crate::app) fn help(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).small().weak());
}

impl App {
    pub(in crate::app) fn create_panel(&mut self, ui: &mut egui::Ui) {
        ui.label("What do you want to do?");
        ui.horizontal_wrapped(|ui| {
            let st = &mut self.panels.create;
            for (mode, id, label, tip) in [
                (
                    Mode::Fill,
                    "fill",
                    "Fill between my poses",
                    "Creates the movement between two poses you've set",
                ),
                (
                    Mode::Describe,
                    "describe",
                    "Describe it",
                    "Type what the character should do",
                ),
                (
                    Mode::Steer,
                    "steer",
                    "Steer it live",
                    "Act it out, then keep the best part",
                ),
            ] {
                ui.selectable_value(&mut st.mode, mode, label)
                    .on_hover_text(tip)
                    .tag(format!("panel.create.mode.{id}"), label);
            }
        });
        ui.separator();
        match self.panels.create.mode {
            Mode::Fill => self.ai_panel(ui),
            Mode::Describe => self.docked_or(ui, DockPanel::Text, |app, ui| app.text_panel(ui)),
            Mode::Steer => self.docked_or(ui, DockPanel::PoseFill, |app, ui| app.steer_panel(ui)),
        }
    }

    /// A job opened as its own panel is drawn there, once, with a way back.
    fn docked_or(
        &mut self,
        ui: &mut egui::Ui,
        panel: DockPanel,
        body: impl FnOnce(&mut Self, &mut egui::Ui),
    ) {
        if !self.is_dock_panel_open(panel) {
            body(self, ui);
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("Open in its own panel.");
            if ui
                .small_button("Return")
                .on_hover_text("Close the standalone panel and show this here")
                .tag(format!("panel.create.{}.return", panel.id_str()), "Return")
                .clicked()
            {
                self.set_dock_panel(panel, false);
            }
        });
    }

    /// Steer it live: the setup line until its model is installed, then the take.
    pub(in crate::app) fn steer_panel(&mut self, ui: &mut egui::Ui) {
        if self.posefill_installed() {
            self.live_panel(ui);
            self.posefill_busy_row(ui);
        } else {
            help(
                ui,
                "Act it out, then keep the best part. This uses NVIDIA AI, which needs an NVIDIA graphics card and a one-time download.",
            );
            self.ai_models_button(ui, "panel.create.steer.setup", "Set up NVIDIA AI…");
        }
    }

    /// Opens the AI models window.
    pub(in crate::app) fn ai_models_button(&mut self, ui: &mut egui::Ui, id: &str, label: &str) {
        if ui.button(label).tag(id, label).clicked() {
            self.panels.create.models_open = true;
        }
    }

    /// Everything about the models themselves: which one, its install and licences, loading it.
    pub(in crate::app) fn ai_models_window(&mut self, ctx: &egui::Context) {
        if !self.panels.create.models_open {
            return;
        }
        let mut open = true;
        egui::Window::new("AI models")
            .open(&mut open)
            .default_size([520.0, 520.0])
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    ui.label(egui::RichText::new("HumanMotion AI").strong());
                    help(ui, "Comes with the app and fills between your poses.");
                    match self.session.document.ai.as_ref().map(|m| m.info().clone()) {
                        Some(info) => {
                            ui.label(egui::RichText::new(info.tag.clone()).small().weak());
                            self.model_row(ui, &info);
                        }
                        None => {
                            ui.label("No model in assets/models: filling uses plain interpolation.");
                        }
                    }
                    ui.separator();
                    ui.label(egui::RichText::new("NVIDIA AI").strong());
                    help(
                        ui,
                        "Fills between your poses and powers Steer it live. Needs an NVIDIA graphics card.",
                    );
                    self.posefill_setup(ui);
                    ui.separator();
                    ui.label(egui::RichText::new("Describe it (NVIDIA)").strong());
                    help(ui, "Turns what you type into motion.");
                    self.text_setup(ui);
                });
            });
        self.panels.create.models_open = open;
    }
}
