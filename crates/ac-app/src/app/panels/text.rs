//! Motion from text: a prompt, how long and which take (seed), then Generate (a new stretch from
//! the playhead, `text.generate`) or Fill gap (between your keys around it, `text.fill`).
//!
//! The model takes a minute or more, so the window never runs these on its UI thread while it
//! works: [`App::run`] hands a `text.*` run that would wait ([`ac_textmotion::waits`]) to a thread
//! here, which makes the motion on a copy of the document ([`ac_textmotion::prepare`]); when it is
//! done the command runs and finds the motion ready. If the animation changed meanwhile the
//! motion no longer fits, and the panel says so instead of making it again unasked.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc;
use std::time::Instant;

use serde_json::{Value, json};

use crate::app::App;
use crate::app::shell::Level;
use crate::app::ui_ids::Tag;

/// A run being made off the UI thread.
struct Job {
    id: String,
    params: Value,
    since: Instant,
    done: mpsc::Receiver<Result<(), String>>,
}

/// The panel's window state.
pub struct TextState {
    pub prompt: String,
    pub seconds: f64,
    pub seed: i64,
    job: Option<Job>,
    /// What `text.helper` said last (checked when the panel first shows, and on Check).
    helper: Option<Value>,
    /// The install, while motion from text is not installed.
    pub install: super::text_install::InstallState,
    ctx: Option<egui::Context>,
}

impl Default for TextState {
    fn default() -> TextState {
        TextState {
            prompt: String::new(),
            seconds: 4.0,
            seed: 1,
            job: None,
            helper: None,
            install: Default::default(),
            ctx: None,
        }
    }
}

impl App {
    /// Whether `run` must make `id`'s motion off the UI thread first; if so it is started.
    pub(in crate::app) fn text_deferred(&mut self, id: &str, params: &Value) -> bool {
        let Some(p) = params.as_object() else { return false };
        if !id.starts_with("text.") || !ac_textmotion::waits(&self.session.document, id, p) {
            return false;
        }
        let st = &mut self.panels.text;
        if let Some(j) = &st.job {
            self.status = format!(
                "Motion from text is still working on {} ({} s)",
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
            .name("text-motion".into())
            .spawn(move || {
                let r = catch_unwind(AssertUnwindSafe(|| ac_textmotion::prepare(&doc, &cmd, &p)))
                    .unwrap_or_else(|_| Err("motion from text failed unexpectedly".into()));
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
                self.status = "Making motion from text…".into();
            }
            Err(e) => {
                let text = format!("Motion from text: can't start its thread: {e}");
                self.shell.messages.push(Level::Error, text.clone());
                self.status = text;
            }
        }
        true
    }

    /// Finish a run whose motion is made (every frame, open or not).
    pub(in crate::app) fn text_poll(&mut self, ctx: &egui::Context) {
        let st = &mut self.panels.text;
        if st.ctx.is_none() {
            st.ctx = Some(ctx.clone());
        }
        let Some(job) = &st.job else { return };
        let r = match job.done.try_recv() {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => Err("motion from text stopped unexpectedly".into()),
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
                if ac_textmotion::waits(&self.session.document, &job.id, &p) {
                    let text = format!(
                        "{label}: the animation changed while the motion was made, so it was not used: run it again"
                    );
                    self.shell.messages.push(Level::Warning, text.clone());
                    self.status = text;
                } else if let Some(out) = self.run(&job.id, job.params) {
                    self.text_done(&label, &out);
                }
            }
        }
        self.panels.text.helper = None; // loaded now (or stopped): ask again
    }

    fn text_done(&mut self, label: &str, out: &Value) {
        self.status = match (out["from"].as_i64(), out["to"].as_i64()) {
            (Some(a), Some(b)) => format!("{label}: frames {a}–{b}"),
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

    pub(in crate::app) fn text_panel(&mut self, ui: &mut egui::Ui) {
        if self.panels.text.helper.is_none() {
            self.panels.text.helper = self.session.run("text.helper", &json!({})).ok();
        }
        let helper = self.panels.text.helper.clone().unwrap_or(Value::Null);
        if helper["installed"] != true {
            if self.text_install_panel(ui) {
                // installed now: ask the helper again, and the prompt panel shows
                self.panels.text.helper = None;
                self.install_forget();
            }
            return;
        }
        let state = if helper["loaded"] == true {
            "model loaded"
        } else if helper["running"] == true {
            "helper running"
        } else {
            "loads on first use (a minute or more)"
        };
        ui.label(egui::RichText::new(format!("{} — {state}", helper["model"].as_str().unwrap_or(""))).weak());
        let st = &mut self.panels.text;
        ui.add(
            egui::TextEdit::multiline(&mut st.prompt)
                .desired_rows(2)
                .hint_text("A person waves hello with their right hand."),
        )
        .tag("panel.text.prompt", "Prompt");
        ui.horizontal(|ui| {
            ui.label("Seconds");
            ui.add(egui::DragValue::new(&mut st.seconds).range(0.1..=9.9).speed(0.1))
                .tag("panel.text.seconds", "Seconds");
            ui.label("Take");
            ui.add(egui::DragValue::new(&mut st.seed).range(0..=999_999))
                .on_hover_text("Another number makes another take of the same prompt")
                .tag("panel.text.seed", "Take");
        });
        let busy = st.job.as_ref().map(|j| j.since);
        let (prompt, seconds, seed) = (st.prompt.clone(), st.seconds, st.seed);
        let doc = &self.session.document;
        let at = doc.key_frame();
        let gap = ac_core::edits::span_around(&doc.clip, at);
        let empty = prompt.trim().is_empty();
        ui.horizontal_wrapped(|ui| {
            let can = busy.is_none() && !empty;
            if ui
                .add_enabled(can, egui::Button::new("Generate"))
                .on_hover_text(format!("Make {seconds:.1} s of motion starting at the playhead"))
                .tag("panel.text.generate", "Generate")
                .clicked()
            {
                let p = json!({ "prompt": prompt, "seconds": seconds, "seed": seed, "at": at.max(0) });
                if let Some(out) = self.run("text.generate", p) {
                    self.text_done("Generate motion from text", &out);
                }
            }
            let fill = gap.map(|(a, b)| format!("Fill the gap {a}–{b} from the prompt, held to both keys"));
            if ui
                .add_enabled(can && gap.is_some(), egui::Button::new("Fill gap"))
                .on_hover_text(fill.unwrap_or_default())
                .on_disabled_hover_text("Put the playhead between two of your keys")
                .tag("panel.text.fill", "Fill gap")
                .clicked()
                && let Some((a, b)) = gap
            {
                // the gap's keys, not the playhead: it may move while the model works
                let p = json!({ "prompt": prompt, "seed": seed, "from": a, "to": b });
                if let Some(out) = self.run("text.fill", p) {
                    self.text_done("Fill gap from text", &out);
                }
            }
        });
        if let Some(since) = busy {
            ui.horizontal_wrapped(|ui| {
                ui.spinner();
                let said = ac_textmotion::engine::progress();
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
}
