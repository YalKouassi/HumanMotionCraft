//! The block-out wizard: library poses placed on frames, filled between by the AI or plain
//! interpolation, with a style and the physics pass: one `anim.block` (one undo step), in the open
//! animation or a new one (the new animation and the block-out are then one undo step together).

use serde_json::{Value, json};

use super::super::App;
use super::super::library::Tab;
use super::super::ui_ids::Tag;

/// Rows the wizard starts with, when the library has them: (pose, frame).
const START: &[(&str, i64)] = &[
    ("Standing/Relaxed", 0),
    ("Standing/Mid-step", 15),
    ("Standing/Relaxed", 30),
];
/// Most poses in one block-out.
const MAX_ROWS: usize = 64;
const FILLS: &[(&str, &str, &str)] = &[
    ("ai", "AI", "the learned in-betweener fills between the poses"),
    ("plain", "Plain", "plain interpolation between the poses"),
    ("none", "None", "only the keys, nothing filled"),
];

pub struct Block {
    rows: Vec<(String, i64)>,
    fill: String,
    /// "" keeps the current AI setting.
    style: String,
    /// Some(on): set the physics pass; None: leave it.
    physics: Option<bool>,
    new_clip: bool,
    name: String,
}

impl App {
    pub(in crate::app) fn open_block(&mut self) {
        let poses = self.pose_ids();
        let mut rows: Vec<(String, i64)> = START
            .iter()
            .filter(|(p, _)| poses.iter().any(|x| x == p))
            .map(|(p, f)| (p.to_string(), *f))
            .collect();
        if rows.len() < 2 {
            rows = poses
                .iter()
                .take(2)
                .enumerate()
                .map(|(i, p)| (p.clone(), i as i64 * 20))
                .collect();
        }
        let doc = &self.session.document;
        let has_work = doc.clip.layers.iter().any(|l| !l.keys.is_empty()) && doc.clip.keys().len() > 1;
        self.files.block = Some(Block {
            rows,
            fill: "ai".into(),
            style: String::new(),
            physics: None,
            new_clip: has_work,
            name: "Blocked out".into(),
        });
    }

    fn pose_ids(&self) -> Vec<String> {
        self.library["poses"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| p["id"].as_str().map(str::to_string))
            .collect()
    }

    pub(super) fn block_window(&mut self, ctx: &egui::Context) {
        let Some(mut b) = self.files.block.take() else {
            return;
        };
        let poses = self.pose_ids();
        let styles: Vec<String> = self
            .session
            .run("ai.info", &json!({}))
            .ok()
            .and_then(|v| v["model"]["styles"].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|s| s.as_str().map(str::to_string))
            .collect();
        let has_ai = self.session.document.ai.is_some();
        let mut open = true;
        let mut go = false;
        egui::Window::new("Block out from poses")
            .pivot(egui::Align2::CENTER_TOP)
            .default_pos(super::dialog_pos(ctx))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(400.0)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("Each pose is keyed on its frame; the gaps are filled.").weak());
                let mut remove = None;
                egui::Grid::new("block-rows").num_columns(3).show(ui, |ui| {
                    ui.label(egui::RichText::new("Pose").weak());
                    ui.label(egui::RichText::new("Frame").weak());
                    ui.end_row();
                    for (i, (pose, frame)) in b.rows.iter_mut().enumerate() {
                        egui::ComboBox::from_id_salt(("block-pose", i))
                            .width(230.0)
                            .selected_text(pose.clone())
                            .show_ui(ui, |ui| {
                                for p in &poses {
                                    ui.selectable_value(pose, p.clone(), p.as_str())
                                        .tag(format!("dialog.block.pose.{i}.{p}"), p.as_str());
                                }
                            })
                            .response
                            .tag(format!("dialog.block.pose.{i}"), format!("Pose {}", i + 1));
                        ui.add(egui::DragValue::new(frame).range(0..=100_000).speed(0.5))
                            .tag(format!("dialog.block.frame.{i}"), format!("Frame {}", i + 1));
                        if ui.small_button("x").on_hover_text("Remove this pose").tag(format!("dialog.block.remove.{i}"), "x").clicked() {
                            remove = Some(i);
                        }
                        ui.end_row();
                    }
                });
                if let Some(i) = remove.filter(|_| b.rows.len() > 1) {
                    b.rows.remove(i);
                }
                if b.rows.len() < MAX_ROWS && ui.button("+ Pose").tag("dialog.block.add_pose", "+ Pose").clicked() {
                    let (p, f) = b.rows.last().cloned().unwrap_or_else(|| (poses.first().cloned().unwrap_or_default(), -15));
                    b.rows.push((p, f + 15));
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Fill");
                    for (key, label, tip) in FILLS {
                        ui.selectable_value(&mut b.fill, (*key).to_string(), *label)
                            .on_hover_text(*tip)
                            .tag(format!("dialog.block.fill.{key}"), *label);
                    }
                });
                if b.fill == "ai" && !has_ai {
                    ui.label(
                        egui::RichText::new("No AI model is loaded: AI fill will be refused. Pick Plain, or install the model.")
                            .color(self.palette().warning),
                    );
                }
                ui.add_enabled_ui(b.fill == "ai", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Style");
                        let shown = if b.style.is_empty() { "current setting".to_string() } else { ac_core::ai::style_name(&b.style) };
                        egui::ComboBox::from_id_salt("block-style").selected_text(shown).height(320.0).show_ui(ui, |ui| {
                            ui.selectable_value(&mut b.style, String::new(), "current setting")
                                .tag("dialog.block.style.current", "current setting");
                            for s in &styles {
                                ui.selectable_value(&mut b.style, s.clone(), ac_core::ai::style_name(s))
                                    .tag(format!("dialog.block.style.{s}"), s.as_str());
                            }
                        })
                        .response
                        .tag("dialog.block.style", "Style");
                    });
                });
                ui.horizontal(|ui| {
                    ui.label("Physics");
                    ui.selectable_value(&mut b.physics, None, "as it is").tag("dialog.block.physics.as_is", "as it is");
                    ui.selectable_value(&mut b.physics, Some(true), "on").tag("dialog.block.physics.on", "on");
                    ui.selectable_value(&mut b.physics, Some(false), "off").tag("dialog.block.physics.off", "off");
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.checkbox(&mut b.new_clip, "In a new animation").tag("dialog.block.new_clip", "In a new animation");
                    if b.new_clip {
                        ui.add(egui::TextEdit::singleline(&mut b.name).desired_width(160.0)).tag("dialog.block.name", "Name");
                    }
                });
                let rising = b.rows.windows(2).all(|w| matches!(w, [a, b] if a.1 < b.1));
                if !rising {
                    ui.label(egui::RichText::new("Frames must rise from one pose to the next.").weak());
                }
                if ui
                    .add_enabled(rising && !b.rows.is_empty(), egui::Button::new("Block out"))
                    .on_hover_text("One undo step")
                    .tag("dialog.block.block_out", "Block out")
                    .clicked()
                {
                    go = true;
                }
            });
        if go && self.run_block(&b) {
            open = false;
        }
        if open {
            self.files.block = Some(b);
        }
    }

    /// Run the block-out; whether it worked (a refusal is in the status bar).
    fn run_block(&mut self, b: &Block) -> bool {
        let mut params = json!({
            "poses": b.rows.iter().map(|r| r.0.clone()).collect::<Vec<_>>(),
            "frames": b.rows.iter().map(|r| r.1).collect::<Vec<_>>(),
            "fill": b.fill,
        });
        if b.fill == "ai" && !b.style.is_empty() {
            params["style"] = json!(b.style);
        }
        if let Some(on) = b.physics {
            params["physics"] = json!(on);
        }
        if b.new_clip {
            if self.session.document.unsaved() && !self.save() {
                return false;
            }
            // the new animation and the block-out undo together
            let _ = self.run("edit.begin", json!({ "label": "Block out" }));
            let name = if b.name.trim().is_empty() {
                "Blocked out"
            } else {
                b.name.trim()
            };
            let made = self.run("clip.new", json!({ "name": name, "group": "My animations" }));
            let blocked = made.is_some() && self.block_step(params).is_some();
            let _ = self.run("edit.end", json!({}));
            if !blocked && let Some(id) = made.as_ref().and_then(|m| m["id"].as_str()) {
                // the block-out was refused: the new, empty animation goes too
                let id = id.to_string();
                self.run("edit.undo", json!({}));
                self.run("clip.delete", json!({ "id": id }));
            }
            self.lib_tab = Tab::Animations;
            self.refresh_library();
            blocked
        } else {
            let ok = self.block_step(params).is_some();
            if ok {
                self.refresh_library();
            }
            ok
        }
    }

    fn block_step(&mut self, params: Value) -> Option<Value> {
        let r = self.run("anim.block", params)?;
        let fills = r["fills"].as_array().map_or(0, Vec::len);
        self.status = format!(
            "blocked out {} poses over {} frames, {fills} gaps filled (Ctrl+Z undoes it all)",
            r["keys"], r["frames"]
        );
        Some(r)
    }
}
