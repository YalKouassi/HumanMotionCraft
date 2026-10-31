//! The physics report: the switch, what `physics.solve` says the pass does (flights, frames lifted
//! out of the ground, sliding feet held, still poses balanced) and a button to the problem frames,
//! a list to click (`frame.set`) in a window of its own, so they never push the panel about. The
//! same problems are marks on the timeline (`timeline::marks`).

use std::collections::BTreeMap;

use ac_core::clip::Clip;
use egui::Color32;
use serde_json::{Value, json};

use crate::app::App;
use crate::app::shell::Level;
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
    /// The Physics problems window is open.
    problems_open: bool,
    made_for: Option<(Clip, BTreeMap<String, Clip>)>,
    /// The problems, for the timeline.
    pub marks: Vec<ProblemMark>,
}

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
            }
            Err(e) => {
                st.report = None;
                st.marks.clear();
                self.shell
                    .messages
                    .push(Level::Error, format!("The physics pass failed: {e}"));
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
            .on_hover_text("Play and export through the physics pass: feet on the ground, planted feet hold, still poses balance, jumps follow gravity. Off: playback shows your keys, and the numbers below are what the pass would fix")
            .tag("panel.physics.on", "Physics")
            .changed()
        {
            self.run("physics.set", json!({ "on": physics }));
            let f = self.session.document.frame;
            self.run("frame.set", json!({ "frame": f }));
        }
        let Some(r) = self.panels.physics.report.clone() else {
            ui.label(egui::RichText::new("No keys yet: nothing for the pass to check.").weak());
            return;
        };
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
        let count = self.panels.physics.marks.len();
        let label = format!("Problems ({count})");
        if ui
            .add_enabled(count > 0, egui::Button::new(&label))
            .on_hover_text("The problem frames, in a window: a click goes to the frame")
            .on_disabled_hover_text("No problems: feet stay on the ground and still poses balance")
            .tag("panel.physics.problems", label.as_str())
            .clicked()
        {
            self.panels.physics.problems_open = !self.panels.physics.problems_open;
        }
    }

    /// The problem frames, a list to click.
    pub(in crate::app) fn physics_problems_window(&mut self, ctx: &egui::Context) {
        if !self.panels.physics.problems_open {
            return;
        }
        let problems: Vec<Value> = self
            .panels
            .physics
            .report
            .as_ref()
            .and_then(|r| r["problems"].as_array().cloned())
            .unwrap_or_default();
        if problems.is_empty() {
            self.panels.physics.problems_open = false;
            return;
        }
        let pal = self.palette();
        let here = self.session.document.key_frame();
        let mut open = true;
        let mut go = None;
        egui::Window::new("Physics problems")
            .open(&mut open)
            .default_size([360.0, 380.0])
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{} problem{} · click one to go there",
                        problems.len(),
                        if problems.len() == 1 { "" } else { "s" }
                    ))
                    .weak()
                    .small(),
                );
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    for (idx, p) in problems.iter().enumerate() {
                        let from = p["from"].as_i64().unwrap_or(0);
                        let kind = p["kind"].as_str().unwrap_or_default();
                        let label = p["label"].as_str().unwrap_or_default();
                        let text = egui::RichText::new(label).color(colour(pal, kind));
                        if ui
                            .selectable_label(here == from, text)
                            .on_hover_text(format!("Go to frame {from}"))
                            .tag(format!("window.physics_problems.{idx}"), label)
                            .clicked()
                        {
                            go = Some(from);
                        }
                    }
                });
            });
        self.panels.physics.problems_open = open;
        if let Some(from) = go {
            self.playing = false;
            self.run("frame.set", json!({ "frame": from }));
        }
    }
}
