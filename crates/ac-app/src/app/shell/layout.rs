//! The panels around the viewport: library and inspector at the sides (either way round), the
//! timeline below, each resizable, shown or hidden from View, their sizes saved; Ctrl+Space
//! maximises the viewport. (craft-ui's docking was looked at and not used yet: it would re-host
//! every panel while other work changes them; egui's resizable panels give sizes, sides and
//! hiding now.)

use super::super::App;
use super::super::prefs::{Layout, Side};

impl App {
    /// The panels and the viewport, laid out as the preferences say.
    pub(in crate::app) fn panels(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let l = self.shell.prefs.get().layout.clone();
        let full = self.shell.maximised;
        let g = self.shell.layout_gen;
        let id = |name: &'static str| (name, g, 0u32);
        let mut sizes = (None, None, None);
        if l.show_timeline && !full {
            // Until the person drags its top edge the timeline fits its content exactly (the
            // dope sheet makes it taller); after that it keeps their height and scrolls. The
            // panel's id stays the same either way, so a drag on the timeline survives a change
            // of its height.
            let natural = self.shell.timeline_natural;
            let manual = l.timeline_height > 0.0;
            if let Some(edge) = self.shell.timeline_rect {
                ui.ctx().input(|i| {
                    let on_edge =
                        |p: egui::Pos2| (p.y - edge.top()).abs() <= 5.0 && edge.x_range().contains(p.x);
                    if i.pointer.any_pressed() && i.pointer.press_origin().is_some_and(on_edge) {
                        self.shell.timeline_resizing = true;
                    }
                });
            }
            let resizing = self.shell.timeline_resizing;
            let mut p = egui::Panel::bottom(id("timeline")).resizable(true).min_size(40.0);
            if manual {
                p = p.default_size(l.timeline_height);
            } else if resizing {
                p = p.default_size(natural.max(40.0));
            } else if natural > 0.0 {
                p = p.exact_size(natural);
            }
            let mut inner = None;
            let r = p.show(ui, |ui| {
                let out = egui::ScrollArea::vertical()
                    .id_salt("timeline-scroll")
                    .auto_shrink([false, true])
                    .show(ui, |ui| self.timeline(ui));
                inner = Some((out.content_size.y, out.inner_rect.height()));
            });
            self.timeline.hovered = r.response.contains_pointer();
            self.shell.timeline_rect = Some(r.response.rect);
            let outer = r.response.rect.height();
            if let Some((content, shown)) = inner {
                self.shell.timeline_natural = content + (outer - shown).max(0.0);
            }
            // the fitted height catches up with the content a frame later (agents wait for it)
            self.shell.layout_settling =
                !manual && !resizing && (self.shell.timeline_natural - outer).abs() > 1.0;
            if manual || (resizing && (outer - natural).abs() > 2.0) {
                sizes.0 = Some(outer);
            }
        } else {
            self.shell.timeline_rect = None;
            self.shell.layout_settling = false;
        }
        if !l.show_timeline || full {
            self.timeline.hovered = false;
            self.timeline.focused = false;
        }
        let (lib_left, width_l, width_r) = match l.library_side {
            Side::Left => (true, l.library_width, l.inspector_width),
            Side::Right => (false, l.inspector_width, l.library_width),
        };
        if l.show_library && !full {
            let p = if lib_left {
                egui::Panel::left(id("library"))
            } else {
                egui::Panel::right(id("library"))
            };
            let w = if lib_left { width_l } else { width_r };
            let r = p
                .resizable(true)
                .default_size(w.clamp(120.0, 900.0))
                .show(ui, |ui| self.library_panel(ui));
            sizes.1 = Some(r.response.rect.width());
        }
        if l.show_inspector && !full {
            let p = if lib_left {
                egui::Panel::right(id("inspector"))
            } else {
                egui::Panel::left(id("inspector"))
            };
            let w = if lib_left { width_r } else { width_l };
            let r = p
                .resizable(true)
                .default_size(w.clamp(120.0, 900.0))
                .show(ui, |ui| self.inspector(ui));
            sizes.2 = Some(r.response.rect.width());
        }
        let r = egui::CentralPanel::default().show(ui, |ui| self.viewport(ui, frame));
        self.shell.viewport_rect = Some(r.response.rect);
        // sizes are saved once a resize ends
        if !ui.ctx().input(|i| i.pointer.any_down()) {
            self.shell.timeline_resizing = false;
            self.shell.prefs.update(|p| {
                let near = |a: f32, b: f32| (a - b).abs() < 0.5;
                if let Some(h) = sizes.0.filter(|h| !near(*h, p.layout.timeline_height)) {
                    p.layout.timeline_height = h;
                }
                if let Some(w) = sizes.1.filter(|w| !near(*w, p.layout.library_width)) {
                    p.layout.library_width = w;
                }
                if let Some(w) = sizes.2.filter(|w| !near(*w, p.layout.inspector_width)) {
                    p.layout.inspector_width = w;
                }
            });
        }
    }

    /// View > Reset layout: default sizes and places, every panel shown.
    pub(in crate::app) fn reset_layout(&mut self, ctx: &egui::Context) {
        self.shell.prefs.update(|p| p.layout = Layout::default());
        self.shell.maximised = false;
        // new panel ids: egui forgets the sizes it kept for the old ones
        self.shell.layout_gen = self.shell.layout_gen.wrapping_add(1);
        ctx.request_repaint();
        self.status = "layout reset".into();
    }
}
