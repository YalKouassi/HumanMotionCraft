//! The pointer on the timeline: the ruler scrubs (or grabs a marker or a play-range handle), a
//! key selects and drags (on a dope-sheet row: that part only), empty space box-selects, the
//! contact strip pins or plants a foot, the marks lane jumps; right-click opens the menus,
//! double-click renames a marker.

use std::collections::BTreeSet;

use egui::{Pos2, Rect};
use serde_json::json;

use super::{Drag, Geo, HIT, KeyInfo, SNAP, Target, side_name};
use crate::app::App;

impl App {
    /// The key under a point: on the summary row any key, on a part row a key touching that row
    /// (with the row's name).
    pub(super) fn key_at(
        &self,
        g: &Geo,
        rows: &[&'static str],
        keys: &[KeyInfo],
        at: Pos2,
    ) -> Option<(i64, Option<&'static str>)> {
        let row = (0..rows.len()).find(|&i| g.row(i).contains(at));
        let part = row.and_then(|i| rows.get(i).copied());
        if part.is_none() && !g.summary.contains(at) {
            return None;
        }
        keys.iter()
            .filter(|k| part.is_none_or(|p| k.rows.contains(&p)))
            .map(|k| (k.frame, (g.x(k.frame as f64) - at.x).abs()))
            .filter(|(_, d)| *d < HIT)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(f, _)| (f, part))
    }

    /// Select a key (shift: add or remove it); a part row's key selects that part only.
    fn pick_key(&mut self, frame: i64, part: Option<&'static str>, shift: bool) {
        let tl = &mut self.timeline;
        if shift && tl.part == part {
            if !tl.selected.remove(&frame) {
                tl.selected.insert(frame);
            }
            return;
        }
        tl.selected = BTreeSet::from([frame]);
        tl.part = part;
        self.playing = false;
        self.run("frame.set", json!({ "frame": frame }));
    }

    pub(super) fn pointer(
        &mut self,
        ui: &egui::Ui,
        resp: &egui::Response,
        g: &Geo,
        rows: &[&'static str],
        keys: &[KeyInfo],
    ) {
        let (shift, alt) = ui.input(|i| (i.modifiers.shift, i.modifiers.alt));
        if ui.input(|i| i.pointer.any_pressed()) {
            // a press on the timeline gives it the keyboard; a press anywhere else takes it away
            if resp.contains_pointer() {
                resp.request_focus();
            } else if resp.has_focus() {
                resp.surrender_focus();
            }
        }
        self.timeline.focused = resp.has_focus();
        let in_ruler = |at: Pos2| at.y < g.summary.top();
        // a drag is decided where the button went down, not where it had got to when the drag began
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(at) = ui
                .input(|i| i.pointer.press_origin())
                .or(resp.interact_pointer_pos())
        {
            let contact = (0..2).find(|&s| g.contacts && g.contact_row(s).contains(at));
            self.timeline.drag = Some(if in_ruler(at) {
                if let Some(out) = self.range_handle_at(g, at) {
                    Drag::Range { out }
                } else if let Some(frame) = self.marker_at(g, at) {
                    Drag::Marker { frame }
                } else {
                    Drag::Scrub
                }
            } else if let Some(side) = contact {
                Drag::Plant {
                    side,
                    from: g.frame(at.x).round().max(0.0) as i64,
                }
            } else if let Some((k, part)) = self.key_at(g, rows, keys, at) {
                let tl = &mut self.timeline;
                if !tl.selected.contains(&k) || tl.part != part {
                    if !shift || tl.part != part {
                        tl.selected.clear();
                    }
                    tl.selected.insert(k);
                    tl.part = part;
                }
                Drag::Keys {
                    from: g.frame(at.x),
                    part,
                }
            } else {
                Drag::Box { from: at }
            });
        }
        if resp.secondary_clicked()
            && let Some(at) = resp.interact_pointer_pos()
        {
            let frame = g.frame(at.x).round().max(0.0) as i64;
            self.timeline.menu = Some(if in_ruler(at) {
                match self.marker_at(g, at) {
                    Some(m) => Target::Marker { frame: m },
                    None => Target::Ruler { frame },
                }
            } else if let Some((k, part)) = self.key_at(g, rows, keys, at) {
                if !self.timeline.selected.contains(&k) || self.timeline.part != part {
                    self.pick_key(k, part, false);
                }
                self.timeline.copy_to = self.session.document.key_frame();
                Target::Key { frame: k, part }
            } else {
                Target::Empty { frame }
            });
        }
        if resp.double_clicked()
            && let Some(at) = resp.interact_pointer_pos()
            && let Some(m) = self.marker_at(g, at)
        {
            let name = self
                .session
                .document
                .clip
                .markers
                .iter()
                .find(|x| x.frame == m)
                .map(|x| x.name.clone());
            self.timeline.renaming = name.map(|n| (m, n));
        } else if resp.clicked()
            && let Some(at) = resp.interact_pointer_pos()
        {
            self.click(g, rows, keys, at, shift, alt);
        }
        self.drag_update(resp, g, alt, keys);
        if resp.drag_stopped_by(egui::PointerButton::Primary) {
            self.drag_end(resp, g, keys, shift);
        }
    }

    fn click(&mut self, g: &Geo, rows: &[&'static str], keys: &[KeyInfo], at: Pos2, shift: bool, alt: bool) {
        if at.y < g.summary.top() {
            if let Some(m) = self.marker_at(g, at) {
                self.playing = false;
                self.run("frame.set", json!({ "frame": m }));
            } else if self.range_handle_at(g, at).is_none() {
                self.scrub(g, at.x, alt, keys);
            }
            return;
        }
        if let Some(f) = self.mark_at(g, at) {
            self.playing = false;
            self.run("frame.set", json!({ "frame": f }));
            return;
        }
        if let Some(side) = (0..2).find(|&s| g.contacts && g.contact_row(s).contains(at)) {
            self.toggle_plant(side, g.frame(at.x).round() as i64);
            return;
        }
        match self.key_at(g, rows, keys, at) {
            Some((k, part)) => self.pick_key(k, part, shift),
            None => {
                if !shift {
                    self.timeline.selected.clear();
                    self.timeline.part = None;
                }
                self.scrub(g, at.x, alt, keys);
            }
        }
    }

    fn drag_update(&mut self, resp: &egui::Response, g: &Geo, alt: bool, keys: &[KeyInfo]) {
        let Some(at) = resp.interact_pointer_pos() else {
            return;
        };
        match self.timeline.drag {
            Some(Drag::Scrub) => self.scrub(g, at.x, alt, keys),
            Some(Drag::Range { out }) => {
                let len = self
                    .session
                    .document
                    .clip
                    .length_with(&self.session.document.links)
                    .max(1);
                let f = g.frame(at.x).round().max(0.0) as i64;
                let pb = &mut self.timeline.playback;
                let (a, b) = pb.range.unwrap_or((0, (len - 1).max(0)));
                pb.range = Some(if out { (a, f.max(a)) } else { (f.min(b), b) });
            }
            _ => {}
        }
    }

    fn drag_end(&mut self, resp: &egui::Response, g: &Geo, keys: &[KeyInfo], shift: bool) {
        let (Some(drag), Some(at)) = (self.timeline.drag.take(), resp.interact_pointer_pos()) else {
            return;
        };
        let to = g.frame(at.x).round().max(0.0) as i64;
        match drag {
            Drag::Keys { from, part } => {
                let by = (g.frame(at.x) - from).round() as i64;
                match part {
                    Some(p) => self.move_part_keys(by, p),
                    None => self.move_keys(by),
                }
            }
            Drag::Box { from } => {
                let r = Rect::from_two_pos(from, at);
                // a box inside one part row selects that part's keys only
                let rows = ac_core::parts::ROWS;
                let row = (0..g.rows).find(|&i| g.row(i).contains(from) && g.row(i).contains(at));
                let part = row.and_then(|i| rows.get(i).copied());
                let tl = &mut self.timeline;
                if !shift || tl.part != part {
                    tl.selected.clear();
                }
                tl.part = part;
                for k in keys {
                    if r.x_range().contains(g.x(k.frame as f64)) && part.is_none_or(|p| k.rows.contains(&p)) {
                        tl.selected.insert(k.frame);
                    }
                }
            }
            Drag::Marker { frame } if to != frame => {
                self.run("marker.move", json!({ "frame": frame, "to": to }));
            }
            Drag::Plant { side, from } if to != from => {
                let foot = if side == 0 { "L" } else { "R" };
                if let Some(r) = self.run(
                    "feet.plant",
                    json!({ "foot": foot, "from": from.min(to), "to": from.max(to) }),
                ) {
                    self.status = format!(
                        "planted the {} foot on {}–{} ({} frames)",
                        side_name(side),
                        from.min(to),
                        from.max(to),
                        r["frames"]
                    );
                }
            }
            _ => {}
        }
    }

    /// Move the playhead to `x`, onto a key within reach unless `alt` (whole frames).
    pub(super) fn scrub(&mut self, g: &Geo, x: f32, alt: bool, keys: &[KeyInfo]) {
        let raw = g.frame(x);
        let mut f = raw.max(0.0).round();
        if !alt
            && let Some(k) = keys
                .iter()
                .map(|k| k.frame as f64)
                .filter(|k| ((k - raw) as f32 * g.zoom).abs() < SNAP)
                .min_by(|a, b| (a - f).abs().total_cmp(&(b - f).abs()))
        {
            f = k;
        }
        self.playing = false;
        self.run("frame.set", json!({ "frame": f }));
    }
}
