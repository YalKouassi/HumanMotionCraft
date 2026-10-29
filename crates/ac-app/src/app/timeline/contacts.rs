//! The foot-contact strip: a row per foot showing where it is planted (`clip.contacts`: near the
//! floor and nearly still) and where a plant pins it (`feet.plant`'s in-between keys). Clicking a
//! planted span pins the foot there, clicking a pinned one frees it, and dragging along a row
//! plants the foot over the frames dragged.

use std::collections::BTreeSet;

use ac_core::clip::KeySource;
use ac_core::keyops::PLANT_TAG;
use egui::{Pos2, Rect, Stroke};
use serde_json::json;

use super::{Drag, Geo, side_name};
use crate::app::App;

/// The contacts worked out for one version of the animation.
#[derive(Default)]
pub struct Cache {
    clip: Option<ac_core::clip::Clip>,
    /// Per frame, planted or not: left, right.
    flags: [Vec<bool>; 2],
    error: Option<String>,
}

/// Runs of planted frames, inclusive.
fn spans(flags: &[bool]) -> Vec<(i64, i64)> {
    let mut out: Vec<(i64, i64)> = Vec::new();
    for (i, on) in flags.iter().enumerate() {
        let f = i as i64;
        match out.last_mut() {
            Some(s) if *on && s.1 == f - 1 => s.1 = f,
            _ if *on => out.push((f, f)),
            _ => {}
        }
    }
    out
}

impl App {
    /// Work the contacts out again when the animation changed.
    pub(super) fn refresh_contacts(&mut self) {
        let doc = &self.session.document;
        if self.timeline.contact_cache.clip.as_ref() == Some(&doc.clip) {
            return;
        }
        let clip = doc.clip.clone();
        let cache = match self.session.run("clip.contacts", &json!({})) {
            Ok(v) => {
                let side = |s: &str| -> Vec<bool> {
                    v[s].as_array()
                        .map(|a| a.iter().map(|x| x.as_bool().unwrap_or(false)).collect())
                        .unwrap_or_default()
                };
                Cache {
                    clip: Some(clip),
                    flags: [side("L"), side("R")],
                    error: None,
                }
            }
            Err(e) => Cache {
                clip: Some(clip),
                flags: [Vec::new(), Vec::new()],
                error: Some(e.to_string()),
            },
        };
        self.timeline.contact_cache = cache;
    }

    /// Frames a plant holds (its in-between keys).
    fn pinned(&self) -> BTreeSet<i64> {
        self.session
            .document
            .clip
            .keys()
            .iter()
            .filter(|k| k.source == KeySource::Ai && k.model == PLANT_TAG)
            .map(|k| k.frame)
            .collect()
    }

    /// A span is pinned when a plant holds most of its frames.
    fn span_pinned(pinned: &BTreeSet<i64>, (a, b): (i64, i64)) -> bool {
        let n = pinned.range(a..=b).count() as i64;
        b > a && 2 * n >= b - a
    }

    pub(super) fn paint_contacts(&self, p: &egui::Painter, g: &Geo) {
        let cache = &self.timeline.contact_cache;
        let pinned = self.pinned();
        for side in 0..2 {
            let row = g.contact_row(side);
            if let Some(e) = &cache.error {
                p.text(
                    row.left_center(),
                    egui::Align2::LEFT_CENTER,
                    format!("  contacts unavailable: {e}"),
                    egui::FontId::proportional(10.0),
                    self.palette().error,
                );
                continue;
            }
            for (a, b) in spans(cache.flags.get(side).map_or(&[][..], Vec::as_slice)) {
                let r = Rect::from_min_max(
                    Pos2::new(g.x(a as f64 - 0.5), row.top() + 3.0),
                    Pos2::new(g.x(b as f64 + 0.5), row.bottom() - 3.0),
                );
                let on = Self::span_pinned(&pinned, (a, b));
                p.rect_filled(
                    r,
                    2.0,
                    if on {
                        self.palette().pinned
                    } else {
                        self.palette().planted
                    },
                );
                if on && r.width() > 12.0 {
                    p.circle_filled(
                        Pos2::new(r.left() + 5.0, r.center().y),
                        2.5,
                        self.palette().surface.on_colour,
                    );
                }
            }
        }
        if let (Some(Drag::Plant { side, from }), Some(at)) =
            (&self.timeline.drag, p.ctx().pointer_interact_pos())
        {
            let row = g.contact_row(*side);
            let to = g.frame(at.x).round().max(0.0);
            let (a, b) = ((*from as f64).min(to), (*from as f64).max(to));
            p.rect_stroke(
                Rect::from_min_max(
                    Pos2::new(g.x(a - 0.5), row.top() + 1.0),
                    Pos2::new(g.x(b + 0.5), row.bottom() - 1.0),
                ),
                2.0,
                Stroke::new(1.5, self.palette().pinned),
                egui::StrokeKind::Inside,
            );
        }
    }

    /// The planted span of a foot around a frame.
    fn span_at(&self, side: usize, frame: i64) -> Option<(i64, i64)> {
        let flags = self.timeline.contact_cache.flags.get(side)?;
        spans(flags).into_iter().find(|(a, b)| *a <= frame && frame <= *b)
    }

    /// Click on the strip: pin the planted span there, or free a pinned one.
    pub(super) fn toggle_plant(&mut self, side: usize, frame: i64) {
        let foot = if side == 0 { "L" } else { "R" };
        let who = side_name(side);
        let Some((a, b)) = self.span_at(side, frame) else {
            self.status = format!(
                "the {who} foot is in the air on frame {frame}: drag along its row to plant it over frames you choose"
            );
            return;
        };
        if b <= a {
            self.status = format!("the {who} foot touches down on frame {a} only: nothing to hold");
            return;
        }
        let on = !Self::span_pinned(&self.pinned(), (a, b));
        if let Some(r) = self.run(
            "feet.plant",
            json!({ "foot": foot, "from": a, "to": b, "on": on }),
        ) {
            self.status = if on {
                format!("pinned the {who} foot on {a}–{b} ({} frames)", r["frames"])
            } else {
                let note = r["notes"]
                    .get(0)
                    .and_then(|n| n.as_str())
                    .unwrap_or_default()
                    .to_string();
                format!("freed the {who} foot on {a}–{b}: {note}")
            };
        }
    }

    /// The strip's tooltip at a point.
    pub(super) fn contact_tip(&self, g: &Geo, at: Pos2) -> Option<String> {
        let side = (0..2).find(|&s| g.contacts && g.contact_row(s).contains(at))?;
        let frame = g.frame(at.x).round() as i64;
        let who = side_name(side);
        Some(match self.span_at(side, frame) {
            Some(span) if Self::span_pinned(&self.pinned(), span) => format!(
                "The {who} foot is planted and pinned on {}–{}\nclick: free it",
                span.0, span.1
            ),
            Some((a, b)) => format!("The {who} foot is planted on {a}–{b}\nclick: pin it in place"),
            None => {
                format!("The {who} foot is in the air on frame {frame}\ndrag along the row: plant it there")
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_of_planted_frames() {
        assert_eq!(spans(&[false, true, true, false, true]), [(1, 2), (4, 4)]);
        assert!(spans(&[]).is_empty());
        let pinned: BTreeSet<i64> = [2, 3, 4].into();
        assert!(App::span_pinned(&pinned, (1, 6)));
        assert!(!App::span_pinned(&pinned, (0, 20)));
    }
}
