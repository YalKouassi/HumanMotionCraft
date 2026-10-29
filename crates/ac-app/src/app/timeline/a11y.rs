//! Accessibility for the painted timeline: the timeline itself, the playhead (a slider with the
//! frame as its value) and every key on screen (frame, interpolation, whose, parts, selected) are
//! nodes in the accessibility tree, so a screen reader can say what a person sees. Keyboard use
//! is in `edit` (arrows / Tab between keys while the timeline has the keyboard).

use egui::{Rect, Role, accesskit};

use super::{Geo, KeyInfo, glyphs};
use crate::app::App;

fn bounds(r: Rect) -> accesskit::Rect {
    accesskit::Rect {
        x0: r.min.x.into(),
        y0: r.min.y.into(),
        x1: r.max.x.into(),
        y1: r.max.y.into(),
    }
}

/// How a key reads aloud.
pub(super) fn key_label(k: &KeyInfo) -> String {
    let who = if k.ai { "AI in-between key" } else { "key" };
    let parts = if k.partial {
        k.rows.join(", ")
    } else {
        "whole body".into()
    };
    format!("{who} on frame {}, {}, {parts}", k.frame, glyphs::label(k.interp))
}

impl App {
    pub(super) fn a11y(&self, ui: &egui::Ui, resp: &egui::Response, g: &Geo, keys: &[KeyInfo]) {
        let doc = &self.session.document;
        let frame = doc.frame;
        let len = doc.clip.length_with(&doc.links).max(1);
        let n_sel = self.timeline.selected.len();
        resp.widget_info(|| {
            egui::WidgetInfo::labeled(
                Role::Group,
                true,
                format!(
                    "Timeline: {} keys, {n_sel} selected, playhead on frame {}. Arrows or Tab move between keys",
                    keys.len(),
                    frame.round()
                ),
            )
        });
        let ctx = ui.ctx();
        let x = g.x(frame);
        ctx.accesskit_node_builder(resp.id.with("playhead"), |node| {
            node.set_role(Role::Slider);
            node.set_label("Playhead");
            node.set_numeric_value(frame);
            node.set_min_numeric_value(0.0);
            node.set_max_numeric_value((len - 1).max(0) as f64);
            node.set_numeric_value_step(1.0);
            node.set_bounds(bounds(Rect::from_min_max(
                egui::pos2(x - 5.0, g.rect.top()),
                egui::pos2(x + 5.0, g.rect.bottom()),
            )));
        });
        for k in keys {
            let kx = g.x(k.frame as f64);
            if kx < g.area.left() || kx > g.area.right() {
                continue;
            }
            let sel = self.timeline.selected.contains(&k.frame);
            ctx.accesskit_node_builder(resp.id.with(("key", k.frame)), |node| {
                node.set_role(Role::ListItem);
                node.set_label(key_label(k));
                node.set_selected(sel);
                node.set_bounds(bounds(Rect::from_center_size(
                    egui::pos2(kx, g.keys_y()),
                    egui::vec2(12.0, 12.0),
                )));
            });
        }
    }
}
