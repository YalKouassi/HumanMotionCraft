//! The coupled joint range-of-motion (ROM) panel: plots one joint angle against another (`hip_knee`,
//! `ankle_knee`, `shoulder_twist`, `spine_twist`) over:
//! - the 2D mocap density cloud (`grid` from `assets/rig/ranges.json`),
//! - the 1st–99th percentile typical mocap band (`typical`),
//! - the published clinical hard-limit boundary (`hard`: CDC Soucie et al. 2011, AAOS),
//! - the static per-bone box (`assets/rig/limits.json`),
//! - the current clip's trajectory (`path`) and playhead (`doc.pose`).
//!
//! Frames outside a clinical hard limit become marks on the timeline (`timeline::marks`, source
//! `"range"`) and clickable rows in the panel (`frame.set`).

use std::collections::BTreeMap;

use ac_core::clip::Clip;
use ac_core::ranges::{BandPoint, PairSpec, Probe, judge};
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::app::theme::{Palette, faded};
use crate::app::timeline::marks::Mark;
use crate::app::ui_ids::Tag;

#[derive(PartialEq)]
pub(super) struct MadeFor {
    clip: Clip,
    links: BTreeMap<String, Clip>,
    pair: String,
    side: String,
    shown: bool,
}

/// The Range panel's window state between frames.
pub struct RangeState {
    pub pair: String,
    pub side: String,
    report: Option<Value>,
    error: Option<String>,
    pub(super) made_for: Option<MadeFor>,
    /// Total frames outside the clinical hard limit across all pairs/sides.
    pub outside_frames: usize,
}

impl Default for RangeState {
    fn default() -> Self {
        Self {
            pair: "hip_knee".into(),
            side: "L".into(),
            report: None,
            error: None,
            made_for: None,
            outside_frames: 0,
        }
    }
}

impl App {
    /// Recompute `clip.range` when the clip, links, chosen pair/side or physics switch changed.
    pub(in crate::app) fn range_refresh(&mut self) {
        let doc = &self.session.document;
        let pair = self.panels.range.pair.clone();
        let side = self.panels.range.side.clone();
        let shown = doc.clip.physics;
        let fresh = self.panels.range.made_for.as_ref().is_some_and(|m| {
            m.clip == doc.clip && m.links == doc.links && m.pair == pair && m.side == side && m.shown == shown
        });
        if fresh || self.session.in_gesture() {
            return;
        }
        let made_for = MadeFor {
            clip: doc.clip.clone(),
            links: doc.links.clone(),
            pair: pair.clone(),
            side: side.clone(),
            shown,
        };
        let st = &mut self.panels.range;
        st.made_for = Some(made_for);

        let motion = if shown { "shown" } else { "keys" };
        match self.session.run(
            "clip.range",
            &json!({ "pair": pair, "side": side, "motion": motion }),
        ) {
            Ok(r) => {
                let mut total_outside = 0usize;
                let mut marks = Vec::new();
                let err_col = self.palette().error;
                if let Some(sum) = r["summary"].as_array() {
                    for item in sum {
                        let out_n = item["outside_hard_frames"].as_u64().unwrap_or(0) as usize;
                        total_outside += out_n;
                        let label = item["label"].as_str().unwrap_or("Range");
                        let side_lbl = item["side"]
                            .as_str()
                            .map(|s| format!(" ({s})"))
                            .unwrap_or_default();
                        let max_deg = item["max_past_hard_deg"].as_f64().unwrap_or(0.0);
                        if let Some(spans) = item["outside_hard_spans"].as_array() {
                            for sp in spans {
                                let from = sp.get(0).and_then(Value::as_i64).unwrap_or(0);
                                let to = sp.get(1).and_then(Value::as_i64).unwrap_or(from);
                                let tip = format!(
                                    "{label}{side_lbl}: up to {max_deg:.1}° past clinical hard limit"
                                );
                                marks.push(
                                    Mark::new(from as f64, "range-limit", err_col, &tip).until(to as f64),
                                );
                            }
                        }
                    }
                }
                let st = &mut self.panels.range;
                st.outside_frames = total_outside;
                st.report = Some(r);
                st.error = None;
                self.timeline.marks.set("range", marks);
            }
            Err(e) => {
                let st = &mut self.panels.range;
                st.report = None;
                st.outside_frames = 0;
                st.error = Some(format!("range check failed: {e}"));
                self.timeline.marks.clear("range");
            }
        }
    }

    pub(in crate::app) fn range_panel(&mut self, ui: &mut egui::Ui) {
        let Some(ranges) = self.session.document.ranges.clone() else {
            let pal = self.palette();
            ui.label(
                egui::RichText::new(
                    "Coupled range-of-motion tables (assets/rig/ranges.json) are not loaded. Run `python -m ac_train.ranges` to build them.",
                )
                .color(pal.warning),
            );
            return;
        };

        if let Some(e) = &self.panels.range.error {
            let col = self.palette().error;
            ui.label(egui::RichText::new(e).color(col));
            return;
        }

        // Pair selector + L/R side buttons
        let cur_pair_id = self.panels.range.pair.clone();
        let spec = ranges
            .pairs
            .iter()
            .find(|p| p.id == cur_pair_id)
            .or_else(|| ranges.pairs.first())
            .cloned();
        let Some(spec) = spec else {
            return;
        };

        let mut new_pair: Option<String> = None;
        let mut new_side: Option<String> = None;
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("range-pair-select")
                .selected_text(&spec.label)
                .width(175.0)
                .show_ui(ui, |ui| {
                    for p in &ranges.pairs {
                        if ui
                            .selectable_label(p.id == spec.id, &p.label)
                            .on_hover_text(&p.note)
                            .clicked()
                        {
                            new_pair = Some(p.id.clone());
                        }
                    }
                })
                .response
                .tag("panel.range.pair", &spec.label);

            if spec.sided {
                for s in ["L", "R"] {
                    let chosen = self.panels.range.side == s;
                    if ui
                        .selectable_label(chosen, s)
                        .on_hover_text(if s == "L" { "Left side" } else { "Right side" })
                        .tag(format!("panel.range.side.{s}"), s)
                        .clicked()
                    {
                        new_side = Some(s.to_string());
                    }
                }
            }
        });
        if let Some(p) = new_pair {
            self.panels.range.pair = p;
            self.range_refresh();
        }
        if let Some(s) = new_side {
            self.panels.range.side = s;
            self.range_refresh();
        }

        let eff_side: String = if spec.sided {
            self.panels.range.side.clone()
        } else {
            "L".into()
        };

        // Live current pose coordinates so viewport drags move the playhead dot at 60 fps.
        let doc = &self.session.document;
        let sk = &doc.skeleton;
        let (cur_x, cur_y) = Probe::new(sk, &spec, &eff_side)
            .and_then(|p| p.point(sk, &doc.pose.rotations))
            .unwrap_or((0.0, 0.0));
        let j = judge(&spec, cur_x, cur_y);
        let (ch_lo, ch_hi, cur_close) = (j.hard_lo, j.hard_hi, j.closeness);
        let (cur_past_hard, cur_past_typ) = (j.past_hard, j.past_typical);

        let pal = self.palette();
        let jump_frame = draw_chart(
            ui,
            pal,
            &spec,
            self.panels.range.report.as_ref(),
            (cur_x, cur_y, cur_past_hard, cur_past_typ),
        );
        if let Some(f) = jump_frame {
            self.playing = false;
            self.run("frame.set", json!({ "frame": f }));
        }

        // Compact legend
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            legend_dot(ui, pal.range, "Hard limit (clinical)");
            legend_dot(ui, pal.key_ai, "Typical mocap (1–99%)");
        });

        // Live playhead readout
        let pct = (cur_close * 100.0).round() as i64;
        let (status_txt, status_col) = if cur_past_hard > 1.0 {
            (
                format!(
                    "Now: {:.1}°, {:.1}° — {pct}% (+{:.1}° past hard limit [{:.0}°, {:.0}°])",
                    cur_x, cur_y, cur_past_hard, ch_lo, ch_hi
                ),
                pal.error,
            )
        } else if cur_past_typ > 1.0 {
            (
                format!(
                    "Now: {:.1}°, {:.1}° — {pct}% of hard [{:.0}°, {:.0}°] (+{:.1}° past typical mocap)",
                    cur_x, cur_y, ch_lo, ch_hi, cur_past_typ
                ),
                pal.warning,
            )
        } else {
            (
                format!(
                    "Now: {:.1}°, {:.1}° — {pct}% of hard limit [{:.0}°, {:.0}°]",
                    cur_x, cur_y, ch_lo, ch_hi
                ),
                pal.surface.ink,
            )
        };
        ui.label(egui::RichText::new(status_txt).small().color(status_col));

        // Summary across all pairs & sides (click a row to switch pair/side and jump to its extreme frame)
        if let Some(rep) = self.panels.range.report.clone()
            && let Some(sum) = rep["summary"].as_array()
        {
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new("Extremes across clip (click to inspect)")
                    .small()
                    .weak(),
            );
            let mut pick_target: Option<(String, String, i64)> = None;
            for (idx, item) in sum.iter().enumerate() {
                let pid = item["pair"].as_str().unwrap_or_default();
                let lbl = item["label"].as_str().unwrap_or_default();
                let s_opt = item["side"].as_str();
                let side_str = s_opt.unwrap_or("L");
                let out_n = item["outside_hard_frames"].as_u64().unwrap_or(0);
                let max_past = item["max_past_hard_deg"].as_f64().unwrap_or(0.0);
                let close_pct = (item["max_closeness"].as_f64().unwrap_or(0.0) * 100.0).round() as i64;
                let target_frame = item["worst_frame"]
                    .as_i64()
                    .or_else(|| item["peak_frame"].as_i64())
                    .unwrap_or(0);
                let name = if let Some(s) = s_opt {
                    format!("{lbl} ({s})")
                } else {
                    lbl.to_string()
                };
                let (row_txt, col) = if out_n > 0 {
                    (
                        format!("{name}: {out_n}f past hard (+{max_past:.1}° at f{target_frame})"),
                        pal.error,
                    )
                } else {
                    (
                        format!("{name}: peak {close_pct}% at f{target_frame}"),
                        if close_pct >= 90 {
                            pal.warning
                        } else {
                            pal.surface.ink
                        },
                    )
                };
                let selected = pid == spec.id && (!spec.sided || side_str == eff_side);
                if ui
                    .selectable_label(selected, egui::RichText::new(&row_txt).small().color(col))
                    .on_hover_text(format!("Switch to {name} and go to frame {target_frame}"))
                    .tag(format!("panel.range.summary.{idx}"), &row_txt)
                    .clicked()
                {
                    pick_target = Some((pid.to_string(), side_str.to_string(), target_frame));
                }
            }
            if let Some((pid, s, f)) = pick_target {
                self.panels.range.pair = pid;
                self.panels.range.side = s;
                self.playing = false;
                self.run("frame.set", json!({ "frame": f }));
                self.range_refresh();
            }
        }

        // Collapsible clinical & mocap sources info
        egui::CollapsingHeader::new(egui::RichText::new("Sources & clinical notes").small())
            .id_salt("range-sources")
            .default_open(false)
            .show(ui, |ui| {
                ui.label(egui::RichText::new(&spec.note).small());
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format!(
                        "Empirical cloud: {} frames across {} takes ({})",
                        ranges.frames,
                        ranges.takes,
                        ranges.sets.join(", ")
                    ))
                    .small()
                    .weak(),
                );
                for src in &ranges.sources {
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new(&src.title).small().strong());
                    ui.label(egui::RichText::new(&src.note).small().weak());
                }
            });
    }
}

fn legend_dot(ui: &mut egui::Ui, colour: Color32, text: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 8.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 3.5, colour);
        ui.label(egui::RichText::new(text).small().weak());
    });
}

/// Draw the 2D coupled ROM chart and return a frame number if the user clicked a point on the path.
fn draw_chart(
    ui: &mut egui::Ui,
    pal: &Palette,
    spec: &PairSpec,
    report: Option<&Value>,
    playhead: (f64, f64, f64, f64),
) -> Option<i64> {
    let width = ui.available_width().max(180.0);
    let height = 185.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());
    let response = response.tag("panel.range.chart", &spec.label);
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 4.0, pal.surface.track);

    let pad = 6.0_f32;
    let plot = rect.shrink(pad);
    let [x_min, x_max] = spec.grid.x_range;
    let [y_min, y_max] = spec.grid.y_range;
    let x_span = (x_max - x_min).max(1e-6);
    let y_span = (y_max - y_min).max(1e-6);

    let to_pos = |x: f64, y: f64| -> Pos2 {
        let tx = ((x - x_min) / x_span).clamp(0.0, 1.0) as f32;
        let ty = ((y - y_min) / y_span).clamp(0.0, 1.0) as f32;
        Pos2::new(
            plot.left() + tx * plot.width(),
            plot.bottom() - ty * plot.height(),
        )
    };

    // 1. 24x24 mocap density cloud
    let cols = spec.grid.cols.max(1);
    let rows = spec.grid.rows.max(1);
    let cw = plot.width() / cols as f32;
    let ch = plot.height() / rows as f32;
    for (r, row) in spec.grid.cells.iter().enumerate().take(rows) {
        for (c, &val) in row.iter().enumerate().take(cols) {
            if val == 0 {
                continue;
            }
            let alpha = (10 + (u16::from(val) * 75 / 100)) as u8;
            let cell_rect = Rect::from_min_size(
                Pos2::new(plot.left() + c as f32 * cw, plot.bottom() - (r + 1) as f32 * ch),
                Vec2::new(cw.ceil(), ch.ceil()),
            );
            painter.rect_filled(cell_rect, 0.0, faded(pal.key_ai, alpha));
        }
    }

    // 2. Zero reference lines
    if x_min <= 0.0 && 0.0 <= x_max {
        let p0 = to_pos(0.0, y_min);
        let p1 = to_pos(0.0, y_max);
        painter.line_segment([p0, p1], Stroke::new(1.0, pal.surface.grid_zero));
    }
    if y_min <= 0.0 && 0.0 <= y_max {
        let p0 = to_pos(x_min, 0.0);
        let p1 = to_pos(x_max, 0.0);
        painter.line_segment([p0, p1], Stroke::new(1.0, pal.surface.grid_zero));
    }

    // 3. Clinical limits of the horizontal angle
    if let Some([lo, hi]) = spec.x_hard {
        for x in [lo, hi] {
            if x_min <= x && x <= x_max {
                let stroke = Stroke::new(1.0, faded(pal.range, 150));
                painter.line_segment([to_pos(x, y_min), to_pos(x, y_max)], stroke);
            }
        }
    }

    // 4. Typical mocap band (1st-99th percentile)
    draw_band(&painter, &spec.typical, &to_pos, pal.key_ai, 1.4, Some(22));

    // 5. Clinical hard limit boundary
    draw_band(&painter, &spec.hard, &to_pos, pal.range, 2.0, None);

    // 6. Current clip trajectory + clickable frame dots
    let mut clicked_frame: Option<i64> = None;
    let mut hover_tip: Option<String> = None;
    let pointer = response.hover_pos().or_else(|| response.interact_pointer_pos());
    let mut best_dist = 18.0_f32;

    if let Some(rep) = report
        && let Some(pts) = rep["path"].as_array()
    {
        let mut line: Vec<Pos2> = Vec::with_capacity(pts.len());
        for pt in pts {
            let x = pt["x"].as_f64().unwrap_or(0.0);
            let y = pt["y"].as_f64().unwrap_or(0.0);
            line.push(to_pos(x, y));
        }
        if line.len() >= 2 {
            painter.add(egui::Shape::line(
                line.clone(),
                Stroke::new(1.5, faded(pal.surface.ink, 170)),
            ));
        }
        for (pt, &pos) in pts.iter().zip(line.iter()) {
            let f = pt["frame"].as_i64().unwrap_or(0);
            let past_hard = pt["past_hard_deg"].as_f64().unwrap_or(0.0);
            let past_typ = pt["past_typical_deg"].as_f64().unwrap_or(0.0);
            let (col, rad) = if past_hard > 1.0 {
                (pal.error, 3.8)
            } else if past_typ > 1.0 {
                (pal.warning, 2.8)
            } else {
                (pal.key_manual, 2.2)
            };
            painter.circle_filled(pos, rad, col);
            if past_hard > 1.0 {
                painter.circle_stroke(pos, rad, Stroke::new(1.0, pal.surface.outline));
            }
            if let Some(ptr) = pointer {
                let d = ptr.distance(pos);
                if d < best_dist {
                    best_dist = d;
                    let x = pt["x"].as_f64().unwrap_or(0.0);
                    let y = pt["y"].as_f64().unwrap_or(0.0);
                    let h_lo = pt["hard_lo"].as_f64().unwrap_or(0.0);
                    let h_hi = pt["hard_hi"].as_f64().unwrap_or(0.0);
                    hover_tip = Some(if past_hard > 1.0 {
                        format!(
                            "Frame {f}: ({x:.1}°, {y:.1}°)\n+{past_hard:.1}° past clinical hard limit [{h_lo:.0}°, {h_hi:.0}°]\nClick to go to frame {f}"
                        )
                    } else {
                        format!(
                            "Frame {f}: ({x:.1}°, {y:.1}°)\nHard limit [{h_lo:.0}°, {h_hi:.0}°]\nClick to go to frame {f}"
                        )
                    });
                    if response.clicked() || response.dragged() {
                        clicked_frame = Some(f);
                    }
                }
            }
        }
    }

    // 7. Playhead dot
    let (cur_x, cur_y, cur_past_hard, cur_past_typ) = playhead;
    let cur_pos = to_pos(cur_x, cur_y);
    let head_col = if cur_past_hard > 1.0 {
        pal.error
    } else if cur_past_typ > 1.0 {
        pal.warning
    } else {
        pal.playhead
    };
    painter.circle_filled(cur_pos, 5.2, head_col);
    painter.circle_stroke(cur_pos, 5.2, Stroke::new(1.5, pal.key_selected));

    // 8. Axis labels
    let font = egui::FontId::proportional(10.5);
    painter.text(
        Pos2::new(plot.left() + 4.0, plot.top() + 2.0),
        egui::Align2::LEFT_TOP,
        format!("{} ({y_max:.0}°)", spec.y.label),
        font.clone(),
        pal.surface.ink,
    );
    painter.text(
        Pos2::new(plot.right() - 4.0, plot.bottom() - 2.0),
        egui::Align2::RIGHT_BOTTOM,
        format!("{} ({x_min:.0}°..{x_max:.0}°)", spec.x.label),
        font,
        pal.surface.ink,
    );

    painter.rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0, pal.surface.grid),
        egui::StrokeKind::Inside,
    );

    if let Some(tip) = hover_tip {
        response.on_hover_text(tip);
    }
    clicked_frame
}

fn draw_band(
    painter: &egui::Painter,
    pts: &[BandPoint],
    to_pos: &impl Fn(f64, f64) -> Pos2,
    colour: Color32,
    width: f32,
    fill_alpha: Option<u8>,
) {
    if pts.len() < 2 {
        return;
    }
    if let Some(a) = fill_alpha {
        let fill_col = faded(colour, a);
        for w in pts.windows(2) {
            let (p0, p1) = (w[0], w[1]);
            let a_hi = to_pos(p0.x, p0.hi);
            let b_hi = to_pos(p1.x, p1.hi);
            let b_lo = to_pos(p1.x, p1.lo);
            let a_lo = to_pos(p0.x, p0.lo);
            let x0 = a_hi.x.min(b_hi.x);
            let x1 = a_hi.x.max(b_hi.x);
            let y0 = a_hi.y.min(b_hi.y);
            let y1 = a_lo.y.max(b_lo.y);
            painter.rect_filled(
                Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1)),
                0.0,
                fill_col,
            );
        }
    }
    let lo_line: Vec<Pos2> = pts.iter().map(|p| to_pos(p.x, p.lo)).collect();
    let hi_line: Vec<Pos2> = pts.iter().map(|p| to_pos(p.x, p.hi)).collect();
    painter.add(egui::Shape::line(lo_line, Stroke::new(width, colour)));
    painter.add(egui::Shape::line(hi_line, Stroke::new(width, colour)));
}
