//! The timeline's toolbar: save, transport (play, loop, speed, real time or every frame), key and
//! delete, the frame readout, zoom, the view toggles (dope sheet, contacts, curves), the time
//! format, the key-shape legend and the undo readout; and the retime toolbar under it.

use serde_json::json;

use super::{TimeFormat, glyphs, playback::SPEEDS};
use crate::app::App;
use crate::app::ui_ids::Tag;

/// Every `--ui` / control `ui` name the window takes (the app's own, then the timeline's).
pub const UI_NAMES: &str = "one of tab, search, dope, overlay, ghost, tool, camera, zoom, contacts, curves, \
     retime, time, speed, every_frame, range, marks, joint, fit";

impl App {
    /// The timeline's window state by name (`--ui`, control op `ui`): `contacts`, `curves`,
    /// `retime`, `every_frame` = on/off; `time` = frames/seconds/timecode; `speed` = 0.25..2;
    /// `range` = `a-b` or `off`; `marks` = `frame[-to]:text;...` (source `ui`) or `off`;
    /// `joint` = a bone name (selects it: the curve editor shows it); `fit` (any value): show the
    /// whole animation.
    pub(in crate::app) fn set_timeline_ui(&mut self, name: &str, value: &str) -> Result<(), String> {
        let on = || match value {
            "on" | "true" | "1" => Ok(true),
            "off" | "false" | "0" => Ok(false),
            _ => Err(format!("{value:?}: on or off")),
        };
        let tl = &mut self.timeline;
        match name {
            "contacts" => tl.contacts = on()?,
            "curves" => tl.curves = on()?,
            "retime" => tl.retime = on()?,
            "every_frame" => tl.playback.every_frame = on()?,
            "time" => {
                // the same setting as Settings > Units
                let t = TimeFormat::parse(value).ok_or("frames, seconds or timecode")?;
                tl.time = t;
                self.shell.prefs.update(|p| p.general.units = t.units());
            }
            "speed" => {
                let s: f64 = value.parse().map_err(|e| format!("{e}"))?;
                if !SPEEDS.contains(&s) {
                    return Err(format!("one of {SPEEDS:?}"));
                }
                tl.playback.speed = s;
            }
            "range" if value == "off" => tl.playback.range = None,
            "range" => {
                let (a, b) = value.split_once('-').ok_or("a-b (frames) or off")?;
                let (a, b): (i64, i64) = (
                    a.trim().parse().map_err(|e| format!("{e}"))?,
                    b.trim().parse().map_err(|e| format!("{e}"))?,
                );
                if a < 0 || b < a {
                    return Err("0 <= a <= b".into());
                }
                tl.playback.range = Some((a, b));
            }
            "fit" => tl.zoom = 0.0,
            "joint" => {
                self.selected = Some(self.session.document.bone(value)?);
            }
            "marks" if value == "off" => tl.marks.clear("ui"),
            "marks" => {
                tl.marks.clear("ui");
                for item in value.split(';').filter(|s| !s.trim().is_empty()) {
                    let (at, text) = item.split_once(':').ok_or("frame[-to]:text;...")?;
                    let (a, b) = at.split_once('-').map_or((at, None), |(a, b)| (a, Some(b)));
                    let a: f64 = a.trim().parse().map_err(|e| format!("{at:?}: {e}"))?;
                    let mut m = super::marks::Mark::new(
                        a,
                        "note",
                        egui::Color32::from_rgb(235, 120, 70),
                        text.trim(),
                    );
                    if let Some(b) = b {
                        m = m.until(b.trim().parse().map_err(|e| format!("{at:?}: {e}"))?);
                    }
                    tl.marks.push("ui", m);
                }
            }
            _ => {
                return Err(format!(
                    "one of {}, dialog, box, layer, travel, {UI_NAMES}, {}",
                    super::super::view_state::UI_NAMES,
                    Self::SHELL_UI
                ));
            }
        }
        Ok(())
    }

    /// Draw the toolbar; returns whether Fit was clicked.
    pub(super) fn timeline_bar(&mut self, ui: &mut egui::Ui, length: f64) -> bool {
        let pal = self.palette();
        let mut fit = false;
        ui.horizontal_wrapped(|ui| {
            self.save_button(ui);
            ui.separator();
            self.transport(ui);
            ui.separator();
            if ui
                .button("Key")
                .on_hover_text("Key the pose shown (K)")
                .tag("timeline.key", "Key")
                .clicked()
            {
                self.run("key.set", json!({}));
            }
            let n = self.timeline.selected.len();
            let del = match self.timeline.part {
                Some(p) if n > 0 => format!("Remove {p}"),
                _ if n > 1 => format!("Delete {n} keys"),
                _ => "Delete key".into(),
            };
            if ui
                .button(del.clone())
                .on_hover_text("Delete the selected keys, or the key on the playhead (Del)")
                .tag("timeline.delete", del)
                .clicked()
            {
                self.delete_keys();
            }
            self.frame_readout(ui, length);
            ui.separator();
            if ui
                .small_button("-")
                .on_hover_text("Zoom out (wheel)")
                .tag("timeline.zoom_out", "-")
                .clicked()
            {
                self.timeline.zoom = (self.timeline.zoom / 1.5).max(0.5);
            }
            if ui
                .small_button("+")
                .on_hover_text("Zoom in (wheel)")
                .tag("timeline.zoom_in", "+")
                .clicked()
            {
                self.timeline.zoom = (self.timeline.zoom * 1.5).min(80.0);
            }
            fit = ui
                .small_button("Fit")
                .on_hover_text("Show the whole animation")
                .tag("timeline.fit", "Fit")
                .clicked();
            let hint = self.palette().hint;
            let tl = &mut self.timeline;
            let dope = ui
                .toggle_value(&mut tl.dope, "Dope sheet")
                .tag("timeline.dope", "Dope sheet")
                .on_hover_text("A row per body part: drag, select or delete one part's keys");
            glow(ui, &dope.rect, hint, &mut tl.dope_hint, tl.dope);
            ui.toggle_value(&mut tl.contacts, "Contacts")
                .tag("timeline.contacts", "Contacts")
                .on_hover_text("Where each foot is planted: click a span to pin or free it, drag to plant");
            ui.toggle_value(&mut tl.curves, "Curves")
                .tag("timeline.curves", "Curves")
                .on_hover_text("The selected joint's rotation (and the hips' position) as curves");
            ui.toggle_value(&mut tl.retime, "Retime")
                .tag("timeline.retime", "Retime")
                .on_hover_text("Insert or delete frames, scale keys");
            let before = tl.time;
            egui::ComboBox::from_id_salt("time-format")
                .selected_text(tl.time.name())
                .width(78.0)
                .show_ui(ui, |ui| {
                    for t in TimeFormat::ALL {
                        ui.selectable_value(&mut tl.time, t, t.name())
                            .tag(format!("timeline.time.{}", t.name()), t.name());
                    }
                })
                .response
                .tag("timeline.time", tl.time.name())
                .on_hover_text("Count time in frames, seconds or timecode (also Settings > Units)");
            let after = tl.time;
            if after != before {
                self.shell.prefs.update(|p| p.general.units = after.units());
            }
            ui.label(egui::RichText::new("Key shapes").weak().small())
                .tag("timeline.legend", "Key shapes")
                .on_hover_ui(|ui| glyphs::legend(ui, pal));
            ui.separator();
            self.undo_readout(ui);
        });
        if self.timeline.retime {
            self.retime_bar(ui);
        }
        fit
    }

    fn transport(&mut self, ui: &mut egui::Ui) {
        if ui
            .button(if self.playing { "Pause" } else { "Play" })
            .on_hover_text("Space")
            .tag("timeline.play", if self.playing { "Pause" } else { "Play" })
            .clicked()
        {
            self.toggle_play();
        }
        let mut looping = self.session.document.clip.loop_;
        if ui
            .toggle_value(&mut looping, "Loop")
            .tag("timeline.loop", "Loop")
            .on_hover_text("Loop the animation: it plays round, and its last key leads into its first")
            .changed()
        {
            self.run("clip.loop", json!({ "on": looping }));
        }
        let pb = &mut self.timeline.playback;
        egui::ComboBox::from_id_salt("play-speed")
            .selected_text(format!("{}×", pb.speed))
            .width(54.0)
            .show_ui(ui, |ui| {
                for s in SPEEDS {
                    ui.selectable_value(&mut pb.speed, *s, format!("{s}×"))
                        .tag(format!("timeline.speed.{s}"), format!("{s}×"));
                }
            })
            .response
            .tag("timeline.speed", format!("{}×", pb.speed))
            .on_hover_text("Playback speed");
        ui.toggle_value(&mut pb.every_frame, "Every frame")
            .tag("timeline.every_frame", "Every frame")
            .on_hover_text(
            "On: every frame is shown (slower when drawing can't keep up). Off: real time, frames may be skipped",
        );
        if let Some((a, b)) = pb.range {
            let r = ui
                .small_button(format!("{a}–{b} ×"))
                .on_hover_text("The play range (drag its ends on the ruler; I / O set them). Click to clear")
                .tag("timeline.range_clear", format!("{a}–{b} ×"));
            if r.clicked() {
                pb.range = None;
            }
        }
    }

    fn frame_readout(&mut self, ui: &mut egui::Ui, length: f64) {
        let fps = self.session.document.clip.fps;
        let fmt = self.timeline.time;
        let mut f = self.session.document.frame;
        ui.label("frame");
        if ui
            .add(
                egui::DragValue::new(&mut f)
                    .range(0.0..=1e6)
                    .speed(0.2)
                    .max_decimals(0),
            )
            .tag("timeline.frame", "frame")
            .changed()
        {
            self.playing = false;
            self.run("frame.set", json!({ "frame": f.round() }));
        }
        let end = (length - 1.0).max(0.0);
        let text = match fmt {
            TimeFormat::Frames => format!("/ {}", length as i64),
            _ => format!("{} / {}", fmt.show(f, fps), fmt.show(end, fps)),
        };
        ui.label(text);
    }

    /// "Undo: <step>" (click to undo) in a fixed width, so a long step name never wraps the
    /// toolbar (which would move the panels); the redo step and the history are in its tooltip.
    fn undo_readout(&mut self, ui: &mut egui::Ui) {
        let undo = self.session.undo_label().map(str::to_string);
        let text = match &undo {
            Some(l) => format!("Undo: {l}"),
            None => "Nothing to undo".into(),
        };
        let history = self.session.history();
        let mut tip = if history.is_empty() {
            "No changes yet".to_string()
        } else {
            let recent: Vec<&str> = history.iter().rev().take(12).copied().collect();
            format!(
                "Undo history (newest first):
{}",
                recent.join(
                    "
"
                )
            )
        };
        if let Some(l) = self.session.redo_label() {
            tip = format!(
                "Redo: {l}

{tip}"
            );
        }
        let width = 180.0_f32.min(ui.available_width().max(60.0));
        let r = ui
            .add_sized(
                [width, ui.spacing().interact_size.y],
                egui::Label::new(egui::RichText::new(text).weak())
                    .truncate()
                    .sense(egui::Sense::click()),
            )
            .on_hover_text(tip)
            .tag("timeline.undo", "Undo");
        if r.clicked() && undo.is_some() {
            self.run("edit.undo", json!({}));
        }
    }

    /// Insert or delete frames at the playhead (everything after slides), and scale keys.
    fn retime_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let at = self.session.document.key_frame().max(0);
            ui.label(egui::RichText::new("Retime").strong());
            ui.add(egui::DragValue::new(&mut self.timeline.retime_count).range(1..=100_000))
                .on_hover_text("How many frames")
                .tag("timeline.retime.count", "How many frames");
            let n = self.timeline.retime_count;
            if ui
                .button(format!("Insert {n} at {at}"))
                .on_hover_text("Everything from the playhead on moves later: keys, markers, layer ranges")
                .tag("timeline.retime.insert", format!("Insert {n} at {at}"))
                .clicked()
            {
                self.retime(json!({ "mode": "insert", "at": at, "count": n }));
            }
            if ui
                .button(format!("Delete {at}–{}", at + n - 1))
                .on_hover_text("Keys and markers on those frames go; everything after moves earlier")
                .tag("timeline.retime.delete", format!("Delete {at}–{}", at + n - 1))
                .clicked()
            {
                self.retime(json!({ "mode": "delete", "at": at, "count": n }));
            }
            ui.separator();
            ui.label("scale ×");
            ui.add(
                egui::DragValue::new(&mut self.timeline.retime_factor)
                    .range(0.01..=100.0)
                    .speed(0.01),
            )
            .tag("timeline.retime.factor", "scale ×");
            let what = match self.timeline.selected.len() {
                0 => "all keys".to_string(),
                n => format!("{n} selected keys"),
            };
            if ui
                .button(format!("Scale {what} about the first"))
                .on_hover_text("2 doubles the spacing, 0.5 halves it; refused if two keys would meet")
                .tag(
                    "timeline.retime.scale_first",
                    format!("Scale {what} about the first"),
                )
                .clicked()
            {
                self.scale_selection(None);
            }
            if ui
                .button(format!("about the playhead ({at})"))
                .tag(
                    "timeline.retime.scale_playhead",
                    format!("about the playhead ({at})"),
                )
                .clicked()
            {
                self.scale_selection(Some(at as f64));
            }
        });
    }
}

/// Make the Dope sheet button glow while `until` (egui's clock, seconds) is ahead: a pulsing
/// ring that fades over its last half second. Opening the dope sheet ends it.
fn glow(ui: &egui::Ui, rect: &egui::Rect, colour: egui::Color32, until: &mut Option<f64>, open: bool) {
    let Some(end) = *until else { return };
    let now = ui.input(|i| i.time);
    if open || !end.is_finite() || now >= end {
        *until = None;
        return;
    }
    let fade = ((end - now) / 0.5).clamp(0.0, 1.0) as f32;
    let pulse = 0.65 + 0.35 * (now * 6.0).sin() as f32;
    let strength = fade * pulse;
    // the outer ring reaches past the toolbar row, which the timeline's scroll area clips
    let painter = ui
        .ctx()
        .layer_painter(ui.layer_id())
        .with_clip_rect(rect.expand(10.0));
    let radius = ui.visuals().widgets.inactive.corner_radius;
    painter.rect_filled(*rect, radius, colour.gamma_multiply(0.18 * strength));
    for (grow, alpha, width) in [(1.0, 1.0, 2.0), (3.0, 0.45, 2.0), (5.0, 0.2, 2.0)] {
        painter.rect_stroke(
            rect.expand(grow),
            radius,
            egui::Stroke::new(width, colour.gamma_multiply(alpha * strength)),
            egui::StrokeKind::Outside,
        );
    }
    ui.ctx().request_repaint();
}
