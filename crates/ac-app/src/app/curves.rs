//! The curve editor, a mode of the timeline (its Curves toggle): the selected joint's rotation as
//! X / Y / Z angle curves (degrees) and, for the hips, their position (hip heights), read from
//! `clip.curves`. Frames line up with the timeline (same zoom and pan). Keys are dots: drag one
//! (Shift: value only) or double-click it to type a frame and a value; both run `key.channel`.
//! Wheel: zoom frames; Ctrl+wheel: zoom values; middle drag: pan; Frame all fits everything.

use egui::{Pos2, Rect, Sense, Stroke, Vec2};
use serde_json::{Value, json};

use super::App;
use crate::app::ui_ids::Tag;

const HEIGHT: f32 = 230.0;
const LABELS: f32 = 74.0;
const NAMES: [&str; 6] = ["X angle", "Y angle", "Z angle", "hips X", "hips Y", "hips Z"];
/// A key dot is under the pointer within this many points.
const HIT: f32 = 6.0;

/// The curves of one joint for one version of the animation.
struct Curves {
    clip: ac_core::clip::Clip,
    bone: usize,
    /// Per frame, a value per channel.
    frames: Vec<Vec<f64>>,
    /// The manual keys that set the joint: frame and a value per channel.
    keys: Vec<(i64, Vec<f64>)>,
    error: Option<String>,
}

/// A key being dragged: its frame and channel, and where it is going.
#[derive(Clone, Copy)]
struct KeyDrag {
    frame: i64,
    channel: usize,
    to: i64,
    value: f64,
}

/// The curve editor's view (window state).
pub struct CurveView {
    /// Values at the bottom and top of the graph.
    lo: f64,
    hi: f64,
    /// Fit the values on the next draw.
    refit: bool,
    /// Channels shown: X, Y, Z angle, then the hips' X, Y, Z position.
    pub show: [bool; 6],
    cache: Option<Curves>,
    drag: Option<KeyDrag>,
    /// A key being typed in (double-click): frame, channel, new frame, new value.
    typing: Option<(i64, usize, i64, f64)>,
}

impl Default for CurveView {
    fn default() -> Self {
        CurveView {
            lo: -90.0,
            hi: 90.0,
            refit: true,
            show: [true, true, true, false, false, false],
            cache: None,
            drag: None,
            typing: None,
        }
    }
}

fn values(v: &Value) -> Vec<f64> {
    v.as_array()
        .map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect())
        .unwrap_or_default()
}

impl App {
    /// The joint the curves show: the one selected in the viewport, else the hips.
    fn curve_bone(&self) -> usize {
        self.selected.unwrap_or(0)
    }

    fn refresh_curves(&mut self) {
        let bone = self.curve_bone();
        let doc = &self.session.document;
        let fresh = self
            .timeline
            .curve_view
            .cache
            .as_ref()
            .is_some_and(|c| c.bone == bone && c.clip == doc.clip);
        if fresh {
            return;
        }
        let name = doc.skeleton.names.get(bone).cloned().unwrap_or_default();
        let clip = doc.clip.clone();
        let curves = match self.session.run("clip.curves", &json!({ "bone": name })) {
            Ok(v) => Curves {
                clip,
                bone,
                frames: v["frames"].as_array().into_iter().flatten().map(values).collect(),
                keys: v["keys"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|k| Some((k["frame"].as_i64()?, values(&k["values"]))))
                    .collect(),
                error: None,
            },
            Err(e) => Curves {
                clip,
                bone,
                frames: Vec::new(),
                keys: Vec::new(),
                error: Some(e.to_string()),
            },
        };
        let view = &mut self.timeline.curve_view;
        if view.cache.as_ref().is_none_or(|c| c.bone != bone) {
            view.refit = true;
        }
        view.cache = Some(curves);
    }

    /// Fit the shown channels' values in the graph.
    fn fit_values(&mut self) {
        let view = &mut self.timeline.curve_view;
        let Some(c) = &view.cache else { return };
        let shown = |i: usize| view.show.get(i).copied().unwrap_or(false);
        let all = c
            .frames
            .iter()
            .chain(c.keys.iter().map(|k| &k.1))
            .flat_map(|vs| vs.iter().enumerate().filter(|(i, _)| shown(*i)).map(|(_, v)| *v))
            .filter(|v| v.is_finite());
        let (lo, hi) = all.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
            (a.min(v), b.max(v))
        });
        if lo.is_finite() && hi.is_finite() {
            // a flat curve still gets room around it
            let pad = ((hi - lo) * 0.1).max(0.5);
            (view.lo, view.hi) = (lo - pad, hi + pad);
        }
        view.refit = false;
    }

    pub(super) fn curve_editor(&mut self, ui: &mut egui::Ui, length: f64, fit: bool) {
        self.refresh_curves();
        let bone = self.curve_bone();
        let name = self
            .session
            .document
            .skeleton
            .names
            .get(bone)
            .cloned()
            .unwrap_or_default();
        let mut frame_all = fit;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("Curves: {name}")).strong());
            if self.selected.is_none() {
                ui.label(
                    egui::RichText::new("(the hips: pick a joint in the viewport for its curves)").weak(),
                );
            }
            ui.separator();
            let channels = if bone == 0 { 6 } else { 3 };
            let colours = self.palette().channels;
            for i in 0..channels {
                let (Some(on), Some(n), Some(c)) = (
                    self.timeline.curve_view.show.get_mut(i),
                    NAMES.get(i),
                    colours.get(i),
                ) else {
                    continue;
                };
                ui.toggle_value(on, egui::RichText::new(*n).color(*c)).tag(
                    format!("curves.channel.{}", n.to_lowercase().replace(' ', "_")),
                    *n,
                );
            }
            ui.separator();
            frame_all |= ui
                .button("Frame all")
                .on_hover_text("Fit every frame and value of the shown curves")
                .tag("curves.frame_all", "Frame all")
                .clicked();
            ui.label(
                egui::RichText::new(
                    "drag a key (Shift: value only) · double-click: type it · Ctrl+wheel: zoom values",
                )
                .weak()
                .small(),
            );
        });
        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), HEIGHT), Sense::click_and_drag());
        let resp = resp.tag("curves.graph", "Curve graph");
        let area = Rect::from_min_max(Pos2::new(rect.left() + LABELS, rect.top()), rect.max);
        if frame_all || self.timeline.zoom <= 0.0 {
            self.timeline.zoom = ((area.width() - 16.0) / length as f32).clamp(0.5, 80.0);
            self.timeline.start = 0.0;
        }
        if frame_all || self.timeline.curve_view.refit {
            self.fit_values();
        }
        self.curve_pan_zoom(ui, &resp, area, length);
        let (start, zoom) = (self.timeline.start, self.timeline.zoom);
        let (lo, hi) = (self.timeline.curve_view.lo, self.timeline.curve_view.hi);
        let span = (hi - lo).max(1e-9);
        let x_of = move |f: f64| area.left() + 8.0 + ((f - start) as f32) * zoom;
        let f_of = move |x: f32| start + f64::from((x - area.left() - 8.0) / zoom);
        let y_of = move |v: f64| area.bottom() - 6.0 - ((v - lo) / span) as f32 * (area.height() - 12.0);
        let v_of = move |y: f32| lo + f64::from((area.bottom() - 6.0 - y) / (area.height() - 12.0)) * span;

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 3.0, self.palette().surface.track);
        self.curve_grid(&painter, rect, area, (lo, hi), &x_of, &y_of);
        let view = &self.timeline.curve_view;
        let mut dots: Vec<(i64, usize, Pos2)> = Vec::new();
        if let Some(c) = &view.cache {
            if let Some(e) = &c.error {
                painter.text(
                    area.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("curves unavailable: {e}"),
                    egui::FontId::proportional(12.0),
                    self.palette().error,
                );
            }
            for ch in (0..6).filter(|&i| view.show.get(i).copied().unwrap_or(false)) {
                let colour = self
                    .palette()
                    .channels
                    .get(ch)
                    .copied()
                    .unwrap_or(self.palette().surface.ink);
                let pts: Vec<Pos2> = c
                    .frames
                    .iter()
                    .enumerate()
                    .filter_map(|(f, vs)| Some(Pos2::new(x_of(f as f64), y_of(*vs.get(ch)?))))
                    .filter(|p| p.x >= area.left() - 20.0 && p.x <= area.right() + 20.0)
                    .collect();
                painter.add(egui::Shape::line(pts, Stroke::new(1.5, colour)));
                for (f, vs) in &c.keys {
                    let Some(v) = vs.get(ch) else { continue };
                    let (at_f, at_v) = match view.drag {
                        Some(d) if d.frame == *f && d.channel == ch => (d.to as f64, d.value),
                        _ => (*f as f64, *v),
                    };
                    let p = Pos2::new(x_of(at_f), y_of(at_v));
                    painter.circle(p, 4.0, colour, Stroke::new(1.0, self.palette().surface.outline));
                    dots.push((*f, ch, p));
                }
            }
        }
        let x = x_of(self.session.document.frame);
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(2.0, self.palette().playhead),
        );
        self.layout["curves"] = json!({
            "rect": [rect.left(), rect.top(), rect.right(), rect.bottom()],
            "bone": name,
            "values": [lo, hi],
            "keys": dots.iter().map(|(f, ch, p)| json!({ "frame": f, "channel": ch, "x": p.x, "y": p.y })).collect::<Vec<_>>(),
        });
        self.curve_pointer(ui, &resp, &dots, (&f_of, &v_of), &name);
        self.curve_typing(ui.ctx(), &name);
    }

    fn curve_grid(
        &self,
        p: &egui::Painter,
        rect: Rect,
        area: Rect,
        (lo, hi): (f64, f64),
        x_of: &dyn Fn(f64) -> f32,
        y_of: &dyn Fn(f64) -> f32,
    ) {
        let step = [
            0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 45.0, 90.0, 180.0, 360.0,
        ]
        .into_iter()
        .find(|s| (hi - lo) / s <= 8.0)
        .unwrap_or(720.0);
        let mut v = (lo / step).ceil() * step;
        while v <= hi {
            // sums of steps leave a hair off zero: say 0, not -0.00
            let v_label = if v.abs() < step * 1e-6 { 0.0 } else { v };
            let y = y_of(v);
            p.line_segment(
                [Pos2::new(area.left(), y), Pos2::new(area.right(), y)],
                Stroke::new(
                    1.0,
                    if v_label == 0.0 {
                        self.palette().surface.grid_zero
                    } else {
                        self.palette().surface.grid
                    },
                ),
            );
            p.text(
                Pos2::new(rect.left() + 4.0, y),
                egui::Align2::LEFT_CENTER,
                format!("{v_label:.2}"),
                egui::FontId::monospace(10.0),
                self.palette().surface.ink_faint,
            );
            v += step;
        }
        let fps = self.session.document.clip.fps;
        let fstep = [1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0]
            .into_iter()
            .find(|s| s * f64::from(self.timeline.zoom) >= 40.0)
            .unwrap_or(1000.0);
        let mut f = ((self.timeline.start / fstep).floor() * fstep).max(0.0);
        while x_of(f) < area.right() {
            let x = x_of(f);
            p.line_segment(
                [Pos2::new(x, area.top()), Pos2::new(x, area.bottom())],
                Stroke::new(1.0, self.palette().surface.grid),
            );
            p.text(
                Pos2::new(x + 2.0, area.top() + 1.0),
                egui::Align2::LEFT_TOP,
                self.timeline.time.show(f, fps),
                egui::FontId::monospace(10.0),
                self.palette().surface.ink_faint,
            );
            f += fstep;
        }
    }

    fn curve_pan_zoom(&mut self, ui: &egui::Ui, resp: &egui::Response, area: Rect, length: f64) {
        let view_h = f64::from(area.height() - 12.0).max(1.0);
        if resp.hovered() {
            let (scroll, ctrl, shift, pointer) = ui.input(|i| {
                (
                    i.smooth_scroll_delta,
                    i.modifiers.command,
                    i.modifiers.shift,
                    i.pointer.hover_pos(),
                )
            });
            if let Some(p) = pointer {
                let cv = &mut self.timeline.curve_view;
                if ctrl && scroll.y != 0.0 {
                    let at = cv.lo + f64::from(area.bottom() - 6.0 - p.y) / view_h * (cv.hi - cv.lo);
                    let k = f64::from((-scroll.y * 0.004).exp());
                    (cv.lo, cv.hi) = (at - (at - cv.lo) * k, at + (cv.hi - at) * k);
                } else if shift {
                    self.timeline.start -= f64::from((scroll.x + scroll.y) / self.timeline.zoom);
                } else if scroll.y != 0.0 {
                    let tl = &mut self.timeline;
                    let at = tl.start + f64::from((p.x - area.left() - 8.0) / tl.zoom);
                    tl.zoom = (tl.zoom * (scroll.y * 0.004).exp()).clamp(0.5, 80.0);
                    tl.start = at - f64::from((p.x - area.left() - 8.0) / tl.zoom);
                }
            }
        }
        if resp.dragged_by(egui::PointerButton::Middle) {
            let d = resp.drag_delta();
            self.timeline.start -= f64::from(d.x / self.timeline.zoom);
            let cv = &mut self.timeline.curve_view;
            let dv = f64::from(d.y) / view_h * (cv.hi - cv.lo);
            (cv.lo, cv.hi) = (cv.lo + dv, cv.hi + dv);
        }
        self.timeline.start = self.timeline.start.clamp(-5.0, length);
    }

    fn curve_pointer(
        &mut self,
        ui: &egui::Ui,
        resp: &egui::Response,
        dots: &[(i64, usize, Pos2)],
        (f_of, v_of): (&dyn Fn(f32) -> f64, &dyn Fn(f32) -> f64),
        bone: &str,
    ) {
        // dots on top of each other (channels with the same value): the one drawn last, which is
        // the one that shows
        let dot_at = |at: Pos2| {
            dots.iter()
                .rev()
                .map(|(f, ch, p)| (*f, *ch, p.distance(at)))
                .filter(|d| d.2 < HIT)
                .min_by(|a, b| a.2.total_cmp(&b.2))
        };
        let shift = ui.input(|i| i.modifiers.shift);
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(at) = ui.input(|i| i.pointer.press_origin())
            && let Some((frame, channel, _)) = dot_at(at)
        {
            let value = self
                .timeline
                .curve_view
                .cache
                .as_ref()
                .and_then(|c| c.keys.iter().find(|k| k.0 == frame))
                .and_then(|k| k.1.get(channel).copied())
                .unwrap_or(0.0);
            self.timeline.curve_view.drag = Some(KeyDrag {
                frame,
                channel,
                to: frame,
                value,
            });
        }
        if let (Some(d), Some(at)) = (
            self.timeline.curve_view.drag.as_mut(),
            resp.interact_pointer_pos(),
        ) {
            d.value = v_of(at.y);
            d.to = if shift {
                d.frame
            } else {
                f_of(at.x).round().max(0.0) as i64
            };
        }
        if resp.drag_stopped_by(egui::PointerButton::Primary)
            && let Some(d) = self.timeline.curve_view.drag.take()
        {
            self.run(
                "key.channel",
                json!({ "bone": bone, "frame": d.frame, "to": d.to, "axis": d.channel, "value": d.value }),
            );
        }
        if resp.double_clicked()
            && let Some(at) = resp.interact_pointer_pos()
            && let Some((frame, channel, _)) = dot_at(at)
        {
            let value = self
                .timeline
                .curve_view
                .cache
                .as_ref()
                .and_then(|c| c.keys.iter().find(|k| k.0 == frame))
                .and_then(|k| k.1.get(channel).copied())
                .unwrap_or(0.0);
            self.timeline.curve_view.typing = Some((frame, channel, frame, value));
        } else if resp.clicked()
            && let Some(at) = resp.interact_pointer_pos()
            && dot_at(at).is_none()
        {
            self.playing = false;
            self.run("frame.set", json!({ "frame": f_of(at.x).round().max(0.0) }));
        }
        if let Some(at) = resp.hover_pos()
            && let Some((f, ch, _)) = dot_at(at)
        {
            let n = NAMES.get(ch).copied().unwrap_or("?");
            resp.clone().on_hover_text_at_pointer(format!(
                "{bone} {n} key on frame {f}\ndrag to move · double-click to type"
            ));
        }
    }

    /// The box a double-clicked key is typed into.
    fn curve_typing(&mut self, ctx: &egui::Context, bone: &str) {
        let Some((frame, channel, mut to, mut value)) = self.timeline.curve_view.typing else {
            return;
        };
        let mut done = None;
        let n = NAMES.get(channel).copied().unwrap_or("?");
        egui::Window::new(format!("{bone} {n} key"))
            .id(egui::Id::unique("curve-key-typing"))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("frame");
                    ui.add(egui::DragValue::new(&mut to).range(0..=10_000_000))
                        .tag("curves.key.frame", "frame");
                    ui.label(if channel < 3 { "degrees" } else { "hip heights" });
                    ui.add(egui::DragValue::new(&mut value).speed(0.1).max_decimals(4))
                        .tag("curves.key.value", "value");
                });
                ui.horizontal(|ui| {
                    if ui.button("Apply").tag("curves.key.apply", "Apply").clicked() {
                        done = Some(true);
                    }
                    if ui.button("Cancel").tag("curves.key.cancel", "Cancel").clicked() {
                        done = Some(false);
                    }
                });
            });
        self.timeline.curve_view.typing = match done {
            Some(true) => {
                self.run(
                    "key.channel",
                    json!({ "bone": bone, "frame": frame, "to": to, "axis": channel, "value": value }),
                );
                None
            }
            Some(false) => None,
            None => Some((frame, channel, to, value)),
        };
    }
}
