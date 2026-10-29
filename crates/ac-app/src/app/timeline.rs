//! The timeline: the toolbar (`bar`), a zoomable ruler with markers and the play range
//! (`ruler`), problem marks from other panels (`marks`), the keys drawn by their interpolation
//! (`glyphs`; one summary row, and a row per body part when the dope sheet is open), AI bands, the
//! AI preview, the foot-contact strip (`contacts`), box-select, moving keys or one part's keys,
//! snapping, the right-click menus (`menu`), copy and paste and keyboard navigation (`edit`),
//! accessibility labels (`a11y`), the pointer (`pointer`) and playback (`playback`). The curve editor is a mode of it
//! (`super::curves`).

mod a11y;
mod bar;
mod contacts;
mod edit;
mod glyphs;
pub mod marks;
mod menu;
pub mod playback;
mod pointer;
mod ruler;

use std::collections::BTreeSet;

use ac_core::clip::{Interp, KeySource};
use egui::{Pos2, Rect, Sense, Stroke, Vec2};
use serde_json::json;

use super::App;
pub use ruler::TimeFormat;

const LABELS: f32 = 74.0;
/// The ruler: frame numbers on top, markers and the play range below.
const RULER: f32 = 30.0;
/// The lane for problem marks, along the top of the summary row.
const MARK_LANE: f32 = 7.0;
const ROW: f32 = 32.0;
const PART_ROW: f32 = 17.0;
const CONTACT_ROW: f32 = 15.0;
/// Snap to a key closer than this (points) while scrubbing; Alt turns it off.
const SNAP: f32 = 7.0;
/// A key is under the pointer within this many points.
const HIT: f32 = 7.0;

/// The timeline's view and selection (window state; nothing here is saved with the animation).
pub struct Timeline {
    /// Points per frame (0: fit the clip on the next draw).
    pub zoom: f32,
    /// The first frame shown at the left edge.
    pub start: f64,
    /// A row per body part.
    pub dope: bool,
    /// The foot-contact strip.
    pub contacts: bool,
    /// The curve editor instead of the key rows.
    pub curves: bool,
    /// The retime toolbar.
    pub retime: bool,
    pub time: TimeFormat,
    pub playback: playback::Playback,
    /// Problem marks other panels put on the timeline.
    pub marks: marks::TimelineMarks,
    /// Selected keys, by frame.
    pub selected: BTreeSet<i64>,
    /// The dope-sheet row the selection is in (`None`: whole keys).
    pub part: Option<&'static str>,
    /// The timeline has the keyboard (arrows and Tab move between keys).
    pub focused: bool,
    /// The pointer was over the timeline panel last frame (its context keys work).
    pub hovered: bool,
    drag: Option<Drag>,
    menu: Option<Target>,
    /// A marker being renamed: its frame and the text so far.
    renaming: Option<(i64, String)>,
    /// The key menu's "copy pose to frame" target.
    copy_to: i64,
    retime_count: i64,
    retime_factor: f64,
    contact_cache: contacts::Cache,
    pub(super) curve_view: super::curves::CurveView,
}

impl Default for Timeline {
    fn default() -> Self {
        Timeline {
            zoom: 0.0,
            start: 0.0,
            dope: false,
            contacts: false,
            curves: false,
            retime: false,
            time: TimeFormat::Frames,
            playback: playback::Playback::default(),
            marks: marks::TimelineMarks::default(),
            selected: BTreeSet::new(),
            part: None,
            focused: false,
            hovered: false,
            drag: None,
            menu: None,
            renaming: None,
            copy_to: 0,
            retime_count: 5,
            retime_factor: 2.0,
            contact_cache: contacts::Cache::default(),
            curve_view: super::curves::CurveView::default(),
        }
    }
}

enum Drag {
    Scrub,
    /// Moving the selected keys (of `part` only, on a dope-sheet row): where the pointer went down.
    Keys {
        from: f64,
        part: Option<&'static str>,
    },
    /// A selection box from this point.
    Box {
        from: Pos2,
    },
    /// A marker, from its frame.
    Marker {
        frame: i64,
    },
    /// The play range's in (or out) point.
    Range {
        out: bool,
    },
    /// Planting a foot (0 left, 1 right) from this frame.
    Plant {
        side: usize,
        from: i64,
    },
}

/// What a right-click was on: the context menu's subject.
#[derive(Clone, Copy)]
enum Target {
    Key { frame: i64, part: Option<&'static str> },
    Ruler { frame: i64 },
    Marker { frame: i64 },
    Empty { frame: i64 },
}

/// Where the parts of the timeline are, for one draw.
#[derive(Clone, Copy)]
struct Geo {
    rect: Rect,
    area: Rect,
    summary: Rect,
    rows: usize,
    contacts: bool,
    start: f64,
    zoom: f32,
}

impl Geo {
    fn x(&self, f: f64) -> f32 {
        self.area.left() + 8.0 + ((f - self.start) as f32) * self.zoom
    }

    fn frame(&self, x: f32) -> f64 {
        self.start + f64::from((x - self.area.left() - 8.0) / self.zoom)
    }

    fn marks_lane(&self) -> Rect {
        Rect::from_min_size(self.summary.min, Vec2::new(self.summary.width(), MARK_LANE))
    }

    fn row(&self, i: usize) -> Rect {
        Rect::from_min_size(
            Pos2::new(self.area.left(), self.summary.bottom() + i as f32 * PART_ROW),
            Vec2::new(self.area.width(), PART_ROW),
        )
    }

    /// The contact strip's row for a foot (0 left, 1 right).
    fn contact_row(&self, side: usize) -> Rect {
        Rect::from_min_size(
            Pos2::new(
                self.area.left(),
                self.summary.bottom() + self.rows as f32 * PART_ROW + side as f32 * CONTACT_ROW,
            ),
            Vec2::new(self.area.width(), CONTACT_ROW),
        )
    }

    fn keys_y(&self) -> f32 {
        self.summary.bottom() - 10.0
    }
}

/// A key as the timeline draws it.
struct KeyInfo {
    frame: i64,
    ai: bool,
    rows: Vec<&'static str>,
    partial: bool,
    interp: Interp,
    model: String,
}

impl App {
    pub(super) fn timeline(&mut self, ui: &mut egui::Ui) {
        let doc = &self.session.document;
        let length = doc.clip.length_with(&doc.links).max(1) as f64;
        // the open menu's items say where they are as they are drawn
        self.layout["menu"] = json!({});
        let fit = self.timeline_bar(ui, length);
        if self.timeline.curves {
            // the key rows (and their keyboard) aren't on screen
            self.timeline.focused = false;
            self.curve_editor(ui, length, fit);
            return;
        }
        let rows: Vec<&'static str> = if self.timeline.dope {
            ac_core::parts::ROWS.to_vec()
        } else {
            Vec::new()
        };
        let strip = if self.timeline.contacts {
            2.0 * CONTACT_ROW
        } else {
            0.0
        };
        let height = RULER + ROW + rows.len() as f32 * PART_ROW + strip + 4.0;
        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click_and_drag());
        let area = Rect::from_min_max(Pos2::new(rect.left() + LABELS, rect.top()), rect.max);
        self.zoom_and_pan(ui, &resp, area, length, fit);
        let geo = Geo {
            rect,
            area,
            summary: Rect::from_min_size(
                Pos2::new(area.left(), rect.top() + RULER),
                Vec2::new(area.width(), ROW),
            ),
            rows: rows.len(),
            contacts: self.timeline.contacts,
            start: self.timeline.start,
            zoom: self.timeline.zoom,
        };
        let keys = self.key_infos();
        let painter = ui.painter_at(rect);
        self.paint_rows(&painter, &geo, &rows, length);
        self.paint_ruler(&painter, &geo);
        self.paint_bands(&painter, &geo);
        if geo.contacts {
            self.refresh_contacts();
            self.paint_contacts(&painter, &geo);
        }
        self.paint_marks(&painter, &geo);
        self.paint_keys(&painter, &geo, &rows, &keys, &resp);
        self.paint_playhead(&painter, &geo);

        let resp = crate::app::ui_ids::Tag::tag(resp, "timeline.keys", "Keys");
        self.pointer(ui, &resp, &geo, &rows, &keys);
        self.hover_tips(&resp, &geo, &keys);
        resp.context_menu(|ui| self.context_menu(ui));
        self.keyboard(ui, &resp);
        self.a11y(ui, &resp, &geo, &keys);
        self.rename_popup(ui.ctx(), &geo);
        // last, so it holds what this frame's clicks and keys did (an agent reads it right after)
        self.publish_layout(&geo, &rows);
    }

    /// The wheel zooms about the pointer; shift + wheel or a middle drag pans.
    fn zoom_and_pan(&mut self, ui: &egui::Ui, resp: &egui::Response, area: Rect, length: f64, fit: bool) {
        let tl = &mut self.timeline;
        if fit || tl.zoom <= 0.0 {
            tl.zoom = ((area.width() - 16.0) / length as f32).clamp(0.5, 80.0);
            tl.start = 0.0;
        }
        if resp.hovered() {
            let (scroll, shift, pointer) =
                ui.input(|i| (i.smooth_scroll_delta, i.modifiers.shift, i.pointer.hover_pos()));
            let d = if shift { scroll.x + scroll.y } else { scroll.x };
            if d != 0.0 {
                tl.start -= f64::from(d / tl.zoom);
            }
            if !shift
                && scroll.y != 0.0
                && let Some(p) = pointer
            {
                let at = tl.start + f64::from((p.x - area.left() - 8.0) / tl.zoom);
                tl.zoom = (tl.zoom * (scroll.y * 0.004).exp()).clamp(0.5, 80.0);
                tl.start = at - f64::from((p.x - area.left() - 8.0) / tl.zoom);
            }
        }
        if resp.dragged_by(egui::PointerButton::Middle) {
            tl.start -= f64::from(resp.drag_delta().x / tl.zoom);
        }
        tl.start = tl.start.clamp(-5.0, length);
    }

    fn key_infos(&self) -> Vec<KeyInfo> {
        let doc = &self.session.document;
        doc.clip
            .keys()
            .iter()
            .map(|k| KeyInfo {
                frame: k.frame,
                ai: k.source == KeySource::Ai,
                rows: ac_core::parts::rows(&doc.skeleton, k.only.as_deref()),
                partial: k.only.is_some(),
                interp: k.interp,
                model: k.model.clone(),
            })
            .collect()
    }

    /// Where things are, for agents driving the window (control op `layout`).
    fn publish_layout(&mut self, g: &Geo, rows: &[&'static str]) {
        let r = g.rect;
        let tl = &self.timeline;
        let markers: serde_json::Map<String, serde_json::Value> = self
            .session
            .document
            .clip
            .markers
            .iter()
            .map(|m| (m.frame.to_string(), json!(m.name)))
            .collect();
        self.layout["timeline"] = json!({
            "rect": [r.left(), r.top(), r.right(), r.bottom()],
            "frame_x": { "origin": g.area.left() + 8.0, "start": g.start, "points_per_frame": g.zoom },
            "ruler_y": r.top() + 7.0,
            "markers_y": r.top() + RULER - 8.0,
            "marks_y": g.marks_lane().center().y,
            "keys_y": g.keys_y(),
            "rows_y": rows.iter().enumerate().map(|(i, n)| (n.to_string(), json!(g.row(i).center().y))).collect::<serde_json::Map<_, _>>(),
            "contacts_y": if g.contacts { json!({ "L": g.contact_row(0).center().y, "R": g.contact_row(1).center().y }) } else { json!(null) },
            "selected": tl.selected,
            "part": tl.part,
            "markers": markers,
            "range": tl.playback.range,
            "speed": tl.playback.speed,
            "every_frame": tl.playback.every_frame,
            "time": tl.time.name(),
            "marks": tl.marks.iter().map(|(s, m)| json!({ "source": s, "frame": m.frame, "kind": m.kind })).collect::<Vec<_>>(),
            "focused": self.timeline.focused,
            "renaming": tl.renaming.as_ref().map(|r| r.0),
        });
    }

    fn paint_rows(&self, p: &egui::Painter, g: &Geo, rows: &[&'static str], length: f64) {
        p.rect_filled(g.rect, 3.0, self.palette().surface.track);
        p.rect_filled(g.summary, 0.0, self.palette().surface.summary);
        let label = |at: Pos2, t: &str| {
            p.text(
                at,
                egui::Align2::LEFT_CENTER,
                t,
                egui::FontId::proportional(11.0),
                self.palette().surface.ink,
            )
        };
        label(Pos2::new(g.rect.left() + 6.0, g.keys_y()), "All");
        for (i, name) in rows.iter().enumerate() {
            let r = g.row(i);
            if i % 2 == 0 {
                p.rect_filled(r, 0.0, self.palette().surface.row);
            }
            let sel = self.timeline.part == Some(*name);
            p.text(
                Pos2::new(g.rect.left() + 10.0, r.center().y),
                egui::Align2::LEFT_CENTER,
                *name,
                egui::FontId::proportional(11.0),
                if sel {
                    self.palette().key_selected
                } else {
                    self.palette().surface.ink
                },
            );
        }
        if g.contacts {
            for (side, name) in [(0, "L foot"), (1, "R foot")] {
                let r = g.contact_row(side);
                p.rect_filled(
                    r,
                    0.0,
                    if side == 0 {
                        self.palette().surface.row
                    } else {
                        self.palette().surface.row_alt
                    },
                );
                label(Pos2::new(g.rect.left() + 10.0, r.center().y), name);
            }
        }
        // the clip's end, and outside the play range
        let below = Pos2::new(0.0, g.rect.top() + RULER);
        let end = g.x(length);
        p.rect_filled(
            Rect::from_min_max(Pos2::new(end, below.y), g.rect.max),
            0.0,
            self.palette().surface.past_end,
        );
        if let Some((a, b)) = self.timeline.playback.range {
            let dim = self.palette().surface.outside_range;
            let (xa, xb) = (g.x(a as f64 - 0.5), g.x(b as f64 + 0.5));
            p.rect_filled(
                Rect::from_min_max(
                    Pos2::new(g.area.left(), below.y),
                    Pos2::new(xa.max(g.area.left()), g.rect.bottom()),
                ),
                0.0,
                dim,
            );
            p.rect_filled(
                Rect::from_min_max(Pos2::new(xb.min(g.area.right()), below.y), g.rect.max),
                0.0,
                dim,
            );
        }
    }

    /// AI bands labelled with the style they were filled in, and the AI preview, behind the keys.
    fn paint_bands(&self, p: &egui::Painter, g: &Geo) {
        let doc = &self.session.document;
        let s = g.summary;
        for b in ac_core::edits::ai_bands(&doc.clip) {
            let r = Rect::from_min_max(
                Pos2::new(g.x(b.from as f64 - 0.5), s.top() + 2.0),
                Pos2::new(g.x(b.to as f64 + 0.5), s.bottom() - 2.0),
            );
            p.rect_filled(r, 3.0, super::theme::faded(self.palette().ai_band, 45));
            if r.width() > 40.0 {
                p.text(
                    r.left_top() + Vec2::new(4.0, MARK_LANE - 1.0),
                    egui::Align2::LEFT_TOP,
                    ac_core::ai::style_name(ac_core::ai::label_style(&b.model)),
                    egui::FontId::proportional(10.0),
                    self.palette().ai_band_text,
                );
            }
        }
        if let Some(pv) = doc.live_preview() {
            let r = Rect::from_min_max(
                Pos2::new(g.x(pv.from as f64 + 0.5), s.top() + 1.0),
                Pos2::new(g.x(pv.to as f64 - 0.5), s.bottom() - 1.0),
            );
            p.rect_stroke(
                r,
                3.0,
                Stroke::new(1.5, self.palette().preview),
                egui::StrokeKind::Inside,
            );
            p.text(
                Pos2::new(r.right() - 4.0, r.top() + MARK_LANE),
                egui::Align2::RIGHT_TOP,
                format!("preview · {}", pv.label),
                egui::FontId::proportional(10.0),
                self.palette().preview,
            );
        }
    }

    /// A glyph on the summary row by the key's interpolation, and on each part row it touches.
    fn paint_keys(
        &self,
        p: &egui::Painter,
        g: &Geo,
        rows: &[&'static str],
        keys: &[KeyInfo],
        resp: &egui::Response,
    ) {
        let tl = &self.timeline;
        let moving = match (&tl.drag, resp.interact_pointer_pos()) {
            (Some(Drag::Keys { from, part }), Some(at)) => {
                Some(((g.frame(at.x) - from).round() as i64, *part))
            }
            _ => None,
        };
        for k in keys {
            let sel = tl.selected.contains(&k.frame);
            let whole_moves = sel && moving.is_some_and(|(_, part)| part.is_none());
            let x_at =
                |moves: bool| g.x((k.frame + if moves { moving.map_or(0, |m| m.0) } else { 0 }) as f64);
            let x = x_at(whole_moves);
            if x < g.area.left() - 8.0 || x > g.area.right() + 8.0 {
                continue;
            }
            let pal = self.palette();
            let fill = if k.ai { pal.key_ai } else { pal.key_manual };
            let stroke = |on: bool| {
                if on {
                    Stroke::new(1.5, pal.key_selected)
                } else {
                    Stroke::new(0.5, pal.surface.outline)
                }
            };
            let whole_sel = sel && tl.part.is_none();
            let r = if k.partial { 4.5 } else { 6.0 };
            glyphs::draw(p, k.interp, Pos2::new(x, g.keys_y()), r, fill, stroke(whole_sel));
            for (i, name) in rows.iter().enumerate() {
                if k.rows.contains(name) {
                    let row_sel = sel && (tl.part.is_none() || tl.part == Some(*name));
                    let row_moves =
                        row_sel && moving.is_some_and(|(_, part)| part.is_none() || part == Some(*name));
                    let c = Pos2::new(x_at(row_moves), g.row(i).center().y);
                    glyphs::draw(p, k.interp, c, 4.0, fill, stroke(row_sel));
                }
            }
        }
        if let Some(Drag::Box { from }) = &tl.drag
            && let Some(at) = resp.interact_pointer_pos()
        {
            p.rect(
                Rect::from_two_pos(*from, at),
                0.0,
                self.palette().surface.select_fill,
                Stroke::new(1.0, self.palette().surface.select_stroke),
                egui::StrokeKind::Inside,
            );
        }
    }

    fn paint_playhead(&self, p: &egui::Painter, g: &Geo) {
        let x = g.x(self.session.document.frame);
        p.line_segment(
            [Pos2::new(x, g.rect.top()), Pos2::new(x, g.rect.bottom())],
            Stroke::new(
                if self.timeline.focused { 2.5 } else { 2.0 },
                self.palette().playhead,
            ),
        );
        p.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(x - 5.0, g.rect.top()),
                Pos2::new(x + 5.0, g.rect.top()),
                Pos2::new(x, g.rect.top() + 7.0),
            ],
            self.palette().playhead,
            Stroke::NONE,
        ));
        if self.timeline.focused {
            p.rect_stroke(
                g.rect.shrink(0.5),
                3.0,
                Stroke::new(1.0, self.palette().focus),
                egui::StrokeKind::Inside,
            );
        }
    }
}

fn side_name(side: usize) -> &'static str {
    if side == 0 { "left" } else { "right" }
}
