//! The coupled joint range-of-motion (ROM) panel: plots one joint angle against another (`hip_knee`,
//! `ankle_knee`, `shoulder_twist`, `spine_twist`) over:
//! - the 2D mocap density cloud (`grid` from `assets/rig/ranges.json`),
//! - the 1st–99th percentile typical mocap band (`typical`),
//! - the published clinical hard-limit boundary (`hard`: CDC Soucie et al. 2011, AAOS),
//! - the static per-bone box (`assets/rig/limits.json`),
//! - the current clip's trajectory (`path`) and playhead (`doc.pose`).
//!
//! Three levels (`ac_core::possible`): inside the clinical limits is typical; past them is
//! unusual or athletic, still realistic (amber); past a possible limit is impossible (red), with a
//! button that runs `clip.fix_impossible`. Frames past either become marks on the timeline
//! (`timeline::marks`, source `"range"`) and clickable rows in the panel (`frame.set`).

use std::collections::{BTreeMap, BTreeSet};

use ac_core::clip::Clip;
use ac_core::possible::Possible;
use ac_core::ranges::{BandPoint, Judged, PairSpec, Probe, judge};
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::app::theme::{Palette, faded};
use crate::app::timeline::marks::Mark;
use crate::app::ui_ids::Tag;

/// The side choice that shows the left and right charts side by side.
pub(super) const BOTH: &str = "Both";

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
    /// `"L"`, `"R"` or `"Both"` (the two charts side by side).
    pub side: String,
    /// `clip.range` for the chosen side (the left one when `side` is `"Both"`).
    report: Option<Value>,
    /// `clip.range` for the right side when `side` is `"Both"`.
    report_right: Option<Value>,
    /// `clip.possible` over the same motion: the clip's level and its impossible frames.
    possible: Option<Value>,
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
            report_right: None,
            possible: None,
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
        let both = side == BOTH;
        let first = if both { "L" } else { side.as_str() };
        let measured = self
            .session
            .run(
                "clip.range",
                &json!({ "pair": pair, "side": first, "motion": motion }),
            )
            .and_then(|r| {
                let right = if both {
                    Some(self.session.run(
                        "clip.range",
                        &json!({ "pair": pair, "side": "R", "motion": motion }),
                    )?)
                } else {
                    None
                };
                Ok((
                    r,
                    right,
                    self.session.run("clip.possible", &json!({ "motion": motion }))?,
                ))
            });
        match measured {
            Ok((r, right, possible)) => {
                let mut total_outside = 0usize;
                let mut marks = Vec::new();
                let (warn_col, err_col) = (self.palette().warning, self.palette().error);
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
                                    "{label}{side_lbl}: up to {max_deg:.1}° past the clinical limit (unusual or athletic, still realistic)"
                                );
                                marks.push(
                                    Mark::new(from as f64, "range-limit", warn_col, &tip).until(to as f64),
                                );
                            }
                        }
                    }
                }
                for b in possible["breaches"].as_array().into_iter().flatten() {
                    let tip = format!(
                        "Impossible: {} {} up to {:.0}° past the possible limit",
                        b["joint"].as_str().unwrap_or("?"),
                        b["component"].as_str().unwrap_or("?"),
                        b["past_deg"].as_f64().unwrap_or(0.0)
                    );
                    for sp in b["spans"].as_array().into_iter().flatten() {
                        let from = sp.get(0).and_then(Value::as_i64).unwrap_or(0);
                        let to = sp.get(1).and_then(Value::as_i64).unwrap_or(from);
                        marks
                            .push(Mark::new(from as f64, "range-impossible", err_col, &tip).until(to as f64));
                    }
                }
                let st = &mut self.panels.range;
                st.outside_frames = total_outside;
                st.report = Some(r);
                st.report_right = right;
                st.possible = Some(possible);
                st.error = None;
                self.timeline.marks.set("range", marks);
            }
            Err(e) => {
                let st = &mut self.panels.range;
                st.report = None;
                st.report_right = None;
                st.possible = None;
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
                for (s, tip) in [
                    ("L", "Left side"),
                    ("R", "Right side"),
                    (BOTH, "Left and right side by side"),
                ] {
                    let chosen = self.panels.range.side == s;
                    if ui
                        .selectable_label(chosen, s)
                        .on_hover_text(tip)
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

        if let Some(level) = self.panels.range.possible.clone() {
            self.range_level(ui, &level);
        }

        let both = spec.sided && self.panels.range.side == BOTH;
        let eff_side: String = if spec.sided && !both {
            self.panels.range.side.clone()
        } else {
            "L".into()
        };
        let sides: Vec<String> = if both {
            vec!["L".into(), "R".into()]
        } else {
            vec![eff_side.clone()]
        };

        // Live current pose coordinates so viewport drags move the playhead dot at 60 fps.
        let doc = &self.session.document;
        let sk = &doc.skeleton;
        let heads: Vec<(String, f64, f64, Judged)> = sides
            .into_iter()
            .map(|s| {
                let (x, y) = Probe::new(sk, &spec, &s)
                    .and_then(|p| p.point(sk, &doc.pose.rotations))
                    .unwrap_or((0.0, 0.0));
                let j = judge(&spec, x, y);
                (s, x, y, j)
            })
            .collect();
        let live = Possible::new(sk, &ranges)
            .and_then(|p| p.breaches(sk, &doc.pose.rotations))
            .unwrap_or_default();
        let live_worst = live.iter().max_by(|a, b| a.past().total_cmp(&b.past())).cloned();
        let impossible: BTreeSet<i64> = self
            .panels
            .range
            .possible
            .as_ref()
            .and_then(|p| p["breaches"].as_array())
            .into_iter()
            .flatten()
            .flat_map(|b| b["spans"].as_array().cloned().unwrap_or_default())
            .flat_map(|sp| {
                let a = sp.get(0).and_then(Value::as_i64).unwrap_or(0);
                let b = sp.get(1).and_then(Value::as_i64).unwrap_or(a);
                a..=b
            })
            .collect();

        let pal = self.palette();
        let st = &self.panels.range;
        let reports = [st.report.as_ref(), st.report_right.as_ref()];
        let playhead = |(_, x, y, j): &(String, f64, f64, Judged)| {
            (*x, *y, j.past_hard, j.past_typical, live_worst.is_some())
        };
        let mut jump_frame = None;
        if both {
            ui.columns(2, |cols| {
                for ((col, head), report) in cols.iter_mut().zip(&heads).zip(reports) {
                    let name = if head.0 == "L" { "Left" } else { "Right" };
                    col.label(egui::RichText::new(name).small().strong());
                    let tag = format!("panel.range.chart.{}", head.0);
                    if let Some(f) = draw_chart(col, pal, &spec, &tag, report, &impossible, playhead(head)) {
                        jump_frame = Some(f);
                    }
                }
            });
        } else if let Some(head) = heads.first() {
            jump_frame = draw_chart(
                ui,
                pal,
                &spec,
                "panel.range.chart",
                reports[0],
                &impossible,
                playhead(head),
            );
        }
        if let Some(f) = jump_frame {
            self.playing = false;
            self.run("frame.set", json!({ "frame": f }));
        }

        // Compact legend, in short rows: a nested horizontal does not wrap on a narrow panel
        let legend: [&[(Key, &str)]; 4] = [
            &[
                (Key::Swatch(pal.range), "Clinical limit"),
                (Key::Swatch(pal.key_ai), "Typical mocap (1–99%)"),
            ],
            &[
                (Key::Head, "You are here"),
                (Key::Dot(Dot::Within), "Within limits"),
            ],
            &[
                (Key::Dot(Dot::PastTypical), "Past typical mocap"),
                (Key::Dot(Dot::Impossible), "Impossible"),
            ],
            &[(
                Key::Dot(Dot::PastClinical),
                "Past clinical limit (unusual, still possible)",
            )],
        ];
        for row in legend {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                for (key, text) in row {
                    legend_item(ui, pal, *key, text);
                }
            });
        }

        // Live playhead readout (one line a side when both are shown)
        if let Some(w) = &live_worst {
            let txt = format!(
                "Now: impossible: {} {} {:.0}° (possible {:.0}° to {:.0}°)",
                w.joint, w.component, w.value, w.lo, w.hi
            );
            ui.label(egui::RichText::new(txt).small().color(pal.error));
        } else {
            for (side, cur_x, cur_y, j) in &heads {
                let now = if both {
                    format!("Now ({side})")
                } else {
                    "Now".into()
                };
                let (ch_lo, ch_hi) = (j.hard_lo, j.hard_hi);
                let pct = (j.closeness * 100.0).round() as i64;
                let (status_txt, status_col) = if j.past_hard > 1.0 {
                    (
                        format!(
                            "{now}: {:.1}°, {:.1}°: {:.1}° past the clinical limit [{:.0}°, {:.0}°]. Realistic, but unusual or athletic",
                            cur_x, cur_y, j.past_hard, ch_lo, ch_hi
                        ),
                        pal.warning,
                    )
                } else if j.past_typical > 1.0 {
                    (
                        format!(
                            "{now}: {:.1}°, {:.1}° — {pct}% of hard [{:.0}°, {:.0}°] (+{:.1}° past typical mocap)",
                            cur_x, cur_y, ch_lo, ch_hi, j.past_typical
                        ),
                        pal.warning,
                    )
                } else {
                    (
                        format!(
                            "{now}: {:.1}°, {:.1}° — {pct}% of hard limit [{:.0}°, {:.0}°]",
                            cur_x, cur_y, ch_lo, ch_hi
                        ),
                        pal.surface.ink,
                    )
                };
                ui.label(egui::RichText::new(status_txt).small().color(status_col));
            }
        }

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
                        format!(
                            "{name}: {out_n}f past clinical, unusual (+{max_past:.1}° at f{target_frame})"
                        ),
                        pal.warning,
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
                let selected = pid == spec.id && (!spec.sided || both || side_str == eff_side);
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
                // Showing both sides already shows the row's side: stay on Both.
                if !both {
                    self.panels.range.side = s;
                }
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

impl App {
    /// The clip's level (`clip.possible`): what it means, the impossible joints, and a fix.
    fn range_level(&mut self, ui: &mut egui::Ui, level: &Value) {
        let pal = self.palette();
        let msg = level["message"].as_str().unwrap_or_default();
        match level["level"].as_str() {
            Some("impossible") => {
                ui.label(egui::RichText::new(msg).small().color(pal.error));
                for b in level["breaches"].as_array().into_iter().flatten().take(4) {
                    let frames = b["spans"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|sp| {
                            match (
                                sp.get(0).and_then(Value::as_i64),
                                sp.get(1).and_then(Value::as_i64),
                            ) {
                                (Some(a), Some(b)) if a != b => format!("{a}–{b}"),
                                (Some(a), _) => a.to_string(),
                                _ => String::new(),
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    let row = format!(
                        "{} {}: {:.0}° past (frames {frames})",
                        b["joint"].as_str().unwrap_or("?"),
                        b["component"].as_str().unwrap_or("?"),
                        b["past_deg"].as_f64().unwrap_or(0.0)
                    );
                    let worst = b["worst_frame"].as_i64().unwrap_or(0);
                    if ui
                        .selectable_label(false, egui::RichText::new(&row).small().color(pal.error))
                        .on_hover_text(format!("Go to frame {worst}"))
                        .tag(
                            format!(
                                "panel.range.impossible.{}.{}",
                                b["joint"].as_str().unwrap_or("?"),
                                b["component"].as_str().unwrap_or("?")
                            ),
                            &row,
                        )
                        .clicked()
                    {
                        self.playing = false;
                        self.run("frame.set", json!({ "frame": worst }));
                    }
                }
                if ui
                    .button("Fix impossible poses")
                    .on_hover_text("Bend every impossible joint on your keys back to its possible limit (clip.fix_impossible; undo puts it back)")
                    .tag("panel.range.fix", "Fix impossible poses")
                    .clicked()
                {
                    self.run("clip.fix_impossible", json!({}));
                }
            }
            Some("unusual") => {
                ui.label(egui::RichText::new(msg).small().color(pal.warning));
            }
            _ => {
                ui.label(egui::RichText::new(msg).small().weak());
            }
        }
        ui.add_space(2.0);
    }
}

/// How far a frame on the path is from typical. Colour on the chart means only this: frames
/// inside the limits are neutral, so the ones that need a look stand out.
#[derive(Clone, Copy)]
enum Dot {
    Within,
    PastTypical,
    PastClinical,
    Impossible,
}

impl Dot {
    fn draw(self, painter: &egui::Painter, pal: &Palette, pos: Pos2) {
        match self {
            Dot::Within => {
                painter.circle_filled(pos, 2.2, faded(pal.surface.ink, 140));
            }
            Dot::PastTypical => {
                painter.circle_filled(pos, 2.8, pal.surface.track);
                painter.circle_stroke(pos, 2.8, Stroke::new(1.4, pal.warning));
            }
            Dot::PastClinical => {
                painter.circle_filled(pos, 3.4, pal.warning);
                painter.circle_stroke(pos, 3.4, Stroke::new(1.0, pal.surface.outline));
            }
            Dot::Impossible => {
                painter.circle_filled(pos, 3.8, pal.error);
                painter.circle_stroke(pos, 3.8, Stroke::new(1.0, pal.surface.outline));
            }
        }
    }
}

/// The playhead: a ring with crosshair ticks, so it reads as "you are here" and not as a level.
/// It is filled only when the pose now is unusual or impossible.
fn draw_head(painter: &egui::Painter, pal: &Palette, pos: Pos2, fill: Option<Color32>) {
    let ink = pal.surface.ink;
    if let Some(c) = fill {
        painter.circle_filled(pos, 5.0, c);
    }
    painter.circle_stroke(pos, 5.5, Stroke::new(1.6, ink));
    for d in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y] {
        painter.line_segment([pos + d * 7.0, pos + d * 10.0], Stroke::new(1.4, ink));
    }
}

/// One legend entry's mark.
#[derive(Clone, Copy)]
enum Key {
    /// A band or limit line's colour.
    Swatch(Color32),
    Dot(Dot),
    Head,
}

fn legend_item(ui: &mut egui::Ui, pal: &Palette, key: Key, text: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let side = if matches!(key, Key::Head) { 20.0 } else { 8.0 };
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(side), Sense::hover());
        let painter = ui.painter_at(rect.expand(1.0));
        match key {
            Key::Swatch(colour) => {
                painter.circle_filled(rect.center(), 3.5, colour);
            }
            Key::Dot(dot) => dot.draw(&painter, pal, rect.center()),
            Key::Head => draw_head(&painter, pal, rect.center(), None),
        }
        ui.label(egui::RichText::new(text).small().weak());
    });
}

/// Draw the 2D coupled ROM chart and return a frame number if the user clicked a point on the path.
fn draw_chart(
    ui: &mut egui::Ui,
    pal: &Palette,
    spec: &PairSpec,
    tag: &str,
    report: Option<&Value>,
    impossible: &BTreeSet<i64>,
    playhead: (f64, f64, f64, f64, bool),
) -> Option<i64> {
    let width = ui.available_width().max(120.0);
    let height = 185.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());
    let response = response.tag(tag, &spec.label);
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
            let gone = impossible.contains(&f);
            let dot = if gone {
                Dot::Impossible
            } else if past_hard > 1.0 {
                Dot::PastClinical
            } else if past_typ > 1.0 {
                Dot::PastTypical
            } else {
                Dot::Within
            };
            dot.draw(&painter, pal, pos);
            if let Some(ptr) = pointer {
                let d = ptr.distance(pos);
                if d < best_dist {
                    best_dist = d;
                    let x = pt["x"].as_f64().unwrap_or(0.0);
                    let y = pt["y"].as_f64().unwrap_or(0.0);
                    let h_lo = pt["hard_lo"].as_f64().unwrap_or(0.0);
                    let h_hi = pt["hard_hi"].as_f64().unwrap_or(0.0);
                    hover_tip = Some(if gone {
                        format!(
                            "Frame {f}: ({x:.1}°, {y:.1}°)\nImpossible: a joint is past its possible limit (see the list above)\nClick to go to frame {f}"
                        )
                    } else if past_hard > 1.0 {
                        format!(
                            "Frame {f}: ({x:.1}°, {y:.1}°)\n+{past_hard:.1}° past the clinical limit [{h_lo:.0}°, {h_hi:.0}°]: unusual or athletic, still realistic\nClick to go to frame {f}"
                        )
                    } else if past_typ > 1.0 {
                        format!(
                            "Frame {f}: ({x:.1}°, {y:.1}°)\n+{past_typ:.1}° past typical mocap, inside the clinical limit [{h_lo:.0}°, {h_hi:.0}°]\nClick to go to frame {f}"
                        )
                    } else {
                        format!(
                            "Frame {f}: ({x:.1}°, {y:.1}°)\nWithin limits (clinical [{h_lo:.0}°, {h_hi:.0}°])\nClick to go to frame {f}"
                        )
                    });
                    if response.clicked() || response.dragged() {
                        clicked_frame = Some(f);
                    }
                }
            }
        }
    }

    // 7. Playhead
    let (cur_x, cur_y, cur_past_hard, cur_past_typ, cur_impossible) = playhead;
    let head_fill = if cur_impossible {
        Some(pal.error)
    } else if cur_past_hard > 1.0 || cur_past_typ > 1.0 {
        Some(pal.warning)
    } else {
        None
    };
    draw_head(&painter, pal, to_pos(cur_x, cur_y), head_fill);

    // 8. Axis labels
    let font = egui::FontId::proportional(10.5);
    painter.text(
        Pos2::new(plot.left() + 4.0, plot.top() + 2.0),
        egui::Align2::LEFT_TOP,
        format!("{} ({y_max:.0}°)", spec.y.label),
        font.clone(),
        pal.surface.ink,
    );
    // A narrow chart (both sides shown) drops the span rather than cut the name off.
    let x_text = format!("{} ({x_min:.0}°..{x_max:.0}°)", spec.x.label);
    let fits = painter
        .layout_no_wrap(x_text.clone(), font.clone(), pal.surface.ink)
        .size()
        .x
        <= plot.width() - 8.0;
    painter.text(
        Pos2::new(plot.right() - 4.0, plot.bottom() - 2.0),
        egui::Align2::RIGHT_BOTTOM,
        if fits { x_text } else { spec.x.label.clone() },
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
