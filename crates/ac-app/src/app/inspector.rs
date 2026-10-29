//! The right panel: a summary, then one collapsible section per panel (`panels/`): the selected
//! joint, layers, physics report, scene boxes, the AI in-between, motion from text, and pins.

use serde_json::json;

use super::App;
use crate::app::ui_ids::Tag;

impl App {
    pub(super) fn inspector(&mut self, ui: &mut egui::Ui) {
        // what runs whether a section is open or not: a finished AI preview, the physics report
        // the timeline marks come from (Shift+A is `ai.toggle`'s shortcut, in the keymap)
        self.ai_poll();
        self.text_poll(ui.ctx());
        self.physics_refresh();
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
        section(ui, "Joint", true, |ui| self.joint_panel(ui));
        section(ui, &format!("Layers ({layers})"), true, |ui| {
            self.layers_panel(ui)
        });
        let physics = if problems > 0 {
            format!("Physics ({problems} problems)")
        } else {
            "Physics".into()
        };
        section(ui, &physics, true, |ui| self.physics_panel(ui));
        section(ui, &format!("Scene boxes ({boxes})"), boxes > 0, |ui| {
            self.scene_panel(ui)
        });
        section(ui, "AI in-between", true, |ui| self.ai_panel(ui));
        section(ui, "Motion from text", true, |ui| self.text_panel(ui));
        section(ui, "Pins", false, |ui| self.pins_panel(ui));
    }

    fn pins_panel(&mut self, ui: &mut egui::Ui) {
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

/// A collapsible section (its id is its first word, so a count in the title doesn't reset it).
fn section(ui: &mut egui::Ui, title: &str, open: bool, body: impl FnOnce(&mut egui::Ui)) {
    let id = title.split(' ').next().unwrap_or(title);
    egui::CollapsingHeader::new(egui::RichText::new(title).strong())
        .id_salt(id)
        .default_open(open)
        .show(ui, body)
        .header_response
        .tag(format!("panel.{}", id.to_lowercase()), title);
}
