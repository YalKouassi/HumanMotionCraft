//! The review: `clip.critique`'s findings, worst first, each with where it is and the fix that was
//! tried on a copy and helped. It runs when asked (it tries fixes, so it is not free) and its answer
//! is dropped when the animation changes. Findings are marks on the timeline (`timeline::marks`,
//! source `"review"`), in the physics panel's colours.

use std::collections::BTreeMap;

use ac_core::clip::Clip;
use serde_json::{Value, json};

use crate::app::App;
use crate::app::timeline::marks::Mark;
use crate::app::ui_ids::Tag;

/// Most findings asked for.
const LIMIT: usize = 50;

/// The panel's window state: the last review, what it was made for and what the last fix did.
#[derive(Default)]
pub struct ReviewState {
    report: Option<Value>,
    made_for: Option<(Clip, BTreeMap<String, Clip>)>,
    last_fix: Option<String>,
    /// A new answer came: bring the section into view.
    scroll: bool,
}

impl ReviewState {
    /// How many findings the review shows.
    pub fn count(&self) -> usize {
        self.findings().len()
    }

    fn findings(&self) -> &[Value] {
        self.report
            .as_ref()
            .and_then(|r| r["findings"].as_array())
            .map_or(&[], Vec::as_slice)
    }
}

/// A finding's own number as it reads in the report, with its unit.
fn amount(report: &Value, finding: &Value) -> Option<(f64, &'static str)> {
    let m = &report["metrics"];
    let max = |k: &str| m[k]["max_cm"].as_f64().unwrap_or(0.0);
    match finding["kind"].as_str()? {
        "sliding" => Some((
            m["sliding"]
                .as_array()?
                .iter()
                .filter_map(|s| s["slide_cm"].as_f64())
                .sum(),
            "cm of slide",
        )),
        "penetration" => Some((max("penetration"), "cm into the ground")),
        "balance" => Some((max("balance"), "cm off balance")),
        "capture" => Some((max("capture"), "cm to the side of the feet")),
        "pop" => {
            let at = &finding["frames"][0];
            let score = report["findings"]
                .as_array()?
                .iter()
                .find(|f| f["kind"] == "pop" && f["frames"][0] == *at)
                .and_then(|f| f["score"].as_f64())
                .unwrap_or(0.0);
            // a pop's score is its change of speed over the 3 m/s high line
            Some((score * 3.0, "m/s of pop"))
        }
        _ => None,
    }
}

impl App {
    /// Run the review, keep its answer and mark the timeline.
    pub(in crate::app) fn review_run(&mut self) {
        if let Some(report) = self.run("clip.critique", json!({ "limit": LIMIT })) {
            self.review_show(report);
        }
    }

    /// Keep `clip.critique`'s answer, whichever way it was asked for, and mark the timeline.
    pub(in crate::app) fn review_show(&mut self, report: Value) {
        let pal = self.palette();
        let marks = report["findings"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|f| {
                let kind = f["kind"].as_str().unwrap_or_default();
                let family = match kind {
                    "balance" | "capture" => "unbalanced",
                    other => other,
                };
                let from = f["frames"][0].as_f64().unwrap_or(0.0);
                let to = f["frames"][1].as_f64().unwrap_or(from);
                Mark::new(
                    from,
                    &format!("review-{kind}"),
                    super::physics::colour(pal, family),
                    f["message"].as_str().unwrap_or_default(),
                )
                .until(to)
            })
            .collect();
        self.timeline.marks.set("review", marks);
        let doc = &self.session.document;
        let made_for = (doc.clip.clone(), doc.links.clone());
        let st = &mut self.panels.review;
        st.made_for = Some(made_for);
        st.report = Some(report);
        st.scroll = true;
    }

    /// Forget a review the animation has moved on from (not in the middle of a drag).
    fn review_drop_stale(&mut self) {
        let doc = &self.session.document;
        let stale = self
            .panels
            .review
            .made_for
            .as_ref()
            .is_some_and(|(c, l)| *c != doc.clip || *l != doc.links);
        if stale && !self.session.in_gesture() {
            let st = &mut self.panels.review;
            st.report = None;
            st.made_for = None;
            st.last_fix = None;
            self.timeline.marks.clear("review");
        }
    }

    /// Run a finding's fix as one undo step, review again and say what it did.
    fn review_fix(&mut self, finding: &Value) {
        let before = self
            .panels
            .review
            .report
            .as_ref()
            .and_then(|r| amount(r, finding));
        let kind = finding["kind"].as_str().unwrap_or("finding");
        let params = json!({ "label": format!("Fix: {kind}"), "commands": finding["batch"] });
        if self.run("edit.batch", params).is_none() {
            return;
        }
        self.review_run();
        let after = self
            .panels
            .review
            .report
            .as_ref()
            .and_then(|r| amount(r, finding));
        self.panels.review.last_fix = match (before, after) {
            (Some((b, unit)), Some((a, _))) => Some(format!(
                "Fixed {kind}: {b:.1} to {a:.1} {unit}. Ctrl+Z puts it back."
            )),
            _ => Some(format!("Fixed {kind}. Ctrl+Z puts it back.")),
        };
    }

    pub(in crate::app) fn review_panel(&mut self, ui: &mut egui::Ui) {
        self.review_drop_stale();
        let asked = self.panels.review.report.is_some();
        let button = ui
            .button(if asked { "Review again" } else { "Review animation" })
            .on_hover_text("Check the motion for sliding feet, the ground, balance, joint range and pops")
            .tag("panel.review.run", "Review animation");
        if std::mem::take(&mut self.panels.review.scroll) {
            button.scroll_to_me(Some(egui::Align::TOP));
        }
        if button.clicked() {
            self.review_run();
        }
        let pal = self.palette();
        if let Some(text) = &self.panels.review.last_fix {
            ui.label(egui::RichText::new(text).small().strong())
                .tag("panel.review.result", text);
        }
        let Some(report) = self.panels.review.report.clone() else {
            ui.label(
                egui::RichText::new("Nothing reviewed since the animation last changed.")
                    .small()
                    .weak(),
            );
            return;
        };
        let metrics = &report["metrics"];
        let span = format!(
            "frames {}–{}",
            metrics["from"].as_i64().unwrap_or(0),
            metrics["to"].as_i64().unwrap_or(0)
        );
        let findings = self.panels.review.findings().to_vec();
        if findings.is_empty() {
            let text =
                format!("Nothing found on {span}: feet, ground, balance, joint range and pops all pass.");
            ui.label(egui::RichText::new(&text).small().color(pal.surface.ink))
                .tag("panel.review.clean", &text);
        } else {
            let summary = format!("{} found on {span}, worst first.", findings.len());
            ui.label(egui::RichText::new(&summary).small().weak())
                .tag("panel.review.summary", &summary);
        }
        let mut go_to = None;
        let mut fix = None;
        for (i, f) in findings.iter().enumerate() {
            let severity = f["severity"].as_str().unwrap_or("low");
            let colour = match severity {
                "high" => pal.error,
                "medium" => pal.warning,
                _ => pal.surface.ink,
            };
            ui.separator();
            let message = f["message"].as_str().unwrap_or_default();
            ui.label(
                egui::RichText::new(format!("{severity}: {message}"))
                    .small()
                    .color(colour),
            )
            .tag(format!("panel.review.finding.{i}"), message);
            let from = f["frames"][0].as_i64().unwrap_or(0);
            ui.horizontal(|ui| {
                if ui
                    .small_button("Go to problem")
                    .on_hover_text(format!("Go to frame {from}"))
                    .tag(format!("panel.review.goto.{i}"), "Go to problem")
                    .clicked()
                {
                    go_to = Some(from);
                }
                if f["batch"].as_array().is_some_and(|b| !b.is_empty())
                    && ui
                        .small_button("Fix")
                        .on_hover_text(format!(
                            "Run {} as one undo step: it helped on a copy",
                            f["command"].as_str().unwrap_or("the fix")
                        ))
                        .tag(format!("panel.review.fix.{i}"), "Fix")
                        .clicked()
                {
                    fix = Some(f.clone());
                }
            });
        }
        for note in metrics["notes"].as_array().into_iter().flatten() {
            ui.label(
                egui::RichText::new(format!("Not checked: {}", note.as_str().unwrap_or_default()))
                    .small()
                    .weak(),
            );
        }
        if let Some(frame) = go_to {
            self.playing = false;
            self.run("frame.set", json!({ "frame": frame }));
        }
        if let Some(f) = fix {
            self.review_fix(&f);
        }
    }
}
