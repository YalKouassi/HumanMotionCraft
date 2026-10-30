//! The library list's rows and group headers.
//!
//! A row is one item: its thumbnail (a lock on a built-in's), its name and details, and a star.
//! A click selects it (and shows its preview), a double-click or Enter opens an animation or
//! applies a pose, and your own items can be dragged onto a group's header to move them there,
//! or onto Starred to star them. A header folds its group away and says how many are in it. The
//! star, lock and fold arrow are drawn here, in code.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, pos2, vec2};
use serde_json::{Value, json};

use super::super::App;
use super::super::ui_ids::Tag;
use super::{ANIM_FRAMES, CELL, Thumb, hover};

/// A row's height and a header's (points).
pub const ROW_H: f32 = CELL.1 as f32 + 6.0;
const HEADER_H: f32 = 20.0;
/// Room kept at a row's right for its star.
const STAR_W: f32 = 22.0;

/// What a dragged row carries.
pub struct Dragged {
    pub anims: bool,
    pub id: String,
    pub name: String,
}

/// What a header drops onto: a group (move there) or the starred section (star it).
#[derive(Clone)]
pub enum Drop {
    Group(String),
    Star,
    Nothing,
}

/// A five-pointed star around `c` (outer radius `r`): filled, or outlined.
pub fn star(p: &egui::Painter, c: Pos2, r: f32, fill: Option<Color32>, stroke: Stroke) {
    let at = |i: usize, rad: f32| {
        let a = -FRAC_PI_2 + i as f32 * PI / 5.0;
        c + vec2(a.cos(), a.sin()) * rad
    };
    let inner = r * 0.45;
    let pts: Vec<Pos2> = (0..10)
        .map(|i| at(i, if i % 2 == 0 { r } else { inner }))
        .collect();
    if let Some(fill) = fill {
        // not convex: the inner pentagon and the five points' triangles
        let pent: Vec<Pos2> = (0..5).map(|i| at(2 * i + 1, inner)).collect();
        p.add(Shape::convex_polygon(pent, fill, Stroke::NONE));
        for i in 0..5 {
            let tri = vec![at(2 * i + 9, inner), at(2 * i, r), at(2 * i + 1, inner)];
            p.add(Shape::convex_polygon(tri, fill, Stroke::NONE));
        }
    }
    p.add(Shape::closed_line(pts, stroke));
}

/// A small padlock around `c` (`s`: its height).
fn lock(p: &egui::Painter, c: Pos2, s: f32, col: Color32) {
    let body = Rect::from_center_size(c + vec2(0.0, s * 0.15), vec2(s * 0.7, s * 0.5));
    p.rect_filled(body, 1.0, col);
    let top = pos2(c.x, body.top());
    let rad = s * 0.22;
    let arc: Vec<Pos2> = (0..=8)
        .map(|i| {
            let a = PI + i as f32 * PI / 8.0;
            top + vec2(a.cos() * rad, a.sin() * rad * 1.3)
        })
        .collect();
    p.add(Shape::line(arc, Stroke::new(1.5, col)));
}

/// The fold arrow: pointing right (folded) or down (open).
fn arrow(p: &egui::Painter, c: Pos2, open: bool, col: Color32) {
    let turn = if open { FRAC_PI_2 } else { 0.0 };
    let pts = (0..3)
        .map(|i| {
            let a = turn + i as f32 * TAU / 3.0;
            c + vec2(a.cos(), a.sin()) * 4.0
        })
        .collect();
    p.add(Shape::convex_polygon(pts, col, Stroke::NONE));
}

/// The details under an item's name: length, part, and what state it is in.
fn details(item: &Value, anims: bool, open: bool, unsaved: bool) -> String {
    let mut d = Vec::new();
    if let Some(n) = item["frames"].as_i64() {
        match item["seconds"].as_f64() {
            Some(s) => d.push(format!("{n} frames · {s:.1} s")),
            None => d.push(format!("{n} frames")),
        }
    }
    if item["loop"].as_bool() == Some(true) {
        d.push("loops".into());
    }
    if let Some(p) = item["part"]
        .as_str()
        .filter(|p| !p.is_empty() && *p != ac_core::parts::WHOLE_BODY)
    {
        d.push(p.to_string());
    }
    if open && unsaved {
        d.push("unsaved changes".into());
    }
    if item["edited"].as_bool() == Some(true) {
        d.push("edited".into());
    }
    if !anims && item["error"].is_null() && d.is_empty() {
        d.push(ac_core::parts::WHOLE_BODY.into());
    }
    if let Some(e) = item["error"].as_str() {
        d.push(e.to_string());
    }
    d.join(" · ")
}

impl App {
    /// Star or unstar an item (`library.favourite`).
    pub(super) fn lib_star(&mut self, anims: bool, id: &str, on: bool) {
        let kind = if anims { "animations" } else { "poses" };
        if self
            .run("library.favourite", json!({ "id": id, "kind": kind, "on": on }))
            .is_some()
        {
            self.status = format!("{} {id}", if on { "starred" } else { "took the star off" });
            self.refresh_library();
        }
    }

    /// One item's row (`section`: where it is drawn, as an item can be in Starred, Recent and its
    /// group); returns its response (click, double-click, right-click, drag).
    pub(super) fn lib_row(
        &mut self,
        ui: &mut egui::Ui,
        item: &Value,
        anims: bool,
        picked: bool,
        section: &str,
    ) -> egui::Response {
        let kind = if anims { "animations" } else { "poses" };
        let id = item["id"].as_str().unwrap_or_default().to_string();
        let name = item["name"].as_str().unwrap_or_default().to_string();
        let builtin = item["builtin"].as_bool() == Some(true);
        let starred = item["favourite"].as_bool() == Some(true);
        let open = anims && self.session.document.clip_id.as_deref() == Some(id.as_str());
        let rect = ui.allocate_space(vec2(ui.available_width(), ROW_H)).1;
        let r = ui.interact(
            rect,
            ui.scope_id().with(("library.row", section, &id)),
            Sense::click_and_drag(),
        );
        let r = r.tag(format!("library.item.{id}"), name.as_str());
        if !builtin {
            r.dnd_set_drag_payload(Dragged {
                anims,
                id: id.clone(),
                name: name.clone(),
            });
        }
        if !ui.is_rect_visible(rect) {
            return r;
        }
        let vis = ui.visuals().clone();
        let bg = if picked {
            Some(vis.selection.bg_fill)
        } else if open {
            Some(vis.widgets.inactive.weak_bg_fill)
        } else if r.hovered() {
            Some(vis.widgets.hovered.weak_bg_fill)
        } else {
            None
        };
        if let Some(bg) = bg {
            ui.painter().rect_filled(rect, 3.0, bg);
        }
        let inner = Rect::from_min_max(rect.min + vec2(2.0, 3.0), rect.max - vec2(STAR_W, 3.0));
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let size = vec2(
            (CELL.0 * if anims { ANIM_FRAMES as u32 } else { 1 }) as f32,
            CELL.1 as f32,
        );
        let thumb = match self.thumbs.cache.get(&format!("{kind}/{id}")) {
            Some(Thumb::Ready(t)) if anims => {
                let big = vec2(CELL.0 as f32 * 2.5, CELL.1 as f32 * 2.5);
                hover::strip(&mut child, t, size, ANIM_FRAMES, big).rect
            }
            Some(Thumb::Ready(t)) => child.add(egui::Image::new((t.id(), size))).rect,
            Some(Thumb::Failed(e)) => {
                let e = e.clone();
                child
                    .allocate_ui(size, |ui| {
                        ui.label(egui::RichText::new("no preview").small().weak())
                    })
                    .response
                    .on_hover_text(e)
                    .rect
            }
            None => {
                let (t, _) = child.allocate_exact_size(size, Sense::hover());
                child.painter().rect_filled(t, 2.0, self.palette().surface.thumb);
                self.thumbs.wanted.push((id.clone(), anims, kind.to_string()));
                t
            }
        };
        if builtin {
            let c = thumb.left_bottom() + vec2(8.0, -8.0);
            let lr = Rect::from_center_size(c, vec2(14.0, 14.0));
            child
                .painter()
                .circle_filled(c, 7.0, Color32::from_black_alpha(150));
            lock(child.painter(), c, 10.0, Color32::from_gray(230));
            child
                .interact(
                    lr,
                    child.scope_id().with(("library.lock", section, &id)),
                    Sense::hover(),
                )
                .on_hover_text("Built-in: kept as it ships. Open it and use Save as to make your own copy");
        }
        let unsaved = open && self.session.document.unsaved();
        let text_col = if picked {
            vis.selection.stroke.color
        } else {
            vis.text_color()
        };
        child.vertical(|ui| {
            let mut t = egui::RichText::new(&name).color(text_col);
            if open {
                t = t.strong();
            }
            ui.add(egui::Label::new(t).selectable(false).truncate());
            let d = details(item, anims, open, unsaved);
            ui.add(
                egui::Label::new(egui::RichText::new(d).small().weak())
                    .selectable(false)
                    .truncate(),
            );
        });
        // the star, on top of the row so its click is its own
        let sc = pos2(rect.right() - STAR_W / 2.0, rect.center().y);
        let sr = ui
            .interact(
                Rect::from_center_size(sc, vec2(18.0, 18.0)),
                ui.scope_id().with(("library.star", section, &id)),
                Sense::click(),
            )
            .on_hover_text(if starred {
                "Take the star off"
            } else {
                "Star it: Starred lists it at the top"
            })
            .tag(
                format!("library.star.{id}"),
                if starred { "Unstar" } else { "Star" },
            );
        let gold = self.palette().marker;
        if starred {
            star(ui.painter(), sc, 7.0, Some(gold), Stroke::new(1.0, gold));
        } else if r.hovered() || sr.hovered() || picked {
            star(
                ui.painter(),
                sc,
                7.0,
                None,
                Stroke::new(1.0, vis.weak_text_color()),
            );
        }
        if sr.clicked() {
            self.lib_star(anims, &id, !starred);
        }
        r
    }

    /// A header: the fold arrow, its title and count; a dragged row can be dropped on it.
    /// Returns its response (a click folds or unfolds it).
    pub(super) fn lib_header(
        &mut self,
        ui: &mut egui::Ui,
        tag: &str,
        title: &str,
        count: usize,
        open: bool,
        drop: Drop,
    ) -> egui::Response {
        let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), HEADER_H), Sense::click());
        let r = r
            .on_hover_text(if open {
                "Fold this group away"
            } else {
                "Show this group"
            })
            .tag(tag, title);
        let vis = ui.visuals().clone();
        let dragging = r.dnd_hover_payload::<Dragged>();
        let target = match (&drop, &dragging) {
            (Drop::Group(g), Some(d)) => d.id.split_once('/').is_none_or(|x| x.0 != g.as_str()),
            (Drop::Star, Some(_)) => true,
            _ => false,
        };
        if target {
            ui.painter().rect_stroke(
                rect.shrink(1.0),
                3.0,
                Stroke::new(1.5, vis.selection.stroke.color),
                egui::StrokeKind::Inside,
            );
        } else if r.hovered() {
            ui.painter()
                .rect_filled(rect, 3.0, vis.widgets.hovered.weak_bg_fill);
        }
        let ink = vis.weak_text_color();
        arrow(ui.painter(), pos2(rect.left() + 8.0, rect.center().y), open, ink);
        let mut x = rect.left() + 18.0;
        if matches!(drop, Drop::Star) {
            star(
                ui.painter(),
                pos2(x + 5.0, rect.center().y),
                5.5,
                Some(self.palette().marker),
                Stroke::NONE,
            );
            x += 14.0;
        }
        ui.painter().text(
            pos2(x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            format!("{title} ({count})"),
            egui::FontId::proportional(12.0),
            vis.text_color(),
        );
        if let Some(d) = r.dnd_release_payload::<Dragged>() {
            match drop {
                Drop::Group(g) if target => self.lib_group(d.anims, &d.id, &g),
                Drop::Star => self.lib_star(d.anims, &d.id, true),
                _ => {}
            }
        }
        r
    }

    /// While a row is dragged: its name follows the pointer.
    pub(super) fn lib_drag_ghost(&self, ctx: &egui::Context) {
        let Some(d) = egui::DragAndDrop::payload::<Dragged>(ctx) else {
            return;
        };
        let Some(at) = ctx.pointer_latest_pos() else {
            return;
        };
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        egui::Area::new(egui::Id::unique("library.drag"))
            .order(egui::Order::Tooltip)
            .fixed_pos(at + vec2(12.0, 4.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(format!("{}: drop on a group to move it there", d.name));
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn details_say_length_loop_and_state() {
        let it = json!({"frames": 240, "seconds": 8.0, "loop": true, "edited": true});
        assert_eq!(
            super::details(&it, true, false, false),
            "240 frames · 8.0 s · loops · edited"
        );
        let open = json!({"frames": 30, "seconds": 1.0});
        assert_eq!(
            super::details(&open, true, true, true),
            "30 frames · 1.0 s · unsaved changes"
        );
        let pose = json!({"part": "Left arm"});
        assert_eq!(super::details(&pose, false, false, false), "Left arm");
    }
}
