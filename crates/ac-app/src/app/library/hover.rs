//! Thumbnails that play while hovered: the cached contact-sheet strip's frames cycle (the one
//! showing is outlined in the strip) and a larger copy of it follows the pointer.

use std::time::Duration;

/// Frames shown a second while playing.
const FPS: f64 = 6.0;
/// The gap between cells in a strip's texture (pixels; `offscreen::grid`).
const GAP: f32 = 2.0;

/// The part of a strip texture (`w` pixels wide, `n` cells) holding cell `i`, in UV.
pub(in crate::app) fn cell_uv(w: f32, n: usize, i: usize) -> egui::Rect {
    let n = n.max(1) as f32;
    let cell = ((w - GAP * (n - 1.0)) / n).max(1.0);
    let x0 = i as f32 * (cell + GAP) / w.max(1.0);
    egui::Rect::from_min_max(egui::pos2(x0, 0.0), egui::pos2(x0 + cell / w.max(1.0), 1.0))
}

/// Draw an animation's strip of `frames` cells at `size`; while hovered it plays, with a copy of
/// the frame at `big` next to the pointer.
pub fn strip(
    ui: &mut egui::Ui,
    tex: &egui::TextureHandle,
    size: egui::Vec2,
    frames: usize,
    big: egui::Vec2,
) -> egui::Response {
    let r = ui.add(egui::Image::new((tex.id(), size)).sense(egui::Sense::hover()));
    if !r.hovered() || frames < 2 {
        return r;
    }
    let i = (ui.input(|inp| inp.time) * FPS) as usize % frames;
    let w = size.x / frames as f32;
    let cell = egui::Rect::from_min_size(r.rect.min + egui::vec2(i as f32 * w, 0.0), egui::vec2(w, size.y));
    ui.painter().rect_stroke(
        cell,
        2.0,
        egui::Stroke::new(1.5, ui.visuals().selection.stroke.color),
        egui::StrokeKind::Inside,
    );
    let uv = cell_uv(tex.size()[0] as f32, frames, i);
    ui.ctx().request_repaint_after(Duration::from_millis(60));
    r.on_hover_ui_at_pointer(|ui| {
        ui.add(egui::Image::new((tex.id(), big)).uv(uv));
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn cells_skip_the_gaps() {
        // 4 cells of 72 px and 3 gaps of 2: 294 px
        let uv = super::cell_uv(294.0, 4, 1);
        assert!((uv.min.x - 74.0 / 294.0).abs() < 1e-6);
        assert!((uv.max.x - 146.0 / 294.0).abs() < 1e-6);
    }
}
