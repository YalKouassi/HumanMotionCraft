//! Scene boxes: the steps and ledges the physics pass stands the feet on. List, add and delete
//! them, and set each one's centre, size, top and turn as numbers (`surface.add/set/delete`; a
//! drag is one undo step). The viewport outlines every box, the selected one highlighted
//! ([`App::scene_overlay`], which also draws the travel path being previewed).

use ac_render::glam::Vec3;
use egui::{Pos2, Stroke};
use serde_json::json;

use super::{gesture, gesture_end};
use crate::app::App;
use crate::app::ui_ids::Tag;

/// The panel's window state.
#[derive(Default)]
pub struct SceneState {
    /// The box picked in the list.
    pub selected: Option<usize>,
    /// The name being typed (box, text).
    name: Option<(usize, String)>,
}

impl App {
    pub(in crate::app) fn scene_panel(&mut self, ui: &mut egui::Ui) {
        let boxes: Vec<(String, f64)> = self
            .session
            .document
            .clip
            .surfaces
            .iter()
            .map(|s| (s.name.clone(), s.top))
            .collect();
        if self.panels.scene.selected.is_some_and(|i| i >= boxes.len()) {
            self.panels.scene.selected = None;
        }
        if boxes.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No boxes: the feet stand on the floor. Add steps and ledges to stand on.",
                )
                .small()
                .weak(),
            );
        }
        for (i, (name, top)) in boxes.iter().enumerate() {
            let on = self.panels.scene.selected == Some(i);
            let text = format!("{name}  ·  top {top:.2} m");
            if ui
                .selectable_label(on, text.clone())
                .tag(format!("panel.scene.box.{i}"), text)
                .clicked()
            {
                self.panels.scene.selected = if on { None } else { Some(i) };
                self.panels.scene.name = None;
            }
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("+ Box")
                .on_hover_text("A box 0.8 m in front of the body (1.2 × 0.6 m, 0.9 m high)")
                .tag("panel.scene.add", "+ Box")
                .clicked()
                && let Some(r) = self.run("surface.add", json!({}))
            {
                self.panels.scene.selected = r["surfaces"].as_array().map(|a| a.len().saturating_sub(1));
            }
            if let Some(i) = self.panels.scene.selected
                && ui
                    .button("Delete box")
                    .tag("panel.scene.delete", "Delete box")
                    .clicked()
                && self.run("surface.delete", json!({ "i": i })).is_some()
            {
                self.panels.scene.selected = None;
            }
        });
        if let Some(i) = self.panels.scene.selected {
            self.box_fields(ui, i);
        }
    }

    /// The selected box's numbers.
    fn box_fields(&mut self, ui: &mut egui::Ui, i: usize) {
        let Some(s) = self.session.document.clip.surfaces.get(i).cloned() else {
            return;
        };
        ui.horizontal_wrapped(|ui| {
            ui.label("Name");
            let text = match &mut self.panels.scene.name {
                Some((j, t)) if *j == i => t,
                slot => &mut slot.insert((i, s.name.clone())).1,
            };
            let r = ui.text_edit_singleline(text).tag("panel.scene.name", "Name");
            let typed = text.trim().to_string();
            if r.lost_focus() && !typed.is_empty() && typed != s.name {
                self.run("surface.set", json!({ "i": i, "name": typed }));
                self.panels.scene.name = None;
            }
        });
        let c = s.centre();
        let label = format!("Box {}", s.name);
        let mut centre = c;
        let mut size = [s.max[0] - s.min[0], s.max[1] - s.min[1]];
        let mut top = s.top;
        let mut turn = s.yaw.to_degrees();
        egui::Grid::new("box-fields").num_columns(3).show(ui, |ui| {
            ui.label("Centre");
            for (k, v) in centre.iter_mut().enumerate() {
                let axis = if k == 0 { "x" } else { "y" };
                let r = ui
                    .add(egui::DragValue::new(v).speed(0.01).suffix(" m").max_decimals(3))
                    .tag(format!("panel.scene.centre.{axis}"), format!("Centre {axis}"));
                if gesture(self, &r, &label) {
                    let to = if k == 0 { [*v, c[1], 0.0] } else { [c[0], *v, 0.0] };
                    self.run("surface.set", json!({ "i": i, "centre": to }));
                }
                gesture_end(self, &r);
            }
            ui.end_row();
            ui.label("Size");
            let was = size;
            for (k, v) in size.iter_mut().enumerate() {
                let r = ui
                    .add(
                        egui::DragValue::new(v)
                            .speed(0.01)
                            .range(0.05..=100.0)
                            .suffix(" m")
                            .max_decimals(3),
                    )
                    .on_hover_text(if k == 0 {
                        "Width (before it turns)"
                    } else {
                        "Depth (before it turns)"
                    })
                    .tag(
                        format!("panel.scene.size.{}", if k == 0 { "width" } else { "depth" }),
                        if k == 0 { "Width" } else { "Depth" },
                    );
                if gesture(self, &r, &label) {
                    let to = if k == 0 {
                        [*v, was[1], 0.0]
                    } else {
                        [was[0], *v, 0.0]
                    };
                    self.run("surface.set", json!({ "i": i, "size": to }));
                }
                gesture_end(self, &r);
            }
            ui.end_row();
            ui.label("Top");
            let r = ui
                .add(
                    egui::DragValue::new(&mut top)
                        .speed(0.01)
                        .range(0.0..=100.0)
                        .suffix(" m")
                        .max_decimals(3),
                )
                .on_hover_text("Height of its top above the floor")
                .tag("panel.scene.top", "Top");
            if gesture(self, &r, &label) {
                self.run("surface.set", json!({ "i": i, "top": top }));
            }
            gesture_end(self, &r);
            ui.end_row();
            ui.label("Turn");
            let r = ui
                .add(
                    egui::DragValue::new(&mut turn)
                        .speed(0.5)
                        .range(-360.0..=360.0)
                        .suffix("°")
                        .max_decimals(1),
                )
                .on_hover_text("Turned about its centre, counter-clockwise seen from above")
                .tag("panel.scene.turn", "Turn");
            if gesture(self, &r, &label) {
                self.run("surface.set", json!({ "i": i, "turn": turn }));
            }
            gesture_end(self, &r);
            ui.end_row();
        });
    }

    /// Draw the scene boxes (the selected one highlighted) and a previewed travel path over the
    /// viewport. `project`: a world point to the screen.
    pub(in crate::app) fn scene_overlay(
        &self,
        painter: &egui::Painter,
        project: &dyn Fn(Vec3) -> Option<Pos2>,
    ) {
        let doc = &self.session.document;
        for (i, s) in doc.clip.surfaces.iter().enumerate() {
            let selected = self.panels.scene.selected == Some(i);
            let (colour, width) = if selected {
                (self.palette().scene_box_selected, 2.0)
            } else {
                (self.palette().scene_box.gamma_multiply(0.7), 1.0)
            };
            let corners = s.corners();
            let at = |q: [f64; 2], z: f64| project(Vec3::new(q[0] as f32, q[1] as f32, z as f32));
            let tops: Vec<Option<Pos2>> = corners.iter().map(|q| at(*q, s.top)).collect();
            let floors: Vec<Option<Pos2>> = corners.iter().map(|q| at(*q, 0.0)).collect();
            let stroke = Stroke::new(width, colour);
            for k in 0..4 {
                let n = (k + 1) % 4;
                for ring in [&tops, &floors] {
                    if let (Some(Some(a)), Some(Some(b))) = (ring.get(k), ring.get(n)) {
                        painter.line_segment([*a, *b], stroke);
                    }
                }
                if let (Some(Some(a)), Some(Some(b))) = (tops.get(k), floors.get(k)) {
                    painter.line_segment([*a, *b], stroke);
                }
            }
            if selected {
                let filled: Vec<Pos2> = tops.iter().flatten().copied().collect();
                if filled.len() == 4 {
                    painter.add(egui::Shape::convex_polygon(
                        filled,
                        super::super::theme::faded(self.palette().scene_box_selected, 40),
                        Stroke::NONE,
                    ));
                }
            }
        }
        if let Some(path) = self.travel_preview_path() {
            let pts: Vec<Pos2> = path
                .iter()
                .filter_map(|p| project(Vec3::new(p[0] as f32, p[1] as f32, 0.0)))
                .collect();
            painter.add(egui::Shape::line(
                pts.clone(),
                Stroke::new(2.0, self.palette().travel),
            ));
            for p in pts.iter().step_by(10) {
                painter.circle_filled(*p, 2.5, self.palette().travel);
            }
        }
    }
}
