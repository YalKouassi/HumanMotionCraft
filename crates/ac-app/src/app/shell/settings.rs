//! The Settings window (File > Settings…, shown as Edit > Preferences on Windows) and About.
//! Every change is kept at once in the preferences; the library folder takes effect at the next
//! launch, and says so.

use egui::RichText;

use super::super::App;
use super::super::prefs::{Colours, Theme, Units};
use super::super::ui_ids::Tag;

impl App {
    pub(in crate::app) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.shell.open.settings {
            return;
        }
        let mut p = self.shell.prefs.get().general.clone();
        let mut open = true;
        let (mut clear_thumbs, mut shortcuts, mut reset_layout) = (false, false, false);
        let library_now = self.session.document.library.clone();
        egui::Window::new("Settings")
            .open(&mut open)
            .collapsible(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                egui::Grid::new("settings-grid").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    ui.label("New animations");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut p.default_fps).range(1.0..=240.0).speed(0.2).fixed_decimals(0))
                            .tag("window.settings.fps", "frames per second");
                        ui.label("frames per second");
                    });
                    ui.end_row();

                    ui.label("Autosave");
                    ui.horizontal(|ui| {
                        let mut on = p.autosave_secs > 0;
                        if ui.checkbox(&mut on, "every").tag("window.settings.autosave", "Autosave").changed() {
                            p.autosave_secs = if on { 120 } else { 0 };
                        }
                        ui.add_enabled(
                            on,
                            egui::DragValue::new(&mut p.autosave_secs).range(10..=3600).speed(1.0),
                        )
                        .tag("window.settings.autosave_secs", "seconds");
                        ui.label("seconds");
                    });
                    ui.end_row();

                    ui.label("UI scale");
                    ui.horizontal(|ui| {
                        ui.add(egui::Slider::new(&mut p.ui_scale, 0.5..=3.0).step_by(0.05).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)))
                            .tag("window.settings.ui_scale", "UI scale");
                        ui.label(RichText::new("Ctrl + / Ctrl − / Ctrl 0").weak().small());
                    });
                    ui.end_row();

                    ui.label("Theme");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut p.theme, Theme::Dark, "Dark").tag("window.settings.theme.dark", "Dark");
                        ui.selectable_value(&mut p.theme, Theme::Light, "Light").tag("window.settings.theme.light", "Light");
                    });
                    ui.end_row();

                    ui.label("Overlay colours");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut p.colours, Colours::Standard, "Standard")
                            .tag("window.settings.colours.standard", "Standard");
                        ui.selectable_value(&mut p.colours, Colours::Safe, "Colour-safe").on_hover_text(
                            "Balance, ghosts and the AI preview in colours told apart with any colour vision (no red against green)",
                        )
                        .tag("window.settings.colours.safe", "Colour-safe");
                    });
                    ui.end_row();

                    ui.label("Show times as");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut p.units, Units::Frames, "Frames").tag("window.settings.units.frames", "Frames");
                        ui.selectable_value(&mut p.units, Units::Seconds, "Seconds").tag("window.settings.units.seconds", "Seconds");
                        ui.selectable_value(&mut p.units, Units::Timecode, "Timecode").tag("window.settings.units.timecode", "Timecode");
                    });
                    ui.end_row();

                    ui.label("Library folder");
                    ui.vertical(|ui| {
                        let shown = p
                            .library_dir
                            .as_ref()
                            .map_or("the app's own folder".to_string(), |d| d.display().to_string());
                        ui.label(RichText::new(shown).monospace());
                        ui.horizontal(|ui| {
                            if ui.button("Choose…").tag("window.settings.library_choose", "Choose…").clicked()
                                && let Some(dir) = rfd::FileDialog::new().pick_folder()
                            {
                                p.library_dir = Some(dir);
                            }
                            if ui
                                .add_enabled(p.library_dir.is_some(), egui::Button::new("Use the app's folder"))
                                .tag("window.settings.library_default", "Use the app's folder")
                                .clicked()
                            {
                                p.library_dir = None;
                            }
                        });
                        if let Some(now) = &library_now
                            && p.library_dir.as_ref().is_some_and(|d| d != now)
                        {
                            ui.label(RichText::new("Used from the next launch.").weak().small());
                        }
                    });
                    ui.end_row();
                });
                ui.separator();
                ui.horizontal(|ui| {
                    clear_thumbs = ui
                        .button("Clear thumbnail cache")
                        .on_hover_text("The library's thumbnails are drawn again")
                        .tag("window.settings.clear_thumbs", "Clear thumbnail cache")
                        .clicked();
                    shortcuts = ui
                        .button("Keyboard shortcuts…")
                        .tag("window.settings.shortcuts", "Keyboard shortcuts…")
                        .clicked();
                    reset_layout = ui
                        .button("Reset layout")
                        .tag("window.settings.reset_layout", "Reset layout")
                        .clicked();
                });
                if let Some(path) = self.shell.prefs.path() {
                    ui.label(RichText::new(format!("Saved in {}", path.display())).weak().small());
                } else {
                    ui.label(RichText::new("This session's settings are not saved (--prefs memory, or a scripted run).").weak().small());
                }
            });
        if !p.default_fps.is_finite() {
            p.default_fps = 30.0;
        }
        self.shell.prefs.update(|s| s.general = p);
        self.shell.open.settings = open;
        if clear_thumbs {
            self.act("app.clear_thumbs", ctx);
        }
        if shortcuts {
            self.open_shortcuts();
        }
        if reset_layout {
            self.reset_layout(ctx);
        }
    }

    pub(in crate::app) fn about_window(&mut self, ctx: &egui::Context) {
        if !self.shell.open.about {
            return;
        }
        let mut open = true;
        egui::Window::new(format!("About {}", ac_core::product::NAME))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.heading(ac_core::product::NAME);
                ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                ui.label("Keyframe animation with physics and learned in-betweening.");
                ui.separator();
                ui.label(RichText::new(format!("Graphics: {}", self.shell.perf.adapter)).small());
                let model = self
                    .session
                    .document
                    .ai
                    .as_ref()
                    .map_or("no AI model loaded".to_string(), |m| {
                        format!("AI model: {}", m.info().tag)
                    });
                ui.label(RichText::new(model).small());
                ui.label(
                    RichText::new("Credits for the assets and models: ATTRIBUTION.md and NOTICE.")
                        .weak()
                        .small(),
                );
            });
        self.shell.open.about = open;
    }
}
