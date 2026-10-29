//! Motion from text before it is installed: what the install needs (`text.install` checks it), the
//! licences installing accepts, and Install, which runs the setup in the background with its step,
//! Stop and the log. When the setup finishes the panel turns into the prompt panel by itself.

use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::app::App;
use crate::app::shell::Level;
use crate::app::ui_ids::Tag;

/// How often the panel asks a running install how it is doing.
const POLL: Duration = Duration::from_secs(1);

const KIMODO_LICENCE: &str =
    "https://www.nvidia.com/en-us/agreements/enterprise-software/nvidia-open-model-license/";
const LLAMA_LICENCE: &str = "https://www.llama.com/llama3/license/";

/// What `text.install` said last, and when.
#[derive(Default)]
pub struct InstallState {
    last: Option<Value>,
    at: Option<Instant>,
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
    /// Ask `text.install` again (now, or once a second while it runs).
    fn install_check(&mut self, ctx: &egui::Context) -> Value {
        let st = &mut self.panels.text.install;
        let due = match (&st.last, st.at) {
            (Some(v), Some(t)) => active(v) && t.elapsed() >= POLL,
            _ => true,
        };
        if due {
            let v = self
                .session
                .run("text.install", &json!({}))
                .unwrap_or_else(|e| json!({ "state": "failed", "error": e.to_string() }));
            let was = st.last.as_ref().is_some_and(active);
            st.last = Some(v.clone());
            st.at = Some(Instant::now());
            if was && !active(&v) {
                let (level, text) = match v["state"].as_str() {
                    Some("installed") => (Level::Info, "Motion from text is installed".to_string()),
                    Some("failed") => (
                        Level::Error,
                        format!(
                            "Motion from text: the install failed: {}",
                            v["error"].as_str().unwrap_or("see the log")
                        ),
                    ),
                    _ => (
                        Level::Info,
                        "Motion from text: the install stopped (Install carries on from there)".to_string(),
                    ),
                };
                self.shell.messages.push(level, text.clone());
                self.status = text;
            }
        }
        let v = self.panels.text.install.last.clone().unwrap_or(Value::Null);
        if active(&v) {
            ctx.request_repaint_after(POLL);
        }
        v
    }

    /// Forget what `text.install` said, so the panel asks again.
    pub(in crate::app) fn install_forget(&mut self) {
        self.panels.text.install = InstallState::default();
    }

    /// The panel while motion from text is not installed; true once it is (the caller then shows
    /// the prompt panel).
    pub(in crate::app) fn text_install_panel(&mut self, ui: &mut egui::Ui) -> bool {
        let v = self.install_check(ui.ctx());
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
                    .tag("panel.text.install_stop", "Stop")
                    .clicked()
                {
                    self.run("text.install", json!({ "action": "stop" }));
                    self.panels.text.install.at = None; // ask again now; it says when it has stopped
                }
                self.install_log_button(ui, &v);
            });
            return false;
        }
        ui.label("Motion from text needs a one-time install: NVIDIA's Kimodo model and its text encoder (about 22 GB, an hour or more to download).");
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
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.label("Installing means you accept ");
            if ui
                .link("NVIDIA's Open Model License")
                .tag("panel.text.licence_kimodo", "NVIDIA Open Model License")
                .clicked()
            {
                self.install_open(KIMODO_LICENCE);
            }
            ui.label(" (Kimodo) and the ");
            if ui
                .link("Llama 3 Community License")
                .tag("panel.text.licence_llama", "Llama 3 Community License")
                .clicked()
            {
                self.install_open(LLAMA_LICENCE);
            }
            ui.label(" (its text encoder).");
        });
        let ready = v["ready"] == true;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(ready, egui::Button::new("Install"))
                .on_hover_text("Download and set up motion from text in the background")
                .on_disabled_hover_text("Something it needs is missing (the red dots above)")
                .tag("panel.text.install", "Install")
                .clicked()
            {
                if self.run("text.install", json!({ "action": "start" })).is_some() {
                    self.status = "Installing motion from text…".into();
                }
                self.panels.text.install.at = None;
            }
            if ui
                .button("Check again")
                .tag("panel.text.check", "Check again")
                .clicked()
            {
                self.install_forget();
            }
            if state == "failed" || state == "stopped" {
                self.install_log_button(ui, &v);
            }
        });
        false
    }

    fn install_log_button(&mut self, ui: &mut egui::Ui, v: &Value) {
        let Some(log) = v["log"].as_str() else { return };
        if ui
            .button("Show log")
            .on_hover_text(log)
            .tag("panel.text.install_log", "Show log")
            .clicked()
            && let Err(e) = crate::app::files::reveal(std::path::Path::new(log))
        {
            self.shell.messages.push(Level::Error, e);
        }
    }

    fn install_open(&mut self, url: &str) {
        if let Err(e) = crate::app::files::open_url(url) {
            self.shell.messages.push(Level::Error, e);
        }
    }
}
