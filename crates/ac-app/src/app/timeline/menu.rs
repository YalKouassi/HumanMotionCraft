//! The timeline's right-click menus. On a key: interpolation, hold parts to the next key, copy the
//! pose to a frame, remove parts, keep or clear the AI fill, copy / paste, retime, delete. On the
//! ruler or empty space: add a marker, set the play range, insert or delete frames, paste. On a
//! marker: go there, rename, delete, play to the next marker. Every item runs commands.

use egui::Ui;
use serde_json::{Value, json};

use super::{KeyInfo, Target, glyphs};
use crate::app::App;

impl App {
    /// Say where a menu item is (control op `layout`, under `menu`), so an agent can click it
    /// by name.
    fn at(&mut self, name: &str, r: egui::Response) -> egui::Response {
        let c = r.rect.center();
        self.layout["menu"][name] = json!([c.x, c.y]);
        let label = name.rsplit(':').next().unwrap_or(name).to_string();
        crate::app::ui_ids::Tag::tag(r, format!("timeline.menu.{}", name.replace(':', ".")), label)
    }

    pub(super) fn context_menu(&mut self, ui: &mut Ui) {
        match self.timeline.menu {
            Some(Target::Key { frame, part }) => self.key_menu(ui, frame, part),
            Some(Target::Marker { frame }) => self.marker_menu(ui, frame),
            Some(Target::Ruler { frame } | Target::Empty { frame }) => self.frame_menu(ui, frame),
            None => {
                ui.label("Right-click a key, a marker or the ruler");
            }
        }
    }

    fn key_menu(&mut self, ui: &mut Ui, frame: i64, part: Option<&'static str>) {
        let keys = self.key_infos();
        let Some(k) = keys.iter().find(|k| k.frame == frame) else {
            ui.label(format!("no key on frame {frame} any more"));
            return;
        };
        let n = self.timeline.selected.len().max(1);
        let title = if n > 1 {
            format!("{n} keys selected")
        } else {
            format!("Key {frame} · {}", if k.ai { "AI in-between" } else { "yours" })
        };
        ui.label(egui::RichText::new(title).strong());
        ui.separator();
        let current = k.interp;
        let r = ui.menu_button(format!("Interpolation: {}", glyphs::label(current)), |ui| {
            for (interp, name) in glyphs::ALL {
                let item = format!("interp:{}", interp.name());
                if self.at(&item, ui.radio(current == *interp, *name)).clicked() {
                    self.set_interp(*interp);
                    ui.close();
                }
            }
        });
        self.at("interp", r.response);
        self.hold_menu(ui, &keys, frame);
        ui.menu_button("Copy pose to frame", |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut self.timeline.copy_to)
                        .range(0..=10_000_000)
                        .prefix("frame "),
                );
                if ui.button("Copy").clicked() {
                    let to = self.timeline.copy_to;
                    self.run("key.copy", json!({ "from": frame, "to": to }));
                    ui.close();
                }
            });
        });
        let r = ui.menu_button("Remove parts", |ui| {
            if let Some(p) = part
                && ui.button(format!("{p} (this row)")).clicked()
            {
                self.run("key.remove_parts", json!({ "frame": frame, "parts": [p] }));
                ui.close();
            }
            for row in &k.rows {
                if Some(*row) != part && self.at(&format!("remove:{row}"), ui.button(*row)).clicked() {
                    self.run("key.remove_parts", json!({ "frame": frame, "parts": [row] }));
                    ui.close();
                }
            }
        });
        self.at("remove", r.response);
        self.ai_items(ui, frame);
        ui.separator();
        if self.at("copy", ui.button("Copy keys    Ctrl+C")).clicked() {
            self.copy_keys(ui.ctx());
            ui.close();
        }
        self.paste_items(ui, None);
        let r = ui.menu_button("Retime", |ui| self.retime_items(ui, frame, true));
        self.at("retime", r.response);
        let del = if part.is_some() {
            format!("Remove {} from {n} keys    Del", part.unwrap_or_default())
        } else if n > 1 {
            format!("Delete {n} keys    Del")
        } else {
            "Delete key    Del".into()
        };
        if self.at("delete", ui.button(del)).clicked() {
            self.delete_keys();
            ui.close();
        }
    }

    /// Hold a part still from this key to the next one (`key.hold`).
    fn hold_menu(&mut self, ui: &mut Ui, keys: &[KeyInfo], frame: i64) {
        let next = keys.iter().map(|k| k.frame).find(|&f| f > frame);
        let Some(to) = next else {
            ui.add_enabled(false, egui::Button::new("Hold parts to next key"))
                .on_disabled_hover_text("this is the last key");
            return;
        };
        let custom: Vec<String> = self.session.document.custom_parts.keys().cloned().collect();
        let r = ui.menu_button(format!("Hold parts to next key ({to})"), |ui| {
            let presets = ac_core::parts::PRESETS.iter().map(|p| p.to_string());
            for p in presets.chain(custom) {
                if self.at(&format!("hold:{p}"), ui.button(&p)).clicked() {
                    self.run("key.hold", json!({ "from": frame, "to": to, "parts": [p] }));
                    ui.close();
                }
            }
        });
        self.at("hold", r.response);
    }

    /// Keep or clear the AI fills the key is in or next to.
    fn ai_items(&mut self, ui: &mut Ui, frame: i64) {
        let bands = ac_core::edits::ai_bands(&self.session.document.clip);
        for b in bands.iter().filter(|b| b.from - 1 <= frame && frame <= b.to + 1) {
            let name = if b.model.is_empty() {
                "AI"
            } else {
                b.model.as_str()
            };
            let r = ui
                .button(format!("Keep AI keys {}–{} ({name})", b.from, b.to))
                .on_hover_text("They become your keys");
            if self.at("keep_ai", r).clicked() {
                self.run("ai.keep", json!({ "from": b.from, "to": b.to }));
                ui.close();
            }
            if self
                .at(
                    "clear_ai",
                    ui.button(format!("Clear AI fill {}–{}", b.from, b.to)),
                )
                .clicked()
            {
                self.run("ai.clear", json!({ "from": (b.from - 1).max(0), "to": b.to + 1 }));
                ui.close();
            }
        }
    }

    fn paste_items(&mut self, ui: &mut Ui, at: Option<i64>) {
        let has = self.session.document.key_clipboard.is_some();
        let here = at.map_or("at the playhead".to_string(), |f| format!("on {f}"));
        for (mirror, label) in [(false, "Paste keys"), (true, "Paste keys mirrored")] {
            let keys = if mirror { "Ctrl+Shift+V" } else { "Ctrl+V" };
            let r = ui
                .add_enabled(has, egui::Button::new(format!("{label} {here}    {keys}")))
                .on_disabled_hover_text("copy keys first (Ctrl+C)");
            if self
                .at(if mirror { "paste_mirror" } else { "paste" }, r)
                .clicked()
            {
                if let Some(f) = at {
                    self.playing = false;
                    self.run("frame.set", json!({ "frame": f }));
                }
                self.paste_keys(mirror);
                ui.close();
            }
        }
    }

    fn frame_menu(&mut self, ui: &mut Ui, frame: i64) {
        ui.label(egui::RichText::new(format!("Frame {frame}")).strong());
        ui.separator();
        if self.at("goto", ui.button("Go to this frame")).clicked() {
            self.playing = false;
            self.run("frame.set", json!({ "frame": frame }));
            ui.close();
        }
        if self.at("add_marker", ui.button("Add marker here")).clicked() {
            self.run("marker.add", json!({ "frame": frame }));
            ui.close();
        }
        let len = self.clip_len();
        if self
            .at("range_in", ui.button("Play range starts here    I"))
            .clicked()
        {
            self.timeline.playback.set_point(false, frame, len);
            ui.close();
        }
        if self
            .at("range_out", ui.button("Play range ends here    O"))
            .clicked()
        {
            self.timeline.playback.set_point(true, frame, len);
            ui.close();
        }
        if ui
            .add_enabled(
                self.timeline.playback.range.is_some(),
                egui::Button::new("Clear play range"),
            )
            .clicked()
        {
            self.timeline.playback.range = None;
            ui.close();
        }
        ui.separator();
        self.paste_items(ui, Some(frame));
        let r = ui.menu_button("Retime", |ui| self.retime_items(ui, frame, false));
        self.at("retime", r.response);
        // between two of your keys: walk it
        if let Some((a, b)) = ac_core::edits::span_around(&self.session.document.clip, frame) {
            let r = ui.menu_button(format!("Footsteps {a}–{b}"), |ui| {
                for (left, label, id) in [
                    (false, "Right foot first", "steps:R"),
                    (true, "Left foot first", "steps:L"),
                ] {
                    if self.at(id, ui.button(label)).clicked() {
                        self.footsteps(a, b, left, None);
                        ui.close();
                    }
                }
            });
            self.at("steps", r.response.on_hover_text(
                "Walk between these keys, each foot planted where it lands (as the AI panel's Footsteps row is set: the Walk loop or the model)",
            ));
        }
    }

    fn marker_menu(&mut self, ui: &mut Ui, frame: i64) {
        let markers = self.session.document.clip.markers.clone();
        let name = markers
            .iter()
            .find(|m| m.frame == frame)
            .map_or_else(String::new, |m| m.name.clone());
        ui.label(egui::RichText::new(format!("Marker {name:?} · frame {frame}")).strong());
        ui.separator();
        if self.at("marker_goto", ui.button("Go to marker")).clicked() {
            self.playing = false;
            self.run("frame.set", json!({ "frame": frame }));
            ui.close();
        }
        if self.at("rename", ui.button("Rename…")).clicked() {
            self.timeline.renaming = Some((frame, name));
            ui.close();
        }
        if let Some(next) = markers.iter().map(|m| m.frame).find(|&f| f > frame)
            && ui
                .button(format!("Play from here to the next marker ({next})"))
                .clicked()
        {
            self.timeline.playback.range = Some((frame, next));
            ui.close();
        }
        if self.at("marker_delete", ui.button("Delete marker")).clicked() {
            self.run("marker.delete", json!({ "frame": frame }));
            ui.close();
        }
    }

    /// Insert / delete frames at a frame, and (on keys) scale the selection.
    fn retime_items(&mut self, ui: &mut Ui, frame: i64, on_key: bool) {
        ui.horizontal(|ui| {
            ui.label("frames");
            ui.add(egui::DragValue::new(&mut self.timeline.retime_count).range(1..=100_000));
        });
        let n = self.timeline.retime_count;
        if self
            .at("insert", ui.button(format!("Insert {n} frames before {frame}")))
            .clicked()
        {
            self.retime(json!({ "mode": "insert", "at": frame, "count": n }));
            ui.close();
        }
        if self
            .at(
                "delete_frames",
                ui.button(format!("Delete frames {frame}–{}", frame + n - 1)),
            )
            .clicked()
        {
            self.retime(json!({ "mode": "delete", "at": frame, "count": n }));
            ui.close();
        }
        if on_key {
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("scale ×");
                ui.add(
                    egui::DragValue::new(&mut self.timeline.retime_factor)
                        .range(0.01..=100.0)
                        .speed(0.01),
                );
            });
            if self
                .at("scale", ui.button("Scale the selected keys about the first"))
                .clicked()
            {
                self.scale_selection(None);
                ui.close();
            }
        }
    }

    /// Scale the selected keys (all keys when none is selected) about `pivot` (default: the first).
    pub(super) fn scale_selection(&mut self, pivot: Option<f64>) {
        let frames: Vec<i64> = self.timeline.selected.iter().copied().collect();
        let mut p = json!({ "mode": "scale", "factor": self.timeline.retime_factor });
        if !frames.is_empty() {
            p["frames"] = json!(frames);
        }
        if let Some(x) = pivot {
            p["pivot"] = json!(x.round().max(0.0));
        }
        self.retime(p);
    }

    /// Run a retime and say what it did.
    pub(super) fn retime(&mut self, params: Value) {
        let mode = params["mode"].as_str().unwrap_or_default().to_string();
        if let Some(r) = self.run("keys.retime", params) {
            let notes: Vec<&str> = r["notes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            let removed = r["removed"].as_array().map_or(0, Vec::len);
            self.status = format!(
                "{mode}: {} keys moved, {removed} removed{}",
                r["moved"],
                if notes.is_empty() {
                    String::new()
                } else {
                    format!("; {}", notes.join("; "))
                }
            );
            // the selected frames moved: start afresh
            self.timeline.selected.clear();
            self.timeline.part = None;
        }
    }

    pub(super) fn clip_len(&self) -> i64 {
        let doc = &self.session.document;
        doc.clip.length_with(&doc.links).max(1)
    }
}
