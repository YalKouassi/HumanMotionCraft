//! Autosave and crash recovery in the window: after edits settle, the open animation's unsaved
//! work goes to `<library>/.recovery/` (`recovery.write`); saving, or going back to what the
//! library holds, clears it (`recovery.clear`). At start-up the window offers unsaved work back
//! (`recovery.list`, `recovery.restore`).

use std::time::{Duration, Instant};

use ac_core::clip::Clip;
use serde_json::{Value, json};

use super::super::App;
use super::super::ui_ids::Tag;

/// The edits must have paused this long before a write (a drag isn't saved mid-way).
const QUIET: Duration = Duration::from_millis(1500);
/// How often the open clip is compared with what was last written.
const CHECK: Duration = Duration::from_millis(500);

#[derive(Default)]
pub struct Autosave {
    checked: Option<Instant>,
    /// The clip as last seen, and when it last changed.
    seen: Option<(Clip, Instant)>,
    /// What the recovery file holds (`None`: no file of ours).
    written: Option<(Option<String>, Clip)>,
    last_write: Option<Instant>,
}

impl App {
    /// The autosave interval in seconds (0: off): Settings > Autosave
    /// (`prefs.general.autosave_secs`).
    pub(super) fn autosave_secs(&self) -> f64 {
        f64::from(self.shell.prefs.get().general.autosave_secs)
    }

    /// Write the recovery file when unsaved edits have settled; clear it once nothing is unsaved.
    pub(super) fn autosave_tick(&mut self) {
        let now = Instant::now();
        let a = &mut self.files.autosave;
        if a.checked.is_some_and(|t| now - t < CHECK) {
            return;
        }
        a.checked = Some(now);
        let secs = self.autosave_secs();
        if self.session.in_gesture() || self.session.document.library.is_none() {
            return;
        }
        let doc = &self.session.document;
        if !doc.unsaved() {
            self.files.autosave.seen = None;
            if self.files.autosave.written.take().is_some() {
                self.clear_recovery();
            }
            return;
        }
        if !(secs.is_finite() && secs > 0.0) {
            return;
        }
        let a = &mut self.files.autosave;
        if a.seen.as_ref().is_none_or(|(c, _)| *c != doc.clip) {
            a.seen = Some((doc.clip.clone(), now));
        }
        let Some((clip, changed)) = &a.seen else { return };
        let same = a
            .written
            .as_ref()
            .is_some_and(|(id, c)| *id == doc.clip_id && c == clip);
        let due = a
            .last_write
            .is_none_or(|t| now - t >= Duration::from_secs_f64(secs.min(86_400.0)));
        if same || now - *changed < QUIET || !due {
            return;
        }
        let (id, clip) = (doc.clip_id.clone(), clip.clone());
        a.last_write = Some(now);
        match self.session.run("recovery.write", &json!({})) {
            Ok(_) => self.files.autosave.written = Some((id, clip)),
            Err(e) => self.status = format!("couldn't keep unsaved work for recovery: {e}"),
        }
    }

    /// Remove the open animation's recovery file (after a save, or work discarded).
    pub(in crate::app) fn clear_recovery(&mut self) {
        self.files.autosave.written = None;
        if self.session.document.library.is_none() {
            return;
        }
        if let Err(e) = self.session.run("recovery.clear", &json!({})) {
            self.status = format!("couldn't remove the recovery file: {e}");
        }
    }

    /// How many recovery files hold work the library doesn't (`None`: none).
    pub(super) fn recovery_offered(&mut self) -> Option<u64> {
        let r = self.session.run("recovery.list", &json!({})).ok()?;
        r["offered"].as_u64().filter(|n| *n > 0)
    }

    pub(in crate::app) fn open_recovery(&mut self) {
        match self.session.run("recovery.list", &json!({})) {
            Ok(r) => {
                let items: Vec<Value> = r["items"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|i| i["offer"] == json!(true) || i.get("error").is_some())
                    .cloned()
                    .collect();
                self.files.recover = Some(items);
            }
            Err(e) => self.status = format!("couldn't look for unsaved work: {e}"),
        }
    }

    /// "Restore unsaved work" / "Discard", per animation.
    pub(super) fn recovery_dialog(&mut self, ctx: &egui::Context) {
        let Some(items) = self.files.recover.clone() else {
            return;
        };
        let mut action: Option<(&str, String)> = None;
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::unique("recovery")).show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.heading("Unsaved work from last time");
            ui.label("These animations had changes that weren't saved when the app closed.");
            ui.add_space(6.0);
            if items.is_empty() {
                ui.label(egui::RichText::new("Nothing is waiting.").weak());
            }
            for it in &items {
                let file = it["file"].as_str().unwrap_or_default().to_string();
                ui.horizontal(|ui| {
                    if let Some(e) = it["error"].as_str() {
                        ui.label(format!("{file}: can't be read ({e})"));
                        if ui
                            .button("Discard")
                            .tag(format!("dialog.recover.discard.{file}"), "Discard")
                            .clicked()
                        {
                            action = Some(("discard", file.clone()));
                        }
                        return;
                    }
                    let name = it["name"].as_str().unwrap_or("untitled");
                    let what = match it["library"].as_str() {
                        Some("missing") if it["id"].is_null() => "never saved".to_string(),
                        Some("missing") => "no longer in the library".to_string(),
                        _ => "newer than the saved one".to_string(),
                    };
                    ui.label(egui::RichText::new(name).strong());
                    ui.label(egui::RichText::new(format!("{} keys, {what}", it["keys"])).weak());
                    if ui
                        .button("Restore")
                        .tag(format!("dialog.recover.restore.{file}"), "Restore")
                        .clicked()
                    {
                        action = Some(("restore", file.clone()));
                    }
                    if ui
                        .button("Discard")
                        .tag(format!("dialog.recover.discard.{file}"), "Discard")
                        .clicked()
                    {
                        action = Some(("discard", file.clone()));
                    }
                });
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if items.len() > 1
                    && ui
                        .button("Discard all")
                        .tag("dialog.recover.discard_all", "Discard all")
                        .clicked()
                {
                    action = Some(("discard-all", String::new()));
                }
                if ui
                    .button("Later")
                    .on_hover_text("Keep them: File > Restore unsaved work… brings this back")
                    .tag("dialog.recover.later", "Later")
                    .clicked()
                {
                    close = true;
                }
            });
        });
        if modal.should_close() {
            close = true;
        }
        match action {
            Some(("restore", file)) => {
                if self.run("recovery.restore", json!({ "file": file })).is_some() {
                    self.status = format!("restored unsaved work ({file}): save to keep it");
                    self.timeline.selected.clear();
                    self.refresh_library();
                    close = true;
                }
            }
            Some(("discard", file)) => {
                if self.run("recovery.clear", json!({ "file": file })).is_some() {
                    self.status = format!("discarded {file}");
                }
                self.open_recovery();
                if self.files.recover.as_ref().is_some_and(Vec::is_empty) {
                    close = true;
                }
            }
            Some((_, _)) => {
                if self.run("recovery.clear", json!({ "all": true })).is_some() {
                    self.status = "discarded all unsaved work from last time".into();
                }
                close = true;
            }
            None => {}
        }
        if close {
            self.files.recover = None;
        }
    }
}
