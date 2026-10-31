//! Pose fill: the gap between your keys around the playhead filled by the pose-to-pose model
//! (`posefill.fill`), with the helper's state and, before it is installed, the install
//! (`posefill.install`: what it needs, the licences installing accepts, Install with its step,
//! Stop and the log; when the setup finishes the panel turns into the fill panel by itself).
//!
//! A fill takes a moment, and the first one minutes while the model's plan for this GPU is
//! built, so the window never runs it on its UI thread: [`App::run`] hands a `posefill.*` run that
//! would wait ([`ac_posefill::waits`]) to a thread here, which makes the fills on a copy of the
//! document ([`ac_posefill::prepare`]); when it is done the command runs and finds them ready. If
//! the animation changed meanwhile the fills no longer fit, and the panel says so instead of
//! making them again unasked.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::app::App;
use crate::app::shell::Level;
use crate::app::ui_ids::Tag;

/// How often the panel asks a running install how it is doing.
const POLL: Duration = Duration::from_secs(1);

/// A run being made off the UI thread.
struct Job {
    id: String,
    params: Value,
    since: Instant,
    done: mpsc::Receiver<Result<(), String>>,
}

/// The panel's window state.
#[derive(Default)]
pub struct PoseFillState {
    job: Option<Job>,
    /// What `posefill.helper` said last (checked when the panel first shows, and after a run).
    helper: Option<Value>,
    /// What `posefill.install` said last, and when.
    install: Option<Value>,
    install_at: Option<Instant>,
    ctx: Option<egui::Context>,
}

fn active(v: &Value) -> bool {
    matches!(v["state"].as_str(), Some("installing" | "stopping"))
}

fn minutes(v: &Value) -> String {
    let s = v["seconds"].as_u64().unwrap_or(0);
    if s < 60 {
        format!("{s} s")
    } else {
        format!("{} min", s / 60)
    }
}

fn step(v: &Value) -> Option<String> {
    let (n, of) = (v["step"].as_i64()?, v["steps"].as_i64()?);
    Some(format!("step {n} of {of}: {}", v["label"].as_str().unwrap_or("")))
}

impl App {
    /// Whether `run` must make `id`'s fills off the UI thread first; if so it is started.
    pub(in crate::app) fn posefill_deferred(&mut self, id: &str, params: &Value) -> bool {
        let Some(p) = params.as_object() else { return false };
        if !id.starts_with("posefill.") || !ac_posefill::waits(&self.session.document, id, p) {
            return false;
        }
        let st = &mut self.panels.posefill;
        if let Some(j) = &st.job {
            self.status = format!(
                "Pose fill is still working on {} ({} s)",
                j.id,
                j.since.elapsed().as_secs()
            );
            return true;
        }
        let (tx, rx) = mpsc::channel();
        let doc = self.session.document.clone();
        let (cmd, p) = (id.to_string(), p.clone());
        let wake = st.ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("pose-fill".into())
            .spawn(move || {
                let r = catch_unwind(AssertUnwindSafe(|| ac_posefill::prepare(&doc, &cmd, &p)))
                    .unwrap_or_else(|_| Err("the pose fill failed unexpectedly".into()));
                let _ = tx.send(r);
                if let Some(c) = wake {
                    c.request_repaint();
                }
            });
        match spawned {
            Ok(_) => {
                st.job = Some(Job {
                    id: id.to_string(),
                    params: params.clone(),
                    since: Instant::now(),
                    done: rx,
                });
                self.status = "Filling from the poses…".into();
            }
            Err(e) => {
                let text = format!("Pose fill: can't start its thread: {e}");
                self.shell.messages.push(Level::Error, text.clone());
                self.status = text;
            }
        }
        true
    }

    /// Finish a run whose fills are made (every frame, open or not).
    pub(in crate::app) fn posefill_poll(&mut self, ctx: &egui::Context) {
        let st = &mut self.panels.posefill;
        if st.ctx.is_none() {
            st.ctx = Some(ctx.clone());
        }
        let Some(job) = &st.job else { return };
        let r = match job.done.try_recv() {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(250));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => Err("the pose fill stopped unexpectedly".into()),
        };
        let Some(job) = st.job.take() else { return };
        let label = self
            .session
            .spec(&job.id)
            .map_or(job.id.as_str(), |s| s.label)
            .to_string();
        match r {
            Err(e) => {
                let text = format!("{label}: {}", e.replace(ac_core::error::HINT, " — "));
                self.shell.messages.push(Level::Error, text.clone());
                self.status = text;
            }
            Ok(()) => {
                let p = job.params.as_object().cloned().unwrap_or_default();
                if ac_posefill::waits(&self.session.document, &job.id, &p) {
                    let text = format!(
                        "{label}: the animation changed while the fill was made, so it was not used: run it again"
                    );
                    self.shell.messages.push(Level::Warning, text.clone());
                    self.status = text;
                } else if let Some(out) = self.run(&job.id, job.params) {
                    self.posefill_done(&label, &out);
                }
            }
        }
        self.panels.posefill.helper = None; // loaded now (or stopped): ask again
    }

    fn posefill_done(&mut self, label: &str, out: &Value) {
        self.status = match (out["from"].as_i64(), out["to"].as_i64(), out["calls"].as_i64()) {
            (Some(a), Some(b), Some(n)) if n > 1 => format!("{label}: frames {a}–{b} in {n} calls"),
            (Some(a), Some(b), _) => format!("{label}: frames {a}–{b}"),
            _ => format!("{label}: done"),
        };
        for n in out["notes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            self.shell.messages.push(Level::Warning, format!("{label}: {n}"));
        }
    }

    pub(in crate::app) fn posefill_panel(&mut self, ui: &mut egui::Ui) {
        if self.panels.posefill.helper.is_none() {
            self.panels.posefill.helper = self.session.run("posefill.helper", &json!({})).ok();
        }
        let helper = self.panels.posefill.helper.clone().unwrap_or(Value::Null);
        if helper["installed"] != true {
            if self.posefill_install_panel(ui) {
                // installed now: ask the helper again, and the fill panel shows
                self.panels.posefill.helper = None;
                self.panels.posefill.install = None;
                self.panels.posefill.install_at = None;
            }
            return;
        }
        let plan_built = helper["plan"].as_str() == Some("built");
        let state = if helper["loaded"] == true {
            "model loaded"
        } else if helper["running"] == true {
            "helper running"
        } else if plan_built {
            "loads on first use (a few seconds)"
        } else {
            "loads on first use, and builds its plan for this GPU first (a few minutes, once)"
        };
        ui.label(egui::RichText::new(format!("{} — {state}", helper["model"].as_str().unwrap_or(""))).weak());
        if let Some(gpu) = helper["gpu"].as_str().filter(|g| !g.is_empty()) {
            ui.label(egui::RichText::new(gpu).small().weak());
        }
        let busy = self.panels.posefill.job.as_ref().map(|j| j.since);
        let doc = &self.session.document;
        let at = doc.key_frame();
        let gap = ac_core::edits::span_around(&doc.clip, at);
        let fps = doc.clip.fps;
        let most = ((ac_posefill::MAX_FRAMES - 1) as f64 * fps / ac_posefill::MODEL_FPS).round() as i64;
        let least = ((ac_posefill::MIN_FRAMES - 1) as f64 * fps / ac_posefill::MODEL_FPS).round() as i64;
        ui.label(
            egui::RichText::new(format!(
                "Fills the gap between your keys around the playhead from the poses before and after it: {least} to {most} frames a gap at {fps} fps. A longer stretch goes through the keys inside it."
            ))
            .small()
            .weak(),
        );
        ui.horizontal_wrapped(|ui| {
            let can = busy.is_none() && gap.is_some();
            let hover =
                gap.map(|(a, b)| format!("Fill the gap {a}–{b} from the poses around it, held to both keys"));
            if ui
                .add_enabled(can, egui::Button::new("Fill gap"))
                .on_hover_text(hover.unwrap_or_default())
                .on_disabled_hover_text("Put the playhead between two of your keys")
                .tag("panel.posefill.fill", "Fill gap")
                .clicked()
                && let Some((a, b)) = gap
            {
                // the gap's keys, not the playhead: it may move while the model works
                if let Some(out) = self.run("posefill.fill", json!({ "from": a, "to": b })) {
                    self.posefill_done("Fill gap from poses", &out);
                }
            }
            if helper["running"] != true
                && ui
                    .add_enabled(busy.is_none(), egui::Button::new("Load"))
                    .on_hover_text("Start the helper and load the model now, so the first fill doesn't wait")
                    .tag("panel.posefill.load", "Load")
                    .clicked()
            {
                self.run("posefill.helper", json!({ "action": "load" }));
            }
            if helper["running"] == true
                && ui
                    .add_enabled(busy.is_none(), egui::Button::new("Stop helper"))
                    .on_hover_text("End the helper process (the next fill starts it again)")
                    .tag("panel.posefill.stop", "Stop helper")
                    .clicked()
            {
                self.run("posefill.helper", json!({ "action": "stop" }));
                self.panels.posefill.helper = None;
            }
        });
        if let Some(since) = busy {
            ui.horizontal_wrapped(|ui| {
                ui.spinner();
                let said = ac_posefill::engine::progress();
                ui.label(format!(
                    "{} ({} s)",
                    if said.is_empty() {
                        "working".to_string()
                    } else {
                        said
                    },
                    since.elapsed().as_secs()
                ));
            });
        }
    }

    /// Ask `posefill.install` again (now, or once a second while it runs).
    fn posefill_install_check(&mut self, ctx: &egui::Context) -> Value {
        let st = &mut self.panels.posefill;
        let due = match (&st.install, st.install_at) {
            (Some(v), Some(t)) => active(v) && t.elapsed() >= POLL,
            _ => true,
        };
        if due {
            let v = self
                .session
                .run("posefill.install", &json!({}))
                .unwrap_or_else(|e| json!({ "state": "failed", "error": e.to_string() }));
            let was = st.install.as_ref().is_some_and(active);
            st.install = Some(v.clone());
            st.install_at = Some(Instant::now());
            if was && !active(&v) {
                let (level, text) = match v["state"].as_str() {
                    Some("installed") => (Level::Info, "Pose fill is installed".to_string()),
                    Some("failed") => (
                        Level::Error,
                        format!(
                            "Pose fill: the install failed: {}",
                            v["error"].as_str().unwrap_or("see the log")
                        ),
                    ),
                    _ => (
                        Level::Info,
                        "Pose fill: the install stopped (Install carries on from there)".to_string(),
                    ),
                };
                self.shell.messages.push(level, text.clone());
                self.status = text;
            }
        }
        let v = self.panels.posefill.install.clone().unwrap_or(Value::Null);
        if active(&v) {
            ctx.request_repaint_after(POLL);
        }
        v
    }

    /// The panel while pose fill is not installed; true once it is (the caller then shows the
    /// fill panel).
    fn posefill_install_panel(&mut self, ui: &mut egui::Ui) -> bool {
        let v = self.posefill_install_check(ui.ctx());
        let state = v["state"].as_str().unwrap_or("");
        if state == "installed" {
            return true;
        }
        let pal = self.palette();
        if active(&v) {
            ui.horizontal_wrapped(|ui| {
                ui.spinner();
                let what = step(&v).unwrap_or_else(|| "starting".into());
                let doing = if state == "stopping" {
                    "Stopping"
                } else {
                    "Installing"
                };
                ui.label(format!("{doing}: {what} ({})", minutes(&v)));
            });
            if let Some(last) = v["last"].as_str() {
                ui.label(egui::RichText::new(last).small().weak());
            }
            ui.label(
                egui::RichText::new("The window stays usable meanwhile. Quitting the app stops the install; Install carries on from there.")
                    .small()
                    .weak(),
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(state == "installing", egui::Button::new("Stop"))
                    .tag("panel.posefill.install_stop", "Stop")
                    .clicked()
                {
                    self.run("posefill.install", json!({ "action": "stop" }));
                    self.panels.posefill.install_at = None; // ask again now; it says when it has stopped
                }
                self.posefill_log_button(ui, &v);
            });
            return false;
        }
        ui.label("Pose fill needs a one-time install: NVIDIA's pose-to-pose motion model and the TensorRT runtime it runs on (about 4 GB, a few minutes). It needs an NVIDIA GPU; the first fill builds the model's plan for it (a few minutes, once).");
        match state {
            "failed" => {
                let at = step(&v).map(|s| format!(" at {s}")).unwrap_or_default();
                ui.label(
                    egui::RichText::new(format!(
                        "The install failed{at}: {}",
                        v["error"].as_str().unwrap_or("see the log")
                    ))
                    .color(pal.error),
                );
            }
            "stopped" => {
                let at = step(&v).map(|s| format!(" at {s}")).unwrap_or_default();
                ui.label(
                    egui::RichText::new(format!("The install stopped{at}. Install carries on from there."))
                        .color(pal.warning),
                );
            }
            _ => {}
        }
        for need in v["requirements"].as_array().into_iter().flatten() {
            let ok = need["ok"] == true;
            // a dot drawn in code: found, missing (needed), missing (it works less well without)
            let (colour, said) = match (ok, need["required"] == true) {
                (true, _) => (pal.support_ok, "found"),
                (false, true) => (pal.error, "missing, needed"),
                (false, false) => (pal.warning, "missing, optional"),
            };
            ui.horizontal_wrapped(|ui| {
                let (rect, dot) = ui.allocate_exact_size(egui::vec2(10.0, 14.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.0, colour);
                dot.on_hover_text(said);
                ui.label(egui::RichText::new(need["name"].as_str().unwrap_or("")).strong());
                ui.label(
                    egui::RichText::new(need["note"].as_str().unwrap_or(""))
                        .small()
                        .weak(),
                );
            });
        }
        ui.label("Installing means you accept:");
        let licences: Vec<(String, String, String)> = v["licences"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|l| {
                (
                    l["part"].as_str().unwrap_or("").to_string(),
                    l["licence"].as_str().unwrap_or("").to_string(),
                    l["url"].as_str().unwrap_or("").to_string(),
                )
            })
            .collect();
        for (i, (part, licence, url)) in licences.iter().enumerate() {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.label(format!("{part}: "));
                if ui
                    .link(licence)
                    .on_hover_text(url)
                    .tag(format!("panel.posefill.licence_{i}"), licence.clone())
                    .clicked()
                    && let Err(e) = crate::app::files::open_url(url)
                {
                    self.shell.messages.push(Level::Error, e);
                }
            });
        }
        if let Some(left) = v["left_out"].as_str() {
            ui.label(egui::RichText::new(left).small().weak());
        }
        let ready = v["ready"] == true;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(ready, egui::Button::new("Install"))
                .on_hover_text("Download and set up pose fill in the background")
                .on_disabled_hover_text("Something it needs is missing (the red dots above)")
                .tag("panel.posefill.install", "Install")
                .clicked()
            {
                if self
                    .run("posefill.install", json!({ "action": "start" }))
                    .is_some()
                {
                    self.status = "Installing pose fill…".into();
                }
                self.panels.posefill.install_at = None;
            }
            if ui
                .button("Check again")
                .tag("panel.posefill.check", "Check again")
                .clicked()
            {
                self.panels.posefill.install = None;
                self.panels.posefill.install_at = None;
            }
            if state == "failed" || state == "stopped" {
                self.posefill_log_button(ui, &v);
            }
        });
        false
    }

    fn posefill_log_button(&mut self, ui: &mut egui::Ui, v: &Value) {
        let Some(log) = v["log"].as_str() else { return };
        if ui
            .button("Show log")
            .on_hover_text(log)
            .tag("panel.posefill.install_log", "Show log")
            .clicked()
            && let Err(e) = crate::app::files::reveal(std::path::Path::new(log))
        {
            self.shell.messages.push(Level::Error, e);
        }
    }
}
