//! The right panel: a summary, then one collapsible section per panel (`panels/`): the selected
//! joint, layers, physics report, the review, scene boxes, the AI in-between, motion from text, and pins.
//! Any section (as well as `Range` and `Picker` from the viewport toolbar) can also be opened as
//! its own dockable or pop-out native-window panel via `craft_ui::docking` (`shell/layout.rs`).

use serde_json::json;

use super::App;
use super::shell::DockPanel;
use crate::app::ui_ids::Tag;

impl App {
    pub(super) fn inspector(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| self.inspector_body(ui));
    }

    fn inspector_body(&mut self, ui: &mut egui::Ui) {
        ui.heading("Inspector");
        let doc = &self.session.document;
        ui.label(format!(
            "Frame {:.1} of {}",
            doc.frame,
            doc.clip.length_with(&doc.links)
        ));
        ui.label(format!(
            "{} keys, {} layers",
            doc.clip.keys().len(),
            doc.clip.layers.len()
        ));
        let problems = self.panels.physics.marks.len();
        let boxes = self.session.document.clip.surfaces.len();
        let layers = self.session.document.clip.layers.len();
        self.inspector_section(ui, DockPanel::Joint, "Joint", true, |app, ui| app.joint_panel(ui));
        self.inspector_section(
            ui,
            DockPanel::Layers,
            &format!("Layers ({layers})"),
            true,
            |app, ui| app.layers_panel(ui),
        );
        let physics = if problems > 0 {
            format!(
                "Physics ({problems} problem{})",
                if problems == 1 { "" } else { "s" }
            )
        } else {
            "Physics".into()
        };
        self.inspector_section(ui, DockPanel::Physics, &physics, true, |app, ui| {
            app.physics_panel(ui)
        });
        let findings = self.panels.review.count();
        let review = if findings > 0 {
            format!("Review ({findings})")
        } else {
            "Review".into()
        };
        self.inspector_section(ui, DockPanel::Review, &review, true, |app, ui| {
            app.review_panel(ui)
        });
        self.inspector_section(
            ui,
            DockPanel::Scene,
            &format!("Scene boxes ({boxes})"),
            boxes > 0,
            |app, ui| app.scene_panel(ui),
        );
        self.inspector_section(ui, DockPanel::Ai, "AI in-between", true, |app, ui| {
            app.ai_panel(ui)
        });
        self.inspector_section(ui, DockPanel::Text, "Motion from text", true, |app, ui| {
            app.text_panel(ui)
        });
        self.inspector_section(ui, DockPanel::PoseFill, "Pose fill", true, |app, ui| {
            app.posefill_panel(ui)
        });
        self.inspector_section(ui, DockPanel::Pins, "Pins", false, |app, ui| app.pins_panel(ui));
    }

    /// A collapsible section in the Inspector; when `panel` is open in the docking workspace (as
    /// its own docked tab or floating second-monitor window), shows a button to return it here
    /// instead of drawing it twice.
    fn inspector_section(
        &mut self,
        ui: &mut egui::Ui,
        panel: DockPanel,
        title: &str,
        open: bool,
        body: impl FnOnce(&mut Self, &mut egui::Ui),
    ) {
        let id = title.split(' ').next().unwrap_or(title);
        let tag_id = format!("panel.{}", id.to_lowercase());
        if self.is_dock_panel_open(panel) {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(title).strong()).tag(&tag_id, title);
                if ui
                    .small_button("Return")
                    .on_hover_text("Close the standalone panel and return this section to the Inspector")
                    .tag(format!("{tag_id}.return"), "Return")
                    .clicked()
                {
                    self.set_dock_panel(panel, false);
                }
            });
            return;
        }
        egui::CollapsingHeader::new(egui::RichText::new(title).strong())
            .id_salt(id)
            .default_open(open)
            .show(ui, |ui| body(self, ui))
            .header_response
            .tag(tag_id, title);
    }

    pub(in crate::app) fn pins_panel(&mut self, ui: &mut egui::Ui) {
        let pins: Vec<String> = self.session.document.pins.keys().cloned().collect();
        if pins.is_empty() {
            ui.label(
                egui::RichText::new("None: right-click a joint to pin it.")
                    .small()
                    .weak(),
            );
        }
        for p in &pins {
            ui.horizontal(|ui| {
                ui.label(p.as_str());
                if ui
                    .small_button("x")
                    .on_hover_text("Unpin")
                    .tag(format!("panel.pins.unpin.{p}"), "x")
                    .clicked()
                {
                    self.run("pin.set", json!({ "bone": p, "on": false }));
                }
            });
        }
        if !pins.is_empty()
            && ui
                .button("Clear pins")
                .tag("panel.pins.clear", "Clear pins")
                .clicked()
        {
            self.run("pins.clear", json!({}));
        }
    }
}
