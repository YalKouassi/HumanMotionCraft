//! The selected item's preview: a popover beside the library panel with a larger picture (an
//! animation plays through [`FRAMES`] frames at its own speed), its details, and Open or Apply,
//! the star, and for a built-in "Duplicate as my own". It shows while the library has the keys
//! and something is selected; Escape, its ×, or a click outside the library hides it.
//!
//! The pictures are drawn like the thumbnails (`thumb_png`), larger, and kept in
//! `<library>/.thumbs/preview/`.

use std::collections::HashMap;
use std::time::Duration;

use egui::{Rect, vec2};
use serde_json::Value;

use super::super::App;
use super::super::ui_ids::Tag;
use super::{Thumb, hover};

/// Frames an animation's preview shows, and the size of each (points).
pub const FRAMES: usize = 12;
pub const CELL: (u32, u32) = (120, 160);
/// The popover's area (the panel's clicks reach it).
pub const AREA: &str = "library.preview";
/// The popover's width (points).
const WIDTH: f32 = 220.0;

#[derive(Default)]
pub struct Preview {
    pub cache: HashMap<String, Thumb>,
    /// Escape or × hid it (until something else is selected).
    pub hidden: bool,
    /// The selected row, as drawn this frame (none: not on screen).
    pub anchor: Option<Rect>,
    /// The selection it was shown for.
    pub shown: Option<(bool, String)>,
    /// The picture to draw on the next frame (so the popover opens at once).
    pending: Option<String>,
}

impl App {
    /// Draw the popover for the selected item (after the list, which set the anchor).
    pub(super) fn lib_preview(&mut self, ctx: &egui::Context, panel: Rect, items: &[Value]) {
        let sel = self.files.nav.sel.clone();
        if self.lib_view.preview.shown != sel {
            self.lib_view.preview.shown = sel.clone();
            self.lib_view.preview.hidden = false;
        }
        let anchor = self.lib_view.preview.anchor.take();
        let (Some((anims, id)), Some(anchor)) = (sel, anchor) else {
            return;
        };
        if !self.files.nav.focus || self.lib_view.preview.hidden || egui::DragAndDrop::has_any_payload(ctx) {
            return;
        }
        let Some(item) = items.iter().find(|it| it["id"].as_str() == Some(id.as_str())) else {
            return;
        };
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.lib_view.preview.hidden = true;
            return;
        }
        let kind = if anims { "animations" } else { "poses" };
        let key = format!("{kind}/{id}");
        // beside the panel, on the side with more room
        let screen = ctx.content_rect();
        let x = if panel.center().x < screen.center().x {
            panel.right() + 6.0
        } else {
            panel.left() - WIDTH - 22.0
        };
        let y = anchor
            .top()
            .clamp(screen.top(), (screen.bottom() - 330.0).max(screen.top()));
        let name = item["name"].as_str().unwrap_or_default().to_string();
        let builtin = item["builtin"].as_bool() == Some(true);
        let starred = item["favourite"].as_bool() == Some(true);
        let mut act: Option<&str> = None;
        let tex = match self.lib_view.preview.cache.get(&key) {
            Some(Thumb::Ready(t)) => Ok(t.clone()),
            Some(Thumb::Failed(e)) => Err(e.clone()),
            None => Err(String::new()),
        };
        let seconds = item["seconds"].as_f64().filter(|s| *s > 0.0).unwrap_or(1.0);
        egui::Area::new(egui::Id::unique(AREA))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(x, y))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(WIDTH);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&name).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .small_button("×")
                                .on_hover_text("Hide the preview (Escape)")
                                .tag("library.preview.close", "×")
                                .clicked()
                            {
                                act = Some("close");
                            }
                        });
                    });
                    ui.label(
                        egui::RichText::new(item["group"].as_str().unwrap_or_default())
                            .small()
                            .weak(),
                    );
                    let size = vec2(CELL.0 as f32 * 1.5, CELL.1 as f32 * 1.5);
                    ui.vertical_centered(|ui| match &tex {
                        Ok(t) => {
                            let n = if anims { FRAMES } else { 1 };
                            // the cells are spread evenly over the animation: play it at its speed
                            let i = (ui.input(|inp| inp.time) * n as f64 / seconds) as usize % n;
                            let uv = hover::cell_uv(t.size()[0] as f32, n, i);
                            ui.add(egui::Image::new((t.id(), size)).uv(uv));
                            if n > 1 {
                                ui.ctx().request_repaint_after(Duration::from_millis(30));
                            }
                        }
                        Err(e) if !e.is_empty() => {
                            ui.label(egui::RichText::new(format!("no preview: {e}")).small().weak());
                        }
                        Err(_) => {
                            ui.add_sized(size, egui::Spinner::new());
                        }
                    });
                    let mut facts = Vec::new();
                    if let Some(f) = item["frames"].as_i64() {
                        facts.push(format!("{f} frames"));
                    }
                    if let (Some(s), Some(fps)) = (item["seconds"].as_f64(), item["fps"].as_f64()) {
                        facts.push(format!("{s:.1} s at {fps} fps"));
                    }
                    if item["loop"].as_bool() == Some(true) {
                        facts.push("loops".into());
                    }
                    if let Some(p) = item["part"].as_str() {
                        facts.push(p.to_string());
                    }
                    if builtin {
                        facts.push(if item["edited"].as_bool() == Some(true) {
                            "built-in, edited".into()
                        } else {
                            "built-in".into()
                        });
                    }
                    ui.label(egui::RichText::new(facts.join(" · ")).small());
                    ui.horizontal_wrapped(|ui| {
                        let (go, tip) = if anims {
                            ("Open", "Open it (double-click or Enter)")
                        } else {
                            ("Apply", "Apply it to the body (double-click or Enter)")
                        };
                        if ui
                            .button(go)
                            .on_hover_text(tip)
                            .tag("library.preview.open", go)
                            .clicked()
                        {
                            act = Some("open");
                        }
                        let s = if starred { "Unstar" } else { "Star" };
                        if ui.button(s).tag("library.preview.star", s).clicked() {
                            act = Some("star");
                        }
                        if anims
                            && builtin
                            && ui
                                .button("Duplicate as my own")
                                .tag("library.preview.duplicate", "Duplicate as my own")
                                .clicked()
                        {
                            act = Some("duplicate");
                        }
                    });
                });
            });
        match act {
            Some("open") => self.lib_activate(anims, &id),
            Some("star") => self.lib_star(anims, &id, !starred),
            Some("duplicate") => self.lib_duplicate(&id),
            Some("close") => self.lib_view.preview.hidden = true,
            _ => {}
        }
        if self.lib_view.preview.cache.contains_key(&key) {
            return;
        }
        if self.lib_view.preview.pending.as_deref() != Some(key.as_str()) {
            // drawn on the next frame, so the popover opens at once
            self.lib_view.preview.pending = Some(key);
            ctx.request_repaint();
            return;
        }
        self.lib_view.preview.pending = None;
        {
            let thumb = match self
                .thumb_png(kind, &id, anims, if anims { FRAMES } else { 1 }, CELL, "preview")
                .and_then(|png| ac_render::offscreen::unpng(&png))
            {
                Ok((w, h, rgba)) => Thumb::Ready(ctx.load_texture(
                    format!("preview:{key}"),
                    egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba),
                    egui::TextureOptions::LINEAR,
                )),
                Err(e) => Thumb::Failed(e),
            };
            self.lib_view.preview.cache.insert(key, thumb);
        }
    }
}
