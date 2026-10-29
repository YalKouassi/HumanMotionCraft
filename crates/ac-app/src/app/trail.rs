//! Motion trails: the path the selected joint takes over the animation (or a span around the
//! playhead), with a tick on every frame, a dot on every key that sets the joint and a ring on
//! the frame shown. The points come from `trail.points` once per clip state, joint and range.

use ac_core::clip::Clip;
use ac_render::glam::Vec3;
use egui::{Color32, Pos2, Rect, Stroke, Vec2};
use serde_json::{Value, json};

use super::App;
use super::gizmo::to_screen;

/// The path before the playhead, and after it.
const TRAIL_PAST: Color32 = Color32::from_rgb(250, 160, 90);
const TRAIL_FUTURE: Color32 = Color32::from_rgb(110, 195, 255);
const TRAIL_KEY: Color32 = Color32::from_rgb(255, 214, 70);
const TRAIL_NOW: Color32 = Color32::WHITE;
const TRAIL_NOTE: Color32 = Color32::from_rgb(200, 205, 215);
/// Key frame numbers closer than this (points) to one already drawn are left out.
const LABEL_GAP: f32 = 28.0;

/// A trail: (frame, point) along it, and the frames of the keys that set the joint.
type Path = (Vec<(i64, Vec3)>, Vec<i64>);

/// The trail worked out for one clip state, joint and range.
pub struct TrailCache {
    clip: Clip,
    asked: (usize, i64, i64),
    /// (frame, point) and the key frames; or why there is none.
    path: Result<Path, String>,
}

/// The frames a trail covers: the whole animation (`span` 0) or `span` each side of `here`,
/// inside the clip and never more than `trail.points` takes.
pub fn trail_range(here: i64, span: u32, len: i64) -> (i64, i64) {
    let last = (len - 1).max(0);
    let max = ac_core::commands::view::MAX_TRAIL;
    let (from, to) = if span == 0 {
        (0, last)
    } else {
        (here - i64::from(span), here + i64::from(span))
    };
    let (from, to) = (from.clamp(0, last), to.clamp(0, last));
    if to - from + 1 > max {
        let from = (here - max / 2).clamp(0, (last - max + 1).max(0));
        (from, from + max - 1)
    } else {
        (from, to)
    }
}

fn parse(r: &Value) -> Path {
    let frames = r["frames"].as_array().cloned().unwrap_or_default();
    let points = r["points"].as_array().cloned().unwrap_or_default();
    let path = frames
        .iter()
        .zip(&points)
        .filter_map(|(f, p)| {
            let c = |i: usize| p.get(i).and_then(Value::as_f64).map(|x| x as f32);
            Some((f.as_i64()?, Vec3::new(c(0)?, c(1)?, c(2)?)))
        })
        .collect();
    let keys = r["keys"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
        .collect();
    (path, keys)
}

impl App {
    /// The selected joint's trail (or why there is none to show).
    fn trail(&mut self) -> Result<Path, String> {
        let bone = self.selected.ok_or("select a joint to see its trail")?;
        let doc = &self.session.document;
        if doc.clip.base().keys.is_empty() {
            return Err("no keys yet: nothing moves".into());
        }
        let (from, to) = trail_range(
            doc.frame.round() as i64,
            self.view.trail_span,
            doc.clip.length_with(&doc.links),
        );
        let asked = (bone, from, to);
        let fresh = self
            .view
            .trail_cache
            .as_ref()
            .is_some_and(|c| c.asked == asked && c.clip == doc.clip);
        if !fresh {
            let name = doc.skeleton.names.get(bone).cloned().unwrap_or_default();
            let path = self
                .session
                .run("trail.points", &json!({ "bone": name, "from": from, "to": to }))
                .map(|r| parse(&r))
                .map_err(|e| e.to_string());
            self.view.trail_cache = Some(TrailCache {
                clip: self.session.document.clip.clone(),
                asked,
                path,
            });
        }
        self.view
            .trail_cache
            .as_ref()
            .map_or(Err("no trail".into()), |c| c.path.clone())
    }

    /// Draw the trail; `moved` carries a Move-tool drag in progress.
    pub(super) fn draw_trail(&mut self, painter: &egui::Painter, rect: Rect, moved: &dyn Fn(Vec3) -> Vec3) {
        self.layout["trail"] = Value::Null;
        if !self.view.trails {
            return;
        }
        let (path, keys) = match self.trail() {
            Ok(t) => {
                self.layout["trail"] = json!({ "frames": t.0.len(), "keys": t.1 });
                t
            }
            Err(why) => {
                self.layout["trail"] = json!({ "note": why });
                painter.text(
                    rect.left_bottom() + Vec2::new(10.0, -10.0),
                    egui::Align2::LEFT_BOTTOM,
                    format!("Trail: {why}"),
                    egui::FontId::proportional(12.0),
                    TRAIL_NOTE,
                );
                return;
            }
        };
        let now = self.session.document.frame;
        let far = path
            .iter()
            .map(|(f, _)| (*f as f64 - now).abs())
            .fold(1.0, f64::max);
        let pts: Vec<(i64, Option<Pos2>)> = path
            .iter()
            .map(|(f, p)| (*f, to_screen(&self.camera, rect, moved(*p))))
            .collect();
        let fade = |f: i64| 1.0 - 0.65 * ((f as f64 - now).abs() / far) as f32;
        for w in pts.windows(2) {
            if let (Some(a), Some(b)) = (w[0].1, w[1].1) {
                let c = if (w[1].0 as f64) <= now {
                    TRAIL_PAST
                } else {
                    TRAIL_FUTURE
                };
                painter.line_segment([a, b], Stroke::new(2.0, c.gamma_multiply(fade(w[0].0))));
            }
        }
        // dense keys get smaller dots, and a frame number only where there is room for one
        let d = if keys.len() > 30 { 3.0 } else { 4.5 };
        let mut labels: Vec<Pos2> = Vec::new();
        for (f, s) in &pts {
            let Some(s) = s else { continue };
            if keys.contains(f) {
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        *s + Vec2::new(0.0, -d),
                        *s + Vec2::new(d, 0.0),
                        *s + Vec2::new(0.0, d),
                        *s + Vec2::new(-d, 0.0),
                    ],
                    TRAIL_KEY,
                    Stroke::new(1.0, Color32::BLACK),
                ));
                if labels.iter().all(|l| l.distance(*s) > LABEL_GAP) {
                    labels.push(*s);
                    painter.text(
                        *s + Vec2::new(6.0, -6.0),
                        egui::Align2::LEFT_BOTTOM,
                        f.to_string(),
                        egui::FontId::proportional(10.0),
                        TRAIL_KEY,
                    );
                }
            } else {
                let c = if (*f as f64) <= now {
                    TRAIL_PAST
                } else {
                    TRAIL_FUTURE
                };
                painter.circle_filled(*s, 1.8, c.gamma_multiply(fade(*f)));
            }
        }
        if let Some((_, Some(s))) = pts.iter().find(|(f, _)| *f == now.round() as i64) {
            painter.circle_stroke(*s, 5.0, Stroke::new(1.5, TRAIL_NOW));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::trail_range;

    /// The whole clip, a span clamped to the clip, and a clip longer than a trail takes centred
    /// on the playhead.
    #[test]
    fn ranges_stay_inside_the_clip() {
        assert_eq!(trail_range(5, 0, 31), (0, 30));
        assert_eq!(trail_range(5, 10, 31), (0, 15));
        assert_eq!(trail_range(28, 10, 31), (18, 30));
        assert_eq!(trail_range(5000, 0, 10_000), (4000, 5999));
        assert_eq!(trail_range(0, 0, 0), (0, 0));
    }
}
