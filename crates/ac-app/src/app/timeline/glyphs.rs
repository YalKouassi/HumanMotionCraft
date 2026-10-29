//! Key glyphs: each key is drawn by the interpolation of the segment it starts (step: a square,
//! linear: a diamond, smooth: a circle, ease in / out: a triangle pointing the way it speeds up,
//! ease: a flat hexagon), and the legend that says so.

use ac_core::clip::Interp;
use egui::{Color32, Painter, Pos2, Shape, Stroke, Vec2};

/// Every interpolation with its name as the menu and legend show it.
pub const ALL: &[(Interp, &str)] = &[
    (Interp::Step, "Step (hold)"),
    (Interp::Linear, "Linear"),
    (Interp::Smooth, "Smooth"),
    (Interp::EaseIn, "Ease in"),
    (Interp::EaseOut, "Ease out"),
    (Interp::Ease, "Ease in and out"),
];

pub fn label(i: Interp) -> &'static str {
    ALL.iter().find(|(x, _)| *x == i).map_or("?", |(_, n)| n)
}

/// One key glyph centred on `c`, `r` points from the centre to its edge.
pub fn draw(p: &Painter, interp: Interp, c: Pos2, r: f32, fill: Color32, stroke: Stroke) {
    let pts = |v: &[(f32, f32)]| v.iter().map(|(x, y)| c + Vec2::new(x * r, y * r)).collect();
    let shape = match interp {
        Interp::Step => Shape::convex_polygon(
            pts(&[(-0.8, -0.8), (0.8, -0.8), (0.8, 0.8), (-0.8, 0.8)]),
            fill,
            stroke,
        ),
        Interp::Linear => Shape::convex_polygon(
            pts(&[(0.0, -1.0), (1.0, 0.0), (0.0, 1.0), (-1.0, 0.0)]),
            fill,
            stroke,
        ),
        Interp::Smooth => Shape::circle_filled(c, r * 0.85, fill),
        Interp::EaseIn => {
            Shape::convex_polygon(pts(&[(-0.9, -0.95), (0.95, 0.0), (-0.9, 0.95)]), fill, stroke)
        }
        Interp::EaseOut => {
            Shape::convex_polygon(pts(&[(0.9, -0.95), (0.9, 0.95), (-0.95, 0.0)]), fill, stroke)
        }
        Interp::Ease => Shape::convex_polygon(
            pts(&[
                (-0.55, -0.6),
                (0.55, -0.6),
                (1.2, 0.0),
                (0.55, 0.6),
                (-0.55, 0.6),
                (-1.2, 0.0),
            ]),
            fill,
            stroke,
        ),
    };
    p.add(shape);
    if interp == Interp::Smooth {
        p.circle_stroke(c, r * 0.85, stroke);
    }
}

/// The legend: a glyph and its name per interpolation, then what the colours mean.
pub fn legend(ui: &mut egui::Ui, pal: &crate::app::theme::Palette) {
    let (manual, ai) = (pal.key_manual, pal.key_ai);
    ui.label(egui::RichText::new("Key shapes: how the motion leaves the key").strong());
    for (interp, name) in ALL {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(14.0), egui::Sense::hover());
            draw(
                ui.painter(),
                *interp,
                r.center(),
                5.5,
                manual,
                Stroke::new(0.5, pal.surface.outline),
            );
            ui.label(*name);
        });
    }
    ui.separator();
    ui.horizontal(|ui| {
        swatch(ui, manual);
        ui.label("your keys");
        swatch(ui, ai);
        ui.label("AI in-between keys");
    });
    ui.label("Smaller: a key of some body parts only. Outlined: selected.");
}

fn swatch(ui: &mut egui::Ui, c: Color32) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(12.0), egui::Sense::hover());
    ui.painter().rect_filled(r.shrink(1.0), 2.0, c);
}
