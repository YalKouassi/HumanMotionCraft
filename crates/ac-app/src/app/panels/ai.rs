//! The AI in-between panel: the gap around the playhead measured before anything is clicked (and
//! Preview / Fill refused with the reason when the model can't take it), Fill all gaps, the
//! preview worked out off the UI thread (spinner, time taken, cancel; superseded while sliders
//! move), keep / compare / styles, and Shift+A to switch the span between AI and plain.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::ai_worker::{self, Worker};
use crate::app::App;
use crate::app::timeline::marks::Mark;
use crate::app::ui_ids::Tag;

/// What the last fill said might look wrong (its `diagnostics`, one entry per span filled),
/// kept while the animation is as the fill left it: the panel lists it and the timeline marks
/// each flag (source `"ai-fill"`). A new fill replaces it; any other change drops it.
pub struct FillReport {
    pub spans: Vec<Value>,
    made_for: (ac_core::clip::Clip, BTreeMap<String, ac_core::clip::Clip>),
}

/// A changed AI setting asks for a new preview at most this often while a slider is dragged
/// (each request copies the document; the worker drops the ones it has not started).
const REPREVIEW: Duration = Duration::from_millis(40);

/// The panel's window state.
#[derive(Default)]
pub struct AiState {
    /// The worker thread (started on first use; `Err`: it could not start, previews run here).
    worker: Option<Result<Worker, String>>,
    /// How long the last preview took to work out.
    last_ms: Option<u128>,
    /// Why the last preview failed, shown under the buttons.
    error: Option<String>,
    /// Fill with the pose-to-pose model (roadmap 163000), once it's installed; else our own.
    pub pose_fill: bool,
    /// Footsteps: the left foot steps first (else the right).
    pub steps_left: bool,
    /// Footsteps: how many (0: as many as suit the distance and time).
    pub steps: u32,
    /// Footsteps: step keys the model fills (else the built-in Walk loop, a key every 2 frames).
    pub steps_ai: bool,
    /// The last fill's diagnostics, while the animation is as it left it.
    pub report: Option<FillReport>,
}

impl App {
    /// A fill's result (`ai.fill`, `ai.fill_all`, or `ai.toggle` to the model): keep what it
    /// measured and mark every flagged frame on the timeline, with the reason as the mark's text.
    /// Any other command's result is left alone.
    pub(in crate::app) fn fill_marks(&mut self, id: &str, r: &Value) {
        let spans: Vec<Value> = match id {
            "ai.fill" => vec![r["diagnostics"].clone()],
            "ai.toggle" if r["mode"] == "ai" => vec![r["diagnostics"].clone()],
            "ai.fill_all" => r["diagnostics"].as_array().cloned().unwrap_or_default(),
            _ => return,
        };
        let pal = self.palette();
        let marks: Vec<Mark> = spans
            .iter()
            .flat_map(|d| d["flags"].as_array().into_iter().flatten())
            .map(|f| {
                let kind = f["kind"].as_str().unwrap_or_default();
                let family = match kind {
                    "sliding" => "sliding",
                    "bent" => "penetration",
                    "path" => "unbalanced",
                    _ => "other",
                };
                let from = f["from"].as_f64().unwrap_or(0.0);
                let to = f["to"].as_f64().unwrap_or(from);
                let m = Mark::new(
                    from,
                    &format!("ai-fill-{kind}"),
                    super::physics::colour(pal, family),
                    f["text"].as_str().unwrap_or_default(),
                );
                if to > from { m.until(to) } else { m }
            })
            .collect();
        self.timeline.marks.set("ai-fill", marks);
        let doc = &self.session.document;
        self.panels.ai.report = Some(FillReport {
            spans,
            made_for: (doc.clip.clone(), doc.links.clone()),
        });
    }

    /// Forget a fill's diagnostics once the animation has moved on from it (not mid-drag): they
    /// measured the keys as the fill left them.
    pub(in crate::app) fn fill_marks_drop_stale(&mut self) {
        let doc = &self.session.document;
        let stale = self
            .panels
            .ai
            .report
            .as_ref()
            .is_some_and(|r| r.made_for.0 != doc.clip || r.made_for.1 != doc.links);
        if stale && !self.session.in_gesture() {
            self.panels.ai.report = None;
            self.timeline.marks.clear("ai-fill");
        }
    }

    /// What the last fill measured, per span: the numbers in one line, each flag (a click goes
    /// to its frame), and what could not be measured.
    fn fill_report(&mut self, ui: &mut egui::Ui) {
        let Some(rep) = self.panels.ai.report.as_ref() else {
            return;
        };
        let spans = rep.spans.clone();
        let pal = self.palette();
        for (n, d) in spans.iter().enumerate() {
            let (a, b) = (d["from"].as_i64().unwrap_or(0), d["to"].as_i64().unwrap_or(0));
            let flags = d["flags"].as_array().cloned().unwrap_or_default();
            let seams: Vec<String> = d["seams"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|s| match (s["dv"].as_f64(), s["dw"].as_f64()) {
                    (Some(dv), Some(dw)) => format!("key {}: {dv:.2} m/s, {dw:.2} rad/s", s["frame"]),
                    (Some(dv), None) => format!("key {}: {dv:.2} m/s", s["frame"]),
                    _ => format!("key {}: not measurable", s["frame"]),
                })
                .collect();
            ui.label(
                egui::RichText::new(format!(
                    "Fill {}–{}: {}. Slide {:.1} cm, root {:.2} m off the keys' path, {}, {} joint{} bent back.",
                    a + 1,
                    b - 1,
                    match flags.len() {
                        0 => "nothing past a line".to_string(),
                        1 => "1 thing to look at".to_string(),
                        k => format!("{k} things to look at"),
                    },
                    d["sliding_max_cm"].as_f64().unwrap_or(0.0),
                    d["path"]["max_m"].as_f64().unwrap_or(0.0),
                    seams.join(", "),
                    d["bent"].as_array().map_or(0, Vec::len),
                    if d["bent"].as_array().map_or(0, Vec::len) == 1 { "" } else { "s" },
                ))
                .small(),
            );
            for (i, f) in flags.iter().enumerate() {
                let kind = f["kind"].as_str().unwrap_or_default();
                let family = match kind {
                    "sliding" => "sliding",
                    "bent" => "penetration",
                    "path" => "unbalanced",
                    _ => "other",
                };
                let text = f["text"].as_str().unwrap_or_default();
                let frame = f["from"].as_i64().unwrap_or(0);
                if ui
                    .add(
                        egui::Label::new(
                            egui::RichText::new(text)
                                .small()
                                .color(super::physics::colour(pal, family)),
                        )
                        .sense(egui::Sense::click()),
                    )
                    .on_hover_text(format!("Go to frame {frame}"))
                    .tag(format!("panel.ai.flag.{n}.{i}"), text)
                    .clicked()
                {
                    self.run("frame.set", json!({ "frame": frame }));
                }
            }
            for note in d["notes"].as_array().into_iter().flatten() {
                ui.label(
                    egui::RichText::new(note.as_str().unwrap_or_default())
                        .small()
                        .weak(),
                );
            }
        }
    }
    /// Collect a finished preview and show it (runs every frame, panel open or not).
    pub(in crate::app) fn ai_poll(&mut self) {
        let Some(Ok(w)) = self.panels.ai.worker.as_mut() else {
            return;
        };
        let Some(done) = w.poll() else { return };
        self.panels.ai.last_ms = Some(done.took.as_millis());
        match done.result {
            Ok(p) if ai_worker::fits(&self.session.document, &p) => {
                let params =
                    json!({ "from": p.from, "to": p.to, "mode": if p.plain { "plain" } else { "ai" } });
                self.session.document.ai_precomputed = Some(p);
                self.panels.ai.error = None;
                if let Some(r) = self.run("ai.preview", params) {
                    self.status = format!(
                        "preview {}–{}: {}",
                        r["from"].as_i64().unwrap_or(0) + 1,
                        r["to"].as_i64().unwrap_or(0) - 1,
                        r["model"].as_str().unwrap_or("")
                    );
                }
                self.ai_previewed = Some(Instant::now());
            }
            // the animation or the settings changed while it was worked out: a newer request
            // follows a settings change; an edit shows "changed since the preview"
            Ok(_) => {}
            Err(e) => {
                self.status = format!("AI preview: {e}");
                self.panels.ai.error = Some(e);
            }
        }
    }

    /// `ai.toggle` (Shift+A, the menu, the palette): switch the span around the playhead between
    /// the model's fill and plain interpolation, and say which way it went.
    pub(in crate::app) fn ai_toggle(&mut self) {
        if let Some(r) = self.run("ai.toggle", json!({})) {
            let (a, b) = (r["from"].as_i64().unwrap_or(0), r["to"].as_i64().unwrap_or(0));
            self.status = match r["mode"].as_str() {
                Some("ai") => format!(
                    "AI fill {}–{} ({})",
                    a + 1,
                    b - 1,
                    r["model"].as_str().unwrap_or("")
                ),
                _ => format!("plain interpolation {}–{}", a + 1, b - 1),
            };
        }
    }

    /// Ask for a preview (`span`: these keys, else the ones around the playhead) on the worker;
    /// without one, work it out here and say so.
    pub(in crate::app) fn preview_fill(
        &mut self,
        span: Option<(i64, i64)>,
        plain: bool,
        ctx: &egui::Context,
    ) {
        if self.panels.ai.worker.is_none() {
            let c = ctx.clone();
            self.panels.ai.worker = Some(Worker::start(move || c.request_repaint()));
        }
        self.ai_previewed = Some(Instant::now());
        match self.panels.ai.worker.as_mut() {
            Some(Ok(w)) => w.submit(self.session.document.clone(), span, plain),
            Some(Err(e)) => {
                let note = format!("{e}; working the preview out here (the window waits)");
                let mode = if plain { "plain" } else { "ai" };
                let params = match span {
                    Some((a, b)) => json!({ "from": a, "to": b, "mode": mode }),
                    None => json!({ "mode": mode }),
                };
                if self.run("ai.preview", params).is_some() {
                    self.status = note;
                }
            }
            None => {}
        }
    }

    fn ai_pending(&self) -> Option<ai_worker::Pending> {
        match &self.panels.ai.worker {
            Some(Ok(w)) => w.pending,
            _ => None,
        }
    }

    /// The learned in-betweener: preview a fill as a ghost, compare it with plain interpolation,
    /// try styles, keep it; fill one gap or all of them.
    pub(in crate::app) fn ai_panel(&mut self, ui: &mut egui::Ui) {
        let Some(info) = self.session.document.ai.as_ref().map(|m| m.info().clone()) else {
            ui.label("No model in assets/models: AI fill is off (keys use plain interpolation).");
            return;
        };
        let ctx = ui.ctx().clone();
        self.fill_with_row(ui);
        let pose = self.panels.ai.pose_fill;
        let doc = &self.session.document;
        let gap = ac_core::gaps::gap_at(doc, doc.key_frame());
        let all = ac_core::gaps::gaps(doc);
        let max = ac_core::gaps::max_gap(doc).unwrap_or(0);
        // why Preview and Fill can't run here (None: they can)
        let refusal: Option<String> = match &gap {
            None => Some("put the playhead between two of your keys".into()),
            Some(_) if pose => ac_posefill::refusal(doc),
            Some(g) => g.refused.clone(),
        };
        match &gap {
            Some(g) => {
                let text = if pose {
                    format!("Gap {}–{}: {} frames", g.from, g.to, g.frames)
                } else {
                    format!("Gap {}–{}: {} frames (max {max})", g.from, g.to, g.frames)
                };
                let text = match g.anchors {
                    Some(n) => format!(
                        "{text}, split by {n} anchor pose{} from the library",
                        if n == 1 { "" } else { "s" }
                    ),
                    None => text,
                };
                let filled = if g.filled { ", filled by the model" } else { "" };
                ui.label(
                    egui::RichText::new(format!("{text}{filled}")).color(if refusal.is_some() {
                        self.palette().error
                    } else {
                        ui.visuals().text_color()
                    }),
                );
            }
            None => {
                ui.label(egui::RichText::new("The playhead is not between two of your keys.").weak());
            }
        }
        if let (Some(why), Some(_)) = (&refusal, &gap) {
            ui.label(
                egui::RichText::new(format!("Preview and Fill are off: {why}"))
                    .small()
                    .color(self.palette().error),
            );
        }
        let pending = self.ai_pending();
        ui.horizontal_wrapped(|ui| {
            let can = refusal.is_none();
            let why = refusal.clone().unwrap_or_default();
            if ui
                .add_enabled(can, egui::Button::new("Preview fill"))
                .on_hover_text(format!(
                    "Show {}'s fill between your keys around the playhead as a green ghost, without changing the animation",
                    info.tag
                ))
                .on_disabled_hover_text(&why)
                .tag("panel.ai.preview", "Preview fill")
                .clicked()
            {
                if pose {
                    self.ai_previewed = Some(Instant::now());
                    if let Some(out) = self.run("posefill.preview", json!({})) {
                        self.posefill_done("Preview fill", &out);
                    }
                } else {
                    self.preview_fill(None, false, &ctx);
                }
            }
            if ui
                .add_enabled(can, egui::Button::new("Fill now"))
                .on_hover_text(format!("Fill between your keys around the playhead (1..{max} frames)"))
                .on_disabled_hover_text(&why)
                .tag("panel.ai.fill", "Fill now")
                .clicked()
                && let Some(r) = self.run(if pose { "posefill.fill" } else { "ai.fill" }, json!({}))
            {
                self.status = format!(
                    "AI filled {}–{} ({})",
                    r["from"].as_i64().unwrap_or(0) + 1,
                    r["to"].as_i64().unwrap_or(0) - 1,
                    r["model"].as_str().unwrap_or("")
                );
            }
        });
        self.fill_all_row(ui, &all);
        self.fill_report(ui);
        self.steps_row(ui, gap.as_ref().map(|g| (g.from, g.to)));
        if let Some(p) = pending {
            ui.horizontal_wrapped(|ui| {
                ui.spinner();
                ui.label(format!(
                    "working out the {}... {} ms",
                    if p.plain { "plain interpolation" } else { "fill" },
                    p.since.elapsed().as_millis()
                ));
                if ui
                    .small_button("Cancel")
                    .tag("panel.ai.cancel", "Cancel")
                    .clicked()
                    && let Some(Ok(w)) = self.panels.ai.worker.as_mut()
                {
                    w.cancel();
                    self.status = "AI preview cancelled".into();
                }
            });
            ctx.request_repaint_after(Duration::from_millis(50));
        } else if let Some(ms) = self.panels.ai.last_ms {
            ui.label(
                egui::RichText::new(format!("last preview worked out in {ms} ms"))
                    .small()
                    .weak(),
            );
        }
        if let Some(e) = &self.panels.ai.error {
            ui.label(egui::RichText::new(e).small().color(self.palette().error));
        }
        self.preview_controls(ui, &info, &ctx);
        self.ai_settings_controls(ui, &info, &ctx);
        ui.label(
            egui::RichText::new("Shift+A switches the span at the playhead between AI and plain.")
                .small()
                .weak(),
        );
    }

    /// Which model fills: shown once the pose-to-pose one is installed; ours is the default, and a
    /// gap the one chosen can't take says why rather than going to the other.
    fn fill_with_row(&mut self, ui: &mut egui::Ui) {
        if self.panels.posefill.helper.is_none() {
            self.panels.posefill.helper = self.session.run("posefill.helper", &json!({})).ok();
        }
        let installed = self
            .panels
            .posefill
            .helper
            .as_ref()
            .is_some_and(|h| h["installed"] == true);
        if !installed {
            self.panels.ai.pose_fill = false;
            return;
        }
        let st = &mut self.panels.ai;
        ui.horizontal(|ui| {
            ui.label("Fill with");
            ui.radio_value(&mut st.pose_fill, false, "Our model")
                .tag("panel.ai.engine.ours", "Our model");
            ui.radio_value(&mut st.pose_fill, true, "Pose fill")
                .on_hover_text("NVIDIA's pose-to-pose model, on your GPU: gaps of 0.6 to 1.9 s a call")
                .tag("panel.ai.engine.pose", "Pose fill");
        });
        if self.panels.ai.pose_fill {
            super::posefill::notice(ui);
        }
    }

    /// "Fill all gaps": every gap the model can take, one undo; the ones it can't are counted.
    fn fill_all_row(&mut self, ui: &mut egui::Ui, all: &[ac_core::gaps::Gap]) {
        let pose = self.panels.ai.pose_fill;
        let doc = &self.session.document;
        let open = all
            .iter()
            .filter(|g| !g.filled)
            .filter(|g| {
                if pose {
                    ac_posefill::takes(doc, g.from, g.to)
                } else {
                    g.refused.is_none()
                }
            })
            .count();
        let too_long = if pose {
            0
        } else {
            all.iter().filter(|g| g.refused.is_some() && g.frames > 0).count()
        };
        ui.horizontal_wrapped(|ui| {
            let why = if all.is_empty() {
                "needs at least two keys of yours".to_string()
            } else {
                "every gap the model can take is filled already".to_string()
            };
            if ui
                .add_enabled(open > 0, egui::Button::new(format!("Fill all gaps ({open})")))
                .on_hover_text("Fill every empty gap between your keys with the model (one undo)")
                .on_disabled_hover_text(why)
                .tag("panel.ai.fill_all", format!("Fill all gaps ({open})"))
                .clicked()
                && let Some(r) = self.run(if pose { "posefill.fill_all" } else { "ai.fill_all" }, json!({}))
            {
                let n = r["filled"].as_array().map_or(0, Vec::len);
                let skipped: Vec<String> = r["skipped"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|s| s["reason"] != "filled already")
                    .map(|s| format!("{}–{} {}", s["from"], s["to"], s["reason"].as_str().unwrap_or("")))
                    .collect();
                self.status = if skipped.is_empty() {
                    format!("AI filled {n} gaps")
                } else {
                    format!("AI filled {n} gaps; left to the spline: {}", skipped.join(", "))
                };
            }
            if too_long > 0 {
                ui.label(
                    egui::RichText::new(format!("{too_long} too long for the model"))
                        .small()
                        .color(self.palette().error),
                );
            }
        });
    }

    /// Footsteps for the gap at the playhead: which foot leads, how many steps and how it walks,
    /// then `anim.steps`.
    fn steps_row(&mut self, ui: &mut egui::Ui, gap: Option<(i64, i64)>) {
        ui.horizontal_wrapped(|ui| {
            let r = ui
                .add_enabled(gap.is_some(), egui::Button::new("Footsteps"))
                .on_hover_text(
                    "Walk between your keys around the playhead, each foot planted where it lands. Put the end key where the walk should end first",
                )
                .on_disabled_hover_text("put the playhead between two of your keys")
                .tag("panel.ai.steps", "Footsteps");
            let st = &mut self.panels.ai;
            egui::ComboBox::from_id_salt("panel.ai.steps_lead")
                .selected_text(if st.steps_left { "left foot first" } else { "right foot first" })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut st.steps_left, false, "right foot first")
                        .tag("panel.ai.steps_lead.R", "right foot first");
                    ui.selectable_value(&mut st.steps_left, true, "left foot first")
                        .tag("panel.ai.steps_lead.L", "left foot first");
                })
                .response
                .tag("panel.ai.steps_lead", "First foot");
            ui.add(
                egui::DragValue::new(&mut st.steps)
                    .range(0..=64)
                    .custom_formatter(|n, _| if n < 2.0 { "auto".into() } else { format!("{n} steps") }),
            )
            .on_hover_text("How many steps (auto: between one every 0.55 m and one every half second)")
            .tag("panel.ai.steps_count", "Steps");
            egui::ComboBox::from_id_salt("panel.ai.steps_fill")
                .selected_text(if st.steps_ai { "AI walk" } else { "Walk loop" })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut st.steps_ai, false, "Walk loop")
                        .on_hover_text("Moves like the built-in Walk, a key every 2 frames")
                        .tag("panel.ai.steps_fill.loop", "Walk loop");
                    ui.selectable_value(&mut st.steps_ai, true, "AI walk")
                        .on_hover_text("A heel-down key for each step, filled by the model in the walk style")
                        .tag("panel.ai.steps_fill.ai", "AI walk");
                })
                .response
                .on_hover_text("How it walks")
                .tag("panel.ai.steps_fill", "Walk with");
            if r.clicked()
                && let Some((a, b)) = gap
            {
                let (left, n) = (st.steps_left, st.steps);
                self.footsteps(a, b, left, (n >= 2).then_some(n));
            }
        });
    }

    /// Run `anim.steps` on `a`..`b` and say what it did.
    pub(in crate::app) fn footsteps(&mut self, a: i64, b: i64, left: bool, steps: Option<u32>) {
        let fill = if self.panels.ai.steps_ai { "ai" } else { "cycle" };
        let mut p = json!({ "from": a, "to": b, "lead": if left { "L" } else { "R" }, "fill": fill });
        if let Some(n) = steps {
            p["steps"] = json!(n);
        }
        if let Some(r) = self.run("anim.steps", p) {
            let notes: Vec<&str> = r["notes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|n| n.as_str())
                .collect();
            self.status = format!(
                "footsteps {a}–{b}: {} steps, {:.2} m each, {} foot first, {}{}",
                r["steps"].as_i64().unwrap_or(0),
                r["stride"].as_f64().unwrap_or(0.0),
                if left { "left" } else { "right" },
                if fill == "ai" { "AI walk" } else { "Walk loop" },
                if notes.is_empty() {
                    String::new()
                } else {
                    format!("; {}", notes.join("; "))
                }
            );
        }
    }

    /// Keep / try another style / compare plain / discard, for a live preview.
    fn preview_controls(&mut self, ui: &mut egui::Ui, info: &ac_core::ai::ModelInfo, ctx: &egui::Context) {
        let Some(p) = self.session.document.ai_preview.clone() else {
            return;
        };
        let span = format!("{}–{}", p.from + 1, p.to - 1);
        if self.session.document.live_preview().is_none() {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new("The animation changed since the preview.")
                        .small()
                        .weak(),
                );
                if ui
                    .small_button("Preview again")
                    .tag("panel.ai.preview_again", "Preview again")
                    .clicked()
                {
                    if p.label == ac_posefill::PREVIEW_LABEL {
                        self.run("posefill.preview", json!({ "from": p.from, "to": p.to }));
                    } else {
                        self.preview_fill(Some((p.from, p.to)), p.plain, ctx);
                    }
                }
            });
            return;
        }
        ui.label(
            egui::RichText::new(format!("Previewing {span}: {}", p.label)).color(self.palette().preview),
        );
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Keep")
                .on_hover_text("Make the previewed frames AI keys (one undo)")
                .tag("panel.ai.keep", "Keep")
                .clicked()
                && self.run("ai.accept", json!({})).is_some()
            {
                self.status = format!("kept the AI fill {span}");
            }
            if !info.styles.is_empty()
                && ui
                    .button("Try another style")
                    .on_hover_text("The next style the model knows")
                    .tag("panel.ai.next_style", "Try another style")
                    .clicked()
            {
                let st = &self.session.document.ai_settings.style;
                let next = info
                    .styles
                    .iter()
                    .position(|s| s == st)
                    .map_or(0, |i| (i + 1) % info.styles.len());
                if let Some(style) = info.styles.get(next).cloned()
                    && self.run("ai.settings", json!({ "style": style })).is_some()
                {
                    self.preview_fill(Some((p.from, p.to)), false, ctx);
                }
            }
            let other = if p.plain { "Show AI" } else { "Compare plain" };
            if ui
                .button(other)
                .on_hover_text("Switch the ghost between the model's fill and plain interpolation")
                .tag("panel.ai.compare", other)
                .clicked()
            {
                self.preview_fill(Some((p.from, p.to)), !p.plain, ctx);
            }
            if ui.button("Discard").tag("panel.ai.discard", "Discard").clicked() {
                if let Some(Ok(w)) = self.panels.ai.worker.as_mut() {
                    w.cancel();
                }
                self.run("ai.discard", json!({}));
            }
        });
    }

    /// Style, forces and holds; a live preview follows them (on the worker, newest wins).
    fn ai_settings_controls(
        &mut self,
        ui: &mut egui::Ui,
        info: &ac_core::ai::ModelInfo,
        ctx: &egui::Context,
    ) {
        let mut st = self.session.document.ai_settings.clone();
        let mut preset_chosen = None;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Preset:").weak());
            if ui
                .button("Preserve motion")
                .tag("panel.ai.preset.preserve", "Preserve motion")
                .clicked()
            {
                preset_chosen = Some("preserve");
            }
            if ui
                .button("Natural")
                .tag("panel.ai.preset.natural", "Natural")
                .clicked()
            {
                preset_chosen = Some("natural");
            }
            if ui
                .button("Follow style")
                .tag("panel.ai.preset.follow_style", "Follow style")
                .clicked()
            {
                preset_chosen = Some("follow_style");
            }
        });
        if let Some(p) = preset_chosen {
            self.run("ai.settings", json!({ "preset": p }));
            if self.session.document.live_preview().is_some() {
                self.ai_dirty = true;
            }
            // the sliders below show the preset's values this same frame
            st = self.session.document.ai_settings.clone();
        }
        let mut changed = false;
        if !info.styles.is_empty() {
            // the model's styles ("any" first): a hundred or more with 100STYLE, so the list scrolls
            egui::ComboBox::from_label("Style")
                .selected_text(ac_core::ai::style_name(&st.style))
                .height(320.0)
                .show_ui(ui, |ui| {
                    for name in &info.styles {
                        changed |= ui
                            .selectable_value(&mut st.style, name.clone(), ac_core::ai::style_name(name))
                            .tag(format!("panel.ai.style.{name}"), name)
                            .changed();
                    }
                })
                .response
                .tag("panel.ai.style", "Style");
            changed |= ui
                .add(egui::Slider::new(&mut st.style_force, 0.0..=3.0).text("style force"))
                .tag("panel.ai.style_force", "style force")
                .changed();
        }
        changed |= ui
            .add(egui::Slider::new(&mut st.ai_force, 0.0..=1.5).text("AI force"))
            .tag("panel.ai.ai_force", "AI force")
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut st.trajectory, 0.0..=1.0).text("keep path"))
            .tag("panel.ai.keep_path", "keep path")
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut st.head, 0.0..=1.0).text("keep head"))
            .tag("panel.ai.keep_head", "keep head")
            .changed();
        let preview = self.session.document.ai_preview.clone();
        let live = self.session.document.live_preview().is_some();
        if changed {
            self.run(
                "ai.settings",
                json!({ "style": st.style, "style_force": st.style_force, "ai_force": st.ai_force, "trajectory": st.trajectory, "head": st.head }),
            );
            if live {
                self.ai_dirty = true;
            }
        }
        // the preview follows the sliders while they move
        if self.ai_dirty
            && let Some(p) = &preview
        {
            if self.ai_previewed.is_none_or(|t| t.elapsed() >= REPREVIEW) {
                self.preview_fill(Some((p.from, p.to)), p.plain, ctx);
                self.ai_dirty = false;
            } else {
                ctx.request_repaint_after(REPREVIEW);
            }
        }
    }
}
