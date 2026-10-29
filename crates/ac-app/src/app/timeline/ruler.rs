//! The ruler (frame numbers as frames, seconds or timecode), the animation's named markers on it
//! (click: go there; double-click: rename; drag: move), the play range's in and out handles, and
//! the lane of problem marks under it; plus the timeline's hover tips.

use egui::{Pos2, Rect, Stroke, Vec2};
use serde_json::json;

use super::{Drag, Geo, KeyInfo, RULER, glyphs};
use crate::app::App;

/// The ruler's lower half (markers, range handles) starts this far down.
const LOWER: f32 = 15.0;

/// How the ruler and the frame readout count time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeFormat {
    Frames,
    Seconds,
    Timecode,
}

impl TimeFormat {
    pub const ALL: [TimeFormat; 3] = [TimeFormat::Frames, TimeFormat::Seconds, TimeFormat::Timecode];

    pub fn name(self) -> &'static str {
        match self {
            TimeFormat::Frames => "frames",
            TimeFormat::Seconds => "seconds",
            TimeFormat::Timecode => "timecode",
        }
    }

    pub fn parse(s: &str) -> Option<TimeFormat> {
        TimeFormat::ALL.into_iter().find(|t| t.name() == s)
    }

    /// A frame as this format writes it (`fps` frames a second).
    pub fn show(self, frame: f64, fps: f64) -> String {
        let fps = if fps.is_finite() && fps >= 1.0 { fps } else { 30.0 };
        match self {
            TimeFormat::Frames => format!("{}", frame.round() as i64),
            TimeFormat::Seconds => format!("{:.2}s", frame / fps),
            TimeFormat::Timecode => {
                let per = fps.round().max(1.0) as i64;
                let f = frame.round().max(0.0) as i64;
                let (s, ff) = (f / per, f % per);
                format!("{:02}:{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60, ff)
            }
        }
    }

    /// Tick spacings (frames) to choose from.
    fn steps(self, fps: f64) -> Vec<f64> {
        match self {
            TimeFormat::Frames => vec![1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0],
            _ => {
                let s = fps.round().max(1.0);
                let mut v = vec![1.0, 2.0, 5.0, (s / 2.0).round().max(1.0)];
                v.extend([1.0, 2.0, 5.0, 10.0, 30.0, 60.0, 300.0, 600.0].map(|k| k * s));
                v
            }
        }
    }
}

impl App {
    pub(super) fn paint_ruler(&self, p: &egui::Painter, g: &Geo) {
        let fps = self.session.document.clip.fps;
        let fmt = self.timeline.time;
        let width: f64 = if fmt == TimeFormat::Timecode { 80.0 } else { 44.0 };
        let step = fmt
            .steps(fps)
            .into_iter()
            .find(|s| s * f64::from(g.zoom) >= width)
            .unwrap_or(600.0 * fps.max(1.0));
        let mut t = ((g.start / step).floor() * step).max(0.0);
        while g.x(t) < g.area.right() {
            let x = g.x(t);
            p.line_segment(
                [Pos2::new(x, g.rect.top() + 12.0), Pos2::new(x, g.rect.bottom())],
                Stroke::new(1.0, self.palette().surface.grid),
            );
            p.text(
                Pos2::new(x + 2.0, g.rect.top() + 1.0),
                egui::Align2::LEFT_TOP,
                fmt.show(t, fps),
                egui::FontId::monospace(10.0),
                self.palette().surface.ink_faint,
            );
            t += step;
        }
        self.paint_range(p, g);
        self.paint_markers(p, g);
    }

    /// Where the in and out handles stand (the whole animation when no range is set).
    fn range_xs(&self, g: &Geo) -> (f32, f32, bool) {
        let doc = &self.session.document;
        let len = doc.clip.length_with(&doc.links).max(1);
        let (a, b) = self.timeline.playback.span(len);
        (g.x(a - 0.5), g.x(b + 0.5), self.timeline.playback.range.is_some())
    }

    fn paint_range(&self, p: &egui::Painter, g: &Geo) {
        let (xa, xb, set) = self.range_xs(g);
        let c = if set {
            self.palette().range
        } else {
            self.palette().surface.range_off
        };
        let (top, bottom) = (g.rect.top() + LOWER, g.rect.top() + RULER);
        if set {
            p.line_segment(
                [Pos2::new(xa, bottom - 1.0), Pos2::new(xb, bottom - 1.0)],
                Stroke::new(2.0, c),
            );
        }
        for (x, dir) in [(xa, 1.0), (xb, -1.0)] {
            p.line_segment([Pos2::new(x, top), Pos2::new(x, bottom)], Stroke::new(2.0, c));
            p.line_segment(
                [Pos2::new(x, top), Pos2::new(x + 5.0 * dir, top)],
                Stroke::new(2.0, c),
            );
            p.line_segment(
                [Pos2::new(x, bottom), Pos2::new(x + 5.0 * dir, bottom)],
                Stroke::new(2.0, c),
            );
        }
    }

    fn paint_markers(&self, p: &egui::Painter, g: &Geo) {
        let marker = self.palette().marker;
        let moving = match (&self.timeline.drag, p.ctx().pointer_interact_pos()) {
            (Some(Drag::Marker { frame }), Some(at)) => Some((*frame, g.frame(at.x).round().max(0.0))),
            _ => None,
        };
        let markers = &self.session.document.clip.markers;
        for (i, m) in markers.iter().enumerate() {
            let f = match moving {
                Some((from, to)) if from == m.frame => to,
                _ => m.frame as f64,
            };
            let x = g.x(f);
            if x < g.area.left() - 120.0 || x > g.area.right() {
                continue;
            }
            let (top, bottom) = (g.rect.top() + LOWER, g.rect.top() + RULER);
            p.line_segment(
                [Pos2::new(x, top), Pos2::new(x, g.rect.bottom())],
                Stroke::new(1.0, marker.gamma_multiply(0.35)),
            );
            p.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(x, top),
                    Pos2::new(x + 7.0, top + 4.0),
                    Pos2::new(x, top + 8.0),
                ],
                marker,
                Stroke::NONE,
            ));
            p.line_segment(
                [Pos2::new(x, top), Pos2::new(x, bottom)],
                Stroke::new(1.5, marker),
            );
            // the name, cut where the next marker starts
            let next = markers.get(i + 1).map_or(g.area.right(), |n| g.x(n.frame as f64));
            let room = (next - x - 12.0).max(0.0);
            if room > 14.0 {
                let galley = p.layout_no_wrap(m.name.clone(), egui::FontId::proportional(10.0), marker);
                let clip = Rect::from_min_size(Pos2::new(x + 9.0, top), Vec2::new(room, RULER - LOWER));
                p.with_clip_rect(clip.intersect(p.clip_rect())).galley(
                    Pos2::new(x + 9.0, top),
                    galley,
                    marker,
                );
            }
        }
    }

    /// The marker under a point on the ruler's lower half: its flag, or its name.
    pub(super) fn marker_at(&self, g: &Geo, at: Pos2) -> Option<i64> {
        if !(g.rect.top() + LOWER - 2.0..=g.rect.top() + RULER).contains(&at.y) {
            return None;
        }
        let markers = &self.session.document.clip.markers;
        markers
            .iter()
            .map(|m| {
                let x = g.x(m.frame as f64);
                let name = (m.name.chars().count() as f32 * 5.5).min(110.0);
                let d = if at.x >= x && at.x <= x + 9.0 + name {
                    0.0
                } else {
                    (x - at.x).abs()
                };
                (m.frame, d)
            })
            .filter(|(_, d)| *d < 6.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(f, _)| f)
    }

    /// The play-range handle under a point: `Some(false)` in, `Some(true)` out.
    pub(super) fn range_handle_at(&self, g: &Geo, at: Pos2) -> Option<bool> {
        if at.y < g.rect.top() + LOWER - 2.0 || at.y > g.rect.top() + RULER {
            return None;
        }
        let (xa, xb, _) = self.range_xs(g);
        let (da, db) = ((at.x - xa).abs(), (at.x - xb).abs());
        if da.min(db) > 5.0 {
            return None;
        }
        Some(db < da)
    }

    pub(super) fn paint_marks(&self, p: &egui::Painter, g: &Geo) {
        if self.timeline.marks.is_empty() {
            return;
        }
        let lane = g.marks_lane();
        for (_, m) in self.timeline.marks.iter() {
            let (xa, xb) = (g.x(m.frame), g.x(m.end()));
            if xb < g.area.left() - 6.0 || xa > g.area.right() + 6.0 {
                continue;
            }
            if m.to.is_some() {
                let r = Rect::from_min_max(
                    Pos2::new(g.x(m.frame - 0.5), lane.top() + 1.0),
                    Pos2::new(g.x(m.end() + 0.5), lane.bottom()),
                );
                p.rect_filled(r, 2.0, m.colour.gamma_multiply(0.8));
            } else {
                p.add(egui::Shape::convex_polygon(
                    vec![
                        Pos2::new(xa - 4.0, lane.top()),
                        Pos2::new(xa + 4.0, lane.top()),
                        Pos2::new(xa, lane.bottom() + 1.0),
                    ],
                    m.colour,
                    Stroke::new(0.5, self.palette().surface.outline),
                ));
            }
        }
    }

    /// The problem mark under a point on its lane: the frame it starts on.
    pub(super) fn mark_at(&self, g: &Geo, at: Pos2) -> Option<i64> {
        if !g.marks_lane().expand2(Vec2::new(0.0, 1.0)).contains(at) {
            return None;
        }
        self.timeline
            .marks
            .iter()
            .find(|(_, m)| at.x >= g.x(m.frame) - 5.0 && at.x <= g.x(m.end()) + 5.0)
            .map(|(_, m)| m.frame.round().max(0.0) as i64)
    }

    /// What is under the pointer, said in a tooltip.
    pub(super) fn hover_tips(&self, resp: &egui::Response, g: &Geo, keys: &[KeyInfo]) {
        let Some(at) = resp.hover_pos() else { return };
        if self.timeline.drag.is_some() {
            return;
        }
        if at.x < g.area.left() && g.summary.y_range().contains(at.y) {
            resp.clone()
                .on_hover_ui_at_pointer(|ui| glyphs::legend(ui, self.palette()));
            return;
        }
        let tip = if let Some((_, m)) = g
            .marks_lane()
            .contains(at)
            .then(|| {
                self.timeline
                    .marks
                    .iter()
                    .find(|(_, m)| at.x >= g.x(m.frame) - 5.0 && at.x <= g.x(m.end()) + 5.0)
            })
            .flatten()
        {
            let span = match m.to {
                Some(t) => format!("frames {}–{}", m.frame, t),
                None => format!("frame {}", m.frame),
            };
            Some(format!("{} · {span}\n{}\nclick: go there", m.kind, m.tooltip))
        } else if let Some(f) = self.marker_at(g, at) {
            let name = self
                .session
                .document
                .clip
                .markers
                .iter()
                .find(|m| m.frame == f)
                .map_or("", |m| m.name.as_str());
            Some(format!(
                "Marker {name:?} on frame {f}\nclick: go there · double-click: rename · drag: move · right-click: more"
            ))
        } else if let Some(out) = self.range_handle_at(g, at) {
            Some(format!(
                "Play range {} point: drag it (I / O set it at the playhead)",
                if out { "out" } else { "in" }
            ))
        } else if let Some(t) = self.contact_tip(g, at) {
            Some(t)
        } else {
            keys.iter()
                .filter(|k| g.summary.contains(at) && (g.x(k.frame as f64) - at.x).abs() < super::HIT)
                .map(|k| {
                    let who = if k.ai {
                        format!("AI in-between ({})", if k.model.is_empty() { "model" } else { &k.model })
                    } else {
                        "your key".to_string()
                    };
                    let parts = if k.partial { k.rows.join(", ") } else { "whole body".into() };
                    format!(
                        "Frame {} · {} · {who}\n{parts}\nright-click: interpolation, hold, copy, remove parts",
                        k.frame,
                        glyphs::label(k.interp)
                    )
                })
                .next()
        };
        if let Some(t) = tip {
            resp.clone().on_hover_text_at_pointer(t);
        }
    }

    /// The marker rename box (double-click a marker): Enter keeps the name, Escape drops it.
    pub(super) fn rename_popup(&mut self, ctx: &egui::Context, g: &Geo) {
        let Some((frame, mut text)) = self.timeline.renaming.take() else {
            return;
        };
        let mut done = None;
        egui::Area::new(egui::Id::unique("marker-rename"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::new(g.x(frame as f64), g.rect.top() + RULER))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(format!("Rename the marker on frame {frame}"));
                    let te = ui.add(egui::TextEdit::singleline(&mut text).desired_width(160.0));
                    te.request_focus();
                    let (enter, esc) =
                        ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
                    ui.horizontal(|ui| {
                        if ui.button("Rename").clicked() || enter {
                            done = Some(true);
                        }
                        if ui.button("Cancel").clicked() || esc {
                            done = Some(false);
                        }
                    });
                });
            });
        match done {
            Some(true) => {
                self.run("marker.rename", json!({ "frame": frame, "name": text }));
            }
            Some(false) => {}
            None => self.timeline.renaming = Some((frame, text)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_reads_three_ways() {
        assert_eq!(TimeFormat::Frames.show(45.0, 30.0), "45");
        assert_eq!(TimeFormat::Seconds.show(45.0, 30.0), "1.50s");
        assert_eq!(
            TimeFormat::Timecode.show(3725.0 * 30.0 + 7.0, 30.0),
            "01:02:05:07"
        );
        assert_eq!(TimeFormat::parse("timecode"), Some(TimeFormat::Timecode));
    }
}
