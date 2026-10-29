//! The scrub bar: the take's frames left to right, the scan's flags in red (and what they are),
//! the frames the decision cuts in orange, the selection in blue and the playhead. Click or drag
//! to scrub; Shift-drag to select.

use egui::{Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

const FLAG: Color32 = Color32::from_rgba_premultiplied(150, 40, 40, 150);
const CUT: Color32 = Color32::from_rgb(220, 130, 40);
const SELECT: Color32 = Color32::from_rgba_premultiplied(40, 90, 160, 120);

pub struct Bar<'a> {
    pub frames: usize,
    pub frame: usize,
    pub fps: f64,
    /// The scan's ranges and why.
    pub flags: &'a [(usize, usize, String)],
    /// The ranges the decision cuts (or would: the working copy).
    pub cuts: &'a [[usize; 2]],
    pub selection: Option<(usize, usize)>,
}

pub enum Change {
    Frame(usize),
    Select(usize, usize),
}

impl Bar<'_> {
    pub fn show(&self, ui: &mut egui::Ui, anchor: &mut Option<usize>) -> Option<Change> {
        let h = 54.0;
        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::click_and_drag());
        let p = ui.painter_at(rect);
        let n = self.frames.max(1);
        let x_of = |f: f64| rect.left() + (f / n as f64) as f32 * rect.width();
        let span = |a: usize, b: usize, top: f32, bottom: f32| {
            Rect::from_min_max(
                Pos2::new(x_of(a as f64), rect.top() + top),
                Pos2::new(
                    x_of(b as f64 + 1.0).max(x_of(a as f64) + 2.0),
                    rect.top() + bottom,
                ),
            )
        };
        p.rect_filled(rect, CornerRadius::same(3), Color32::from_rgb(34, 36, 40));
        // seconds
        let secs = (n as f64 / self.fps.max(1.0)).ceil() as usize;
        let every = [1, 2, 5, 10, 30, 60, 120, 300]
            .into_iter()
            .find(|s| secs / s <= 20)
            .unwrap_or(600);
        for s in (0..=secs).step_by(every) {
            let x = x_of(s as f64 * self.fps);
            p.line_segment(
                [Pos2::new(x, rect.bottom() - 8.0), Pos2::new(x, rect.bottom())],
                Stroke::new(1.0, Color32::GRAY),
            );
            p.text(
                Pos2::new(x + 2.0, rect.bottom() - 2.0),
                egui::Align2::LEFT_BOTTOM,
                format!("{s}s"),
                egui::FontId::proportional(10.0),
                Color32::GRAY,
            );
        }
        for (a, b, why) in self.flags {
            let r = span(*a, *b, 2.0, 22.0);
            p.rect_filled(r, CornerRadius::ZERO, FLAG);
            if r.width() > 40.0 {
                p.text(
                    r.left_center() + Vec2::new(3.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    why,
                    egui::FontId::proportional(10.0),
                    Color32::WHITE,
                );
            }
        }
        for [a, b] in self.cuts {
            p.rect_filled(span(*a, *b, 24.0, 32.0), CornerRadius::ZERO, CUT);
        }
        if let Some((a, b)) = self.selection {
            p.rect_filled(
                span(a.min(b), a.max(b), 0.0, h - 10.0),
                CornerRadius::ZERO,
                SELECT,
            );
        }
        let x = x_of(self.frame as f64 + 0.5);
        p.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(2.0, Color32::WHITE),
        );
        p.rect_stroke(
            rect,
            CornerRadius::same(3),
            Stroke::new(1.0, Color32::from_gray(70)),
            StrokeKind::Inside,
        );

        let at = |pos: Pos2| -> usize {
            let u = ((pos.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
            ((u * n as f32) as usize).min(n - 1)
        };
        let shift = ui.input(|i| i.modifiers.shift);
        let pos = resp.interact_pointer_pos()?;
        let f = at(pos);
        if shift && (resp.drag_started() || resp.clicked()) {
            *anchor = Some(f);
        }
        if shift
            && let Some(a) = *anchor
            && (resp.dragged() || resp.clicked() || resp.drag_started())
        {
            return Some(Change::Select(a.min(f), a.max(f)));
        }
        if resp.drag_stopped() {
            *anchor = None;
        }
        if resp.dragged() || resp.clicked() {
            return Some(Change::Frame(f));
        }
        None
    }
}
