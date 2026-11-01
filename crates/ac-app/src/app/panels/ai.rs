//! The AI in-between panel: the gap around the playhead measured before anything is clicked (and
//! Preview / Fill refused with the reason when the model can't take it), Fill all gaps, the
//! preview worked out off the UI thread (spinner, time taken, cancel; superseded while sliders
//! move), keep / compare / styles, which model fills (`ai.models`, `ai.model`; the model's own
//! styles stay under it), and Shift+A to switch the span between AI and plain.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::ai_worker::{self, Worker};
use crate::app::App;
use crate::app::shell::Level;
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
    /// Fill with the pose-to-pose model (roadmap 163000), once it's installed; else our own.
    pub pose_fill: bool,
    /// Walking steps rather than AI movement.
    pub walking: bool,
    /// Apply fills every gap, not only the one at the playhead.
    pub every: bool,
    /// Footsteps: the left foot steps first (else the right).
    pub steps_left: bool,
    /// Footsteps: how many (0: as many as suit the distance and time).
    pub steps: u32,
    /// Footsteps: step keys the model fills (else the built-in Walk loop, a key every 2 frames).
    pub steps_ai: bool,
    /// The last fill's diagnostics, while the animation is as it left it.
    pub report: Option<FillReport>,
    /// What `ai.models` said last (the model menu); `None`: ask again at the next frame.
    models: Option<Value>,
    /// The Fill report window is open.
    report_open: bool,
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

    /// The Fill report button: always there, so nothing below it moves when a fill reports.
    fn fill_report_button(&mut self, ui: &mut egui::Ui) {
        let flags = self.panels.ai.report.as_ref().map(|r| {
            r.spans
                .iter()
                .map(|d| d["flags"].as_array().map_or(0, Vec::len))
                .sum::<usize>()
        });
        let label = match flags {
            Some(n) if n > 0 => format!("Report ({n})"),
            _ => "Report".to_string(),
        };
        if ui
            .add_enabled(flags.is_some(), egui::Button::new(&label))
            .on_hover_text("What the last fill measured, and anything to look at")
            .on_disabled_hover_text("Fill a gap first: this says what the fill measured")
            .tag("panel.ai.report", label.as_str())
            .clicked()
        {
            self.panels.ai.report_open = !self.panels.ai.report_open;
        }
    }

    /// What the last fill measured, per span, in a window of its own: the numbers in one line,
    /// each flag (a click goes to its frame), and what could not be measured.
    pub(in crate::app) fn fill_report_window(&mut self, ctx: &egui::Context) {
        let Some(rep) = self
            .panels
            .ai
            .report
            .as_ref()
            .filter(|_| self.panels.ai.report_open)
        else {
            return;
        };
        let spans = rep.spans.clone();
        let pal = self.palette();
        let flagged = spans
            .iter()
            .filter(|d| d["flags"].as_array().is_some_and(|f| !f.is_empty()))
            .count();
        let mut open = true;
        let mut go = None;
        egui::Window::new("Fill report")
            .open(&mut open)
            .default_size([520.0, 380.0])
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{} fills, {flagged} with something to look at · click a line to go to its frame",
                        spans.len()
                    ))
                    .weak()
                    .small(),
                );
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
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
                                .tag(format!("window.fill_report.flag.{n}.{i}"), text)
                                .clicked()
                            {
                                go = Some(frame);
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
                });
            });
        self.panels.ai.report_open = open;
        if let Some(frame) = go {
            self.run("frame.set", json!({ "frame": frame }));
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
                self.shell.messages.push(Level::Error, format!("AI preview: {e}"));
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

    /// Fill between my poses: AI movement (preview as a ghost, compare with no AI, try styles, keep
    /// it; one gap or all of them) or walking steps, which are a separate feature and not AI.
    pub(in crate::app) fn ai_panel(&mut self, ui: &mut egui::Ui) {
        let doc = &self.session.document;
        let gap = ac_core::gaps::gap_at(doc, doc.key_frame());
        super::create::help(ui, "Creates the movement between two poses you've set.");
        let st = &mut self.panels.ai;
        ui.horizontal(|ui| {
            ui.label("Movement");
            ui.radio_value(&mut st.walking, false, "AI")
                .on_hover_text("The AI works out the movement")
                .tag("panel.ai.movement.ai", "AI");
            ui.radio_value(&mut st.walking, true, "Walking steps")
                .on_hover_text("Steps from one pose to the other, each foot planted where it lands")
                .tag("panel.ai.movement.steps", "Walking steps");
        });
        if self.panels.ai.walking {
            if gap.is_none() {
                self.gap_hint(ui);
            }
            self.steps_row(ui, gap.as_ref().map(|g| (g.from, g.to)));
            return;
        }
        let Some(info) = self.session.document.ai.as_ref().map(|m| m.info().clone()) else {
            ui.label("No AI model found in assets/models, so the movement stays plain (no AI).");
            return;
        };
        let ctx = ui.ctx().clone();
        let pose = self.fill_with_row(ui);
        let doc = &self.session.document;
        let all = ac_core::gaps::gaps(doc);
        let every = self.panels.ai.every;
        // why Preview and Apply can't run here (None: they can)
        let refusal: Option<String> = match &gap {
            None => Some("Put the playhead between two of your poses".into()),
            Some(_) if pose => ac_posefill::refusal(doc),
            Some(g) => g.refused.clone(),
        };
        match (&gap, &refusal) {
            (Some(g), None) => {
                let anchors = g.anchors.map_or(String::new(), |n| {
                    format!(
                        ", with {n} library pose{} along the way",
                        if n == 1 { "" } else { "s" }
                    )
                });
                let filled = if g.filled { " (already filled)" } else { "" };
                ui.add(
                    egui::Label::new(format!("Filling frames {} to {}{anchors}{filled}", g.from, g.to))
                        .truncate(),
                );
            }
            (Some(g), Some(why)) => {
                // said in full, with the fix: a greyed button alone explains nothing
                ui.label(
                    egui::RichText::new(format!("Can't fill frames {} to {}: {why}", g.from, g.to))
                        .color(self.palette().error),
                );
            }
            (None, _) => self.gap_hint(ui),
        }
        self.ai_settings_controls(ui, &info, &ctx);
        let (open, too_long) = self.open_gaps(&all, pose);
        let pending = self.ai_pending();
        ui.horizontal_wrapped(|ui| {
            let why = refusal.clone().unwrap_or_default();
            if ui
                .add_enabled(refusal.is_none() && !every, egui::Button::new("Preview"))
                .on_hover_text("Show the movement as a green ghost, without changing the animation")
                .on_disabled_hover_text(if every {
                    "Preview shows one gap at a time: untick Fill every gap".to_string()
                } else {
                    why.clone()
                })
                .tag("panel.ai.preview", "Preview")
                .clicked()
            {
                if pose {
                    self.ai_previewed = Some(Instant::now());
                    if let Some(out) = self.run("posefill.preview", json!({})) {
                        self.posefill_done("Preview", &out);
                    }
                } else {
                    self.preview_fill(None, false, &ctx);
                }
            }
            let can = if every { open > 0 } else { refusal.is_none() };
            let apply = ui
                .add_enabled(can, egui::Button::new("Apply"))
                .on_hover_text(if every {
                    "Fill every empty gap between your poses (one undo)"
                } else {
                    "Fill between your poses around the playhead (one undo)"
                })
                .on_disabled_hover_text(if every {
                    "Every gap the AI can take is filled already".to_string()
                } else {
                    why
                })
                .tag(
                    if every {
                        "panel.ai.fill_all"
                    } else {
                        "panel.ai.fill"
                    },
                    "Apply",
                );
            if apply.clicked() {
                if every {
                    self.fill_all(pose);
                } else if let Some(r) = self.run(if pose { "posefill.fill" } else { "ai.fill" }, json!({})) {
                    self.status = format!(
                        "AI filled {}–{} ({})",
                        r["from"].as_i64().unwrap_or(0) + 1,
                        r["to"].as_i64().unwrap_or(0) - 1,
                        r["model"].as_str().unwrap_or("")
                    );
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            let label = format!("Fill every gap ({open})");
            ui.checkbox(&mut self.panels.ai.every, &label)
                .on_hover_text(match too_long {
                    0 => "Apply fills every empty gap, not only the one at the playhead".to_string(),
                    n => format!("Apply fills every empty gap, not only the one at the playhead. {n} too long for the AI stay as they are"),
                })
                .tag("panel.ai.every", label.as_str());
            self.fill_report_button(ui);
        });
        if pose {
            self.posefill_busy_row(ui);
        }
        if let Some(p) = &pending {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!(
                    "working out the {}... {} ms",
                    if p.plain { "plain movement" } else { "fill" },
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
                ctx.request_repaint_after(Duration::from_millis(50));
            });
        }
        self.preview_controls(ui, &info, &ctx);
    }

    /// The fix for a gap that isn't there.
    fn gap_hint(&self, ui: &mut egui::Ui) {
        ui.add(
            egui::Label::new(
                egui::RichText::new("Put the playhead between two of your poses to start.").weak(),
            )
            .truncate(),
        );
    }

    /// Which AI fills, returning whether it's NVIDIA's: HumanMotion AI is the default, and a gap
    /// the one chosen can't take says why rather than going to the other. NVIDIA AI is offered
    /// before it's installed, with the way to set it up.
    fn fill_with_row(&mut self, ui: &mut egui::Ui) -> bool {
        let installed = self.posefill_installed();
        let st = &mut self.panels.ai;
        if !installed {
            st.pose_fill = false;
        }
        let name = |pose: bool| if pose { "NVIDIA AI" } else { "HumanMotion AI" };
        let mut setup = false;
        ui.horizontal(|ui| {
            egui::ComboBox::from_label("AI model")
                .selected_text(name(st.pose_fill))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut st.pose_fill, false, name(false))
                        .on_hover_text("Comes with the app. Try both and keep the one you prefer")
                        .tag("panel.ai.engine.ours", name(false));
                    if installed {
                        ui.selectable_value(&mut st.pose_fill, true, name(true))
                            .on_hover_text(format!(
                                "Needs an NVIDIA graphics card: gaps of 0.6 to 1.9 s between poses. Try both and keep the one you prefer.\n{}",
                                ac_posefill::NOTICE
                            ))
                            .tag("panel.ai.engine.pose", name(true));
                    } else if ui
                        .selectable_label(false, "Set up NVIDIA AI…")
                        .on_hover_text("Needs an NVIDIA graphics card and a one-time download of about 4 GB")
                        .tag("panel.ai.engine.setup", "Set up NVIDIA AI…")
                        .clicked()
                    {
                        setup = true;
                    }
                })
                .response
                .tag("panel.ai.engine", "AI model");
        });
        if setup {
            self.panels.create.models_open = true;
        }
        self.panels.ai.pose_fill
    }

    /// How many gaps "Fill every gap" would fill, and how many are too long for the AI.
    fn open_gaps(&self, all: &[ac_core::gaps::Gap], pose: bool) -> (usize, usize) {
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
        (open, too_long)
    }

    /// Every gap the AI can take, one undo; the ones it can't are said.
    fn fill_all(&mut self, pose: bool) {
        if let Some(r) = self.run(if pose { "posefill.fill_all" } else { "ai.fill_all" }, json!({})) {
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
    }

    /// Footsteps for the gap at the playhead: which foot leads, how many steps and how it walks,
    /// then `anim.steps`.
    fn steps_row(&mut self, ui: &mut egui::Ui, gap: Option<(i64, i64)>) {
        ui.horizontal_wrapped(|ui| {
            let r = ui
                .add_enabled(gap.is_some(), egui::Button::new("Add steps"))
                .on_hover_text(
                    "Walk between your poses around the playhead, each foot planted where it lands. Put the end key where the walk should end first",
                )
                .on_disabled_hover_text("Put the playhead between two of your poses")
                .tag("panel.ai.steps", "Add steps");
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
                .selected_text(if st.steps_ai { "AI walk" } else { "Built-in walk" })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut st.steps_ai, false, "Built-in walk")
                        .on_hover_text("Moves like the library's Walk")
                        .tag("panel.ai.steps_fill.loop", "Built-in walk");
                    ui.selectable_value(&mut st.steps_ai, true, "AI walk")
                        .on_hover_text("Sets where each heel lands, and the AI fills in the walking between")
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
                if fill == "ai" { "AI walk" } else { "Built-in walk" },
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
                .on_hover_text("Keep this movement (one undo)")
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
            let other = if p.plain { "Show AI" } else { "Compare with no AI" };
            if ui
                .button(other)
                .on_hover_text("Switch the ghost between the AI's movement and a plain blend of your poses")
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

    /// Which model fills (`ai.models`): every model the session can load, the chosen one selected;
    /// one its checks refuse is listed greyed with the reason, and said under the menu too (Rule
    /// 1). Picking one runs `ai.model`; the model's own styles stay in the Style menu under it.
    /// With one model only there is nothing to choose, so the row stays away.
    pub(in crate::app) fn model_row(&mut self, ui: &mut egui::Ui, info: &ac_core::ai::ModelInfo) {
        if self.panels.ai.models.is_none() {
            self.panels.ai.models = Some(
                self.session
                    .run("ai.models", &json!({}))
                    .unwrap_or_else(|e| json!({ "error": e.to_string() })),
            );
        }
        let Some(listed) = self.panels.ai.models.clone() else {
            return;
        };
        if let Some(e) = listed["error"].as_str() {
            ui.label(
                egui::RichText::new(format!("Models: {e}"))
                    .small()
                    .color(self.palette().error),
            );
            return;
        }
        let models: Vec<Value> = listed["models"].as_array().cloned().unwrap_or_default();
        if models.len() < 2 {
            return;
        }
        let name_of = |m: &Value| {
            let name = m["name"].as_str().unwrap_or_default();
            let tag = m["tag"].as_str().unwrap_or_default();
            if name.is_empty() || name == tag {
                tag.to_string()
            } else {
                format!("{name} ({tag})")
            }
        };
        let current = models
            .iter()
            .find(|m| m["tag"] == info.tag.as_str())
            .map_or_else(|| info.tag.clone(), name_of);
        let mut pick: Option<String> = None;
        egui::ComboBox::from_label("Model")
            .selected_text(&current)
            .show_ui(ui, |ui| {
                for m in &models {
                    let tag = m["tag"].as_str().unwrap_or_default().to_string();
                    let loads = m["loads"].as_bool().unwrap_or(false);
                    let why = m["refused"].as_str().unwrap_or("does not load").to_string();
                    let about = format!(
                        "{}: {} bones, {} frames of context, gaps up to {} frames, {}{}\n{}",
                        m["source"].as_str().unwrap_or_default(),
                        m["bones"].as_array().map_or(0, Vec::len),
                        m["context"],
                        m["max_gap"],
                        match m["styles"].as_array().map_or(0, Vec::len) {
                            0 => "no styles".to_string(),
                            n => format!("{n} styles"),
                        },
                        m["base"]["tag"]
                            .as_str()
                            .map(|b| format!(", an add-on to {b}"))
                            .unwrap_or_default(),
                        m["hash"].as_str().unwrap_or_default(),
                    );
                    if ui
                        .add_enabled(loads, egui::Button::selectable(tag == info.tag, name_of(m)))
                        .on_hover_text(&about)
                        .on_disabled_hover_text(format!("Not used: {why}"))
                        .tag(format!("panel.ai.model.{tag}"), &tag)
                        .clicked()
                    {
                        pick = Some(tag);
                    }
                }
            })
            .response
            .on_hover_text("Which model fills; its styles, if any, are in the Style menu below")
            .tag("panel.ai.model", "Model");
        for m in models.iter().filter(|m| m["loads"] != true) {
            ui.label(
                egui::RichText::new(format!(
                    "{} not used: {}",
                    m["tag"].as_str().unwrap_or_default(),
                    m["refused"].as_str().unwrap_or("does not load")
                ))
                .small()
                .color(self.palette().error),
            );
        }
        if let Some(tag) = pick
            && let Some(r) = self.run("ai.model", json!({ "tag": tag }))
        {
            self.status = match r["refused"].as_str() {
                Some(why) => why.to_string(),
                None => format!("filling with {}", r["name"].as_str().unwrap_or_default()),
            };
            // list again: the chosen one changed, and so may the styles under it
            self.panels.ai.models = None;
            if self.session.document.live_preview().is_some() {
                self.ai_dirty = true;
            }
        }
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
        ui.horizontal_wrapped(|ui| {
            for (id, label, tip) in [
                (
                    "preserve",
                    "Keep my movement",
                    "Stays close to your poses and their route",
                ),
                ("natural", "Natural", "Lets the AI move the body freely"),
                (
                    "follow_style",
                    "Match the style",
                    "Leans towards the style chosen below",
                ),
            ] {
                if ui
                    .button(label)
                    .on_hover_text(tip)
                    .tag(format!("panel.ai.preset.{id}"), label)
                    .clicked()
                {
                    preset_chosen = Some(id);
                }
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
        ui.label("Stay closer to");
        ui.horizontal(|ui| {
            ui.spacing_mut().slider_width = 70.0;
            ui.label("My poses");
            changed |= ui
                .add(egui::Slider::new(&mut st.ai_force, 0.0..=1.5).show_value(false))
                .on_hover_text(
                    "How much of the AI's change to keep. All the way left is a plain blend with no AI",
                )
                .tag("panel.ai.ai_force", "Stay closer to")
                .changed();
            ui.label("AI's ideas");
        });
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
        }
        let last_ms = self.panels.ai.last_ms;
        let mut models = false;
        egui::CollapsingHeader::new("Fine-tune")
            .id_salt("panel.ai.fine_tune")
            .show(ui, |ui| {
                if !info.styles.is_empty() {
                    changed |= ui
                        .add(egui::Slider::new(&mut st.style_force, 0.0..=3.0).text("Style strength"))
                        .on_hover_text("How strongly the chosen style shows")
                        .tag("panel.ai.style_force", "Style strength")
                        .changed();
                }
                changed |= ui
                    .add(egui::Slider::new(&mut st.trajectory, 0.0..=1.0).text("Stick to the route"))
                    .on_hover_text("How closely the body follows the route of your poses")
                    .tag("panel.ai.keep_path", "Stick to the route")
                    .changed();
                changed |= ui
                    .add(egui::Slider::new(&mut st.head, 0.0..=1.0).text("Keep head as posed"))
                    .on_hover_text("How closely the neck and head follow your poses")
                    .tag("panel.ai.keep_head", "Keep head as posed")
                    .changed();
                super::create::help(
                    ui,
                    "Shift+A switches the movement at the playhead between AI and plain.",
                );
                if let Some(ms) = last_ms {
                    super::create::help(ui, &format!("The last preview took {ms} ms."));
                }
                models = ui
                    .button("AI models…")
                    .tag("panel.ai.models", "AI models…")
                    .clicked();
            })
            .header_response
            .tag("panel.ai.fine_tune", "Fine-tune");
        if models {
            self.panels.create.models_open = true;
        }
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
