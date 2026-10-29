//! The physics report: the switch, what `physics.solve` says the pass does (flights, frames lifted
//! out of the ground, sliding feet held, still poses balanced) and the problem frames as a list
//! to click (`frame.set`). The same problems are marks on the timeline (`timeline::marks`).

use std::collections::BTreeMap;

use ac_core::clip::Clip;
use egui::Color32;
use serde_json::{Value, json};

use crate::app::App;
use crate::app::timeline::marks::Mark;
use crate::app::ui_ids::Tag;

/// A problem frame for the timeline: where it starts, its kind ("penetration", "sliding",
/// "unbalanced") and how it reads.
#[derive(Clone, Debug, PartialEq)]
pub struct ProblemMark {
    pub frame: i64,
    /// The run's last frame.
    pub to: i64,
    pub kind: String,
    pub label: String,
}

/// The panel's window state: the last report and the clip it was made for.
#[derive(Default)]
pub struct PhysicsState {
    report: Option<Value>,
    error: Option<String>,
    made_for: Option<(Clip, BTreeMap<String, Clip>)>,
    /// The problems, for the timeline.
    pub marks: Vec<ProblemMark>,
}

/// Most problem rows listed (the rest are counted).
const MAX_ROWS: usize = 40;

/// A problem's colour (the theme's: penetration, sliding, off balance, anything else).
pub fn colour(pal: &crate::app::theme::Palette, kind: &str) -> Color32 {
    let [penetration, sliding, unbalanced, other] = pal.problems;
    match kind {
        "penetration" => penetration,
        "sliding" => sliding,
        "unbalanced" => unbalanced,
        _ => other,
    }
}

impl App {
    /// Work the report out again when the animation changed (not in the middle of a drag).
    pub(in crate::app) fn physics_refresh(&mut self) {
        let doc = &self.session.document;
        let fresh = self
            .panels
            .physics
            .made_for
            .as_ref()
            .is_some_and(|(c, l)| *c == doc.clip && *l == doc.links);
        if fresh || self.session.in_gesture() {
            return;
        }
        let made_for = (doc.clip.clone(), doc.links.clone());
        let st = &mut self.panels.physics;
        st.made_for = Some(made_for);
        if self.session.document.clip.base().keys.is_empty() {
            st.report = None;
            st.error = None;
            st.marks.clear();
            return;
        }
        match self.session.run("physics.solve", &json!({})) {
            Ok(r) => {
                st.marks = r["problems"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|p| ProblemMark {
                        frame: p["from"].as_i64().unwrap_or(0),
                        to: p["to"].as_i64().or(p["from"].as_i64()).unwrap_or(0),
                        kind: p["kind"].as_str().unwrap_or_default().to_string(),
                        label: p["label"].as_str().unwrap_or_default().to_string(),
                    })
                    .collect();
                st.report = Some(r);
                st.error = None;
            }
            Err(e) => {
                st.report = None;
                st.marks.clear();
                st.error = Some(format!("the physics pass failed: {e}"));
            }
        }
        let pal = self.palette();
        let marks = self
            .panels
            .physics
            .marks
            .iter()
            .map(|m| Mark::new(m.frame as f64, &m.kind, colour(pal, &m.kind), &m.label).until(m.to as f64))
            .collect();
        self.timeline.marks.set("physics", marks);
    }

    pub(in crate::app) fn physics_panel(&mut self, ui: &mut egui::Ui) {
        let mut physics = self.session.document.clip.physics;
        if ui
            .checkbox(&mut physics, "Physics")
            .on_hover_text("Play and export through the physics pass: feet on the ground, planted feet hold, still poses balance, jumps follow gravity")
            .tag("panel.physics.on", "Physics")
            .changed()
        {
            self.run("physics.set", json!({ "on": physics }));
            let f = self.session.document.frame;
            self.run("frame.set", json!({ "frame": f }));
        }
        if let Some(e) = &self.panels.physics.error {
            ui.label(egui::RichText::new(e).color(colour(self.palette(), "penetration")));
            return;
        }
        let Some(r) = self.panels.physics.report.clone() else {
            ui.label(egui::RichText::new("No keys yet: nothing for the pass to check.").weak());
            return;
        };
        if !physics {
            ui.label(
                egui::RichText::new("Off: playback shows your keys. This is what the pass would fix.")
                    .small()
                    .weak(),
            );
        }
        let n = |k: &str| r[k].as_u64().unwrap_or(0);
        let count = |k: &str| r[k].as_array().map_or(0, Vec::len);
        egui::Grid::new("physics-report").num_columns(2).show(ui, |ui| {
            for (label, v, tip) in [
                (
                    "Jumps",
                    count("flights"),
                    "Runs of frames in the air that follow gravity",
                ),
                (
                    "Lifted",
                    n("lifted") as usize,
                    "Frames with a foot keyed into the ground, lifted onto it",
                ),
                (
                    "Feet held",
                    n("slid") as usize,
                    "Frames where a planted foot slid and was held in place",
                ),
                (
                    "Balanced",
                    count("balanced"),
                    "Still frames off balance: the hips moved over the feet",
                ),
            ] {
                ui.label(label).on_hover_text(tip);
                ui.label(format!("{v}"));
                ui.end_row();
            }
        });
        let problems: Vec<Value> = r["problems"].as_array().cloned().unwrap_or_default();
        if problems.is_empty() {
            ui.label(
                egui::RichText::new("No problems: feet stay on the ground and still poses balance.")
                    .small()
                    .weak(),
            );
            return;
        }
        ui.label(
            egui::RichText::new("Problem frames (click to go there)")
                .small()
                .weak(),
        );
        for (idx, p) in problems.iter().take(MAX_ROWS).enumerate() {
            let from = p["from"].as_i64().unwrap_or(0);
            let kind = p["kind"].as_str().unwrap_or_default();
            let label = p["label"].as_str().unwrap_or_default();
            let text = egui::RichText::new(label).color(colour(self.palette(), kind));
            let here = self.session.document.key_frame() == from;
            if ui
                .selectable_label(here, text)
                .on_hover_text(format!("Go to frame {from}"))
                .tag(format!("panel.physics.problem.{idx}"), label)
                .clicked()
            {
                self.playing = false;
                self.run("frame.set", json!({ "frame": from }));
            }
        }
        if problems.len() > MAX_ROWS {
            ui.label(egui::RichText::new(format!("and {} more", problems.len() - MAX_ROWS)).small());
        }
    }
}
