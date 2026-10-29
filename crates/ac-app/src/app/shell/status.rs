//! The status bar along the bottom of the window (the tool's hints, the selection, the frame and
//! its timecode, fps, physics, the AI model, the latest message), the message history, and
//! toasts for refusals and warnings.
//!
//! A refused command reads with its title ("Move key: ..."), not its id; see [`App::refused`].

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use ac_core::CoreError;
use egui::{Color32, RichText};

use super::super::App;
use super::super::prefs;
use super::super::ui_ids::Tag;
use super::super::viewport::Tool;

/// Most messages kept.
const MAX_MESSAGES: usize = 500;
/// Most toasts on screen at once.
const MAX_TOASTS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    /// A fallback or something not set up: the app carried on.
    Warning,
    /// A refusal or failure.
    Error,
}

#[derive(Clone, Debug)]
pub struct Message {
    pub level: Level,
    pub text: String,
    /// Seconds since the app started.
    pub at: f64,
}

pub struct Messages {
    pub list: VecDeque<Message>,
    toasts: Vec<(Message, Instant)>,
    started: Instant,
    /// Errors since the message list was last looked at.
    pub unseen_errors: usize,
}

impl Default for Messages {
    fn default() -> Messages {
        Messages {
            list: VecDeque::new(),
            toasts: Vec::new(),
            started: Instant::now(),
            unseen_errors: 0,
        }
    }
}

impl Messages {
    /// Add a message; errors and warnings also show as a toast.
    pub fn push(&mut self, level: Level, text: String) {
        let m = Message {
            level,
            text,
            at: self.started.elapsed().as_secs_f64(),
        };
        if level != Level::Info {
            self.toasts.push((m.clone(), Instant::now()));
            if self.toasts.len() > MAX_TOASTS {
                self.toasts.remove(0);
            }
        }
        if level == Level::Error {
            self.unseen_errors += 1;
        }
        self.list.push_back(m);
        while self.list.len() > MAX_MESSAGES {
            self.list.pop_front();
        }
    }

    /// A status line set somewhere in the app: kept in the history unless it was just pushed.
    pub fn note(&mut self, level: Level, text: String) {
        if self.list.back().is_some_and(|m| m.text == text) {
            return;
        }
        self.push(level, text);
    }

    pub fn latest(&self) -> Option<&Message> {
        self.list.back()
    }
}

/// How a status line set elsewhere reads: a warning when it says something didn't work.
pub fn level_of(text: &str) -> Level {
    let t = text.to_lowercase();
    if ["can't", "couldn't", "failed", "not loaded", "refused", "won't"]
        .iter()
        .any(|w| t.contains(w))
    {
        Level::Warning
    } else {
        Level::Info
    }
}

/// A refusal as a person reads it: the command's title, then why.
pub fn refusal_text(label: &str, e: &CoreError) -> String {
    let why = match e {
        CoreError::Params { message, .. } | CoreError::Refused { message, .. } => message.clone(),
        CoreError::Panicked { message, .. } => {
            format!("failed unexpectedly and nothing was changed ({message}); please report this")
        }
        other => other.to_string(),
    };
    format!("{label}: {why}")
}

fn colour(level: Level, palette: &super::super::theme::Palette, ui: &egui::Ui) -> Color32 {
    match level {
        Level::Info => ui.visuals().text_color(),
        Level::Warning => palette.warning,
        Level::Error => palette.error,
    }
}

impl App {
    /// A command was refused: the status bar, a toast and the message history say so, under the
    /// command's title.
    pub(in crate::app) fn refused(&mut self, id: &str, e: &CoreError) {
        let label = self.session.spec(id).map_or(id, |s| s.label);
        let text = refusal_text(label, e);
        self.shell.messages.push(Level::Error, text.clone());
        self.status = text;
    }

    /// The tool's hints, by what the pointer does with it (and the modifiers held).
    fn tool_hint(&self, shift: bool) -> &'static str {
        match self.tool {
            Tool::Drag => "Drag a joint to pose it · right-click pins it · K sets a key",
            Tool::Reach => "Drag a joint: the whole body reaches · pins and joint limits hold",
            Tool::Move if shift => {
                "Shift+drag turns the body on this frame about the root · Alt+Shift: the whole animation"
            }
            Tool::Move => {
                "Drag slides the body on this frame (and its key) · Shift+drag turns it · Alt+drag moves the whole animation"
            }
            Tool::Rotate => "Drag a ring to turn the joint about it · inside turns freely · Ctrl snaps 15°",
        }
    }

    /// The bottom strip.
    pub(in crate::app) fn status_bar(&mut self, ui: &mut egui::Ui) {
        let shift = ui.input(|i| i.modifiers.shift);
        let doc = &self.session.document;
        let units = self.shell.prefs.get().general.units;
        let fps = doc.clip.fps;
        let frame = doc.frame;
        let selection = match self.selected.and_then(|i| doc.skeleton.names.get(i)) {
            Some(n) => n.clone(),
            None => "no joint selected".into(),
        };
        let keys = self.timeline.selected.len();
        let physics = match (doc.clip.physics, doc.physics_shown()) {
            (true, true) => "Physics on",
            (true, false) => "Physics on (not shown)",
            (false, _) => "Physics off",
        };
        let model = doc
            .ai
            .as_ref()
            .map_or("no AI model".to_string(), |m| format!("AI: {}", m.info().tag));
        let palette = self.palette();
        ui.horizontal(|ui| {
            ui.label(RichText::new(self.tool_hint(shift)).weak());
            ui.separator();
            ui.label(&selection);
            if keys > 0 {
                ui.label(format!(
                    "· {keys} key{} selected",
                    if keys == 1 { "" } else { "s" }
                ));
            }
            ui.separator();
            let main = match units {
                prefs::Units::Frames => format!("Frame {}", prefs::time_text(frame, fps, units)),
                _ => prefs::time_text(frame, fps, units),
            };
            ui.label(main).on_hover_text(
                "Frame stepping: Left and Right a frame, Up and Down the previous or next key, Home and End",
            );
            if units != prefs::Units::Timecode {
                ui.label(RichText::new(prefs::timecode(frame, fps)).weak().monospace());
            }
            ui.label(format!("{fps:.0} fps"));
            ui.separator();
            ui.label(physics);
            ui.separator();
            ui.label(&model);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let errors = self.shell.messages.unseen_errors;
                let badge = if errors > 0 {
                    format!("Messages ({errors})")
                } else {
                    "Messages".into()
                };
                if ui
                    .small_button(badge.clone())
                    .on_hover_text("Every message this session (status, refusals, warnings)")
                    .tag("status.messages", badge)
                    .clicked()
                {
                    self.shell.open.messages = !self.shell.open.messages;
                }
                if let Some(m) = self.shell.messages.latest() {
                    let c = colour(m.level, palette, ui);
                    let r = ui.add(
                        egui::Label::new(RichText::new(&m.text).color(c))
                            .truncate()
                            .sense(egui::Sense::click()),
                    );
                    if r.on_hover_text(&m.text)
                        .tag("status.latest", m.text.as_str())
                        .clicked()
                    {
                        self.shell.open.messages = true;
                    }
                }
            });
        });
    }

    /// The message history (View > Messages, or a click on the status bar's message).
    pub(in crate::app) fn messages_window(&mut self, ctx: &egui::Context) {
        if !self.shell.open.messages {
            return;
        }
        self.shell.messages.unseen_errors = 0;
        let palette = self.palette();
        let mut open = true;
        let mut clear = false;
        egui::Window::new("Messages")
            .open(&mut open)
            .default_size([520.0, 320.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "{} messages, newest first",
                        self.shell.messages.list.len()
                    ));
                    if ui
                        .button("Copy all")
                        .tag("window.messages.copy_all", "Copy all")
                        .clicked()
                    {
                        let all: Vec<String> = self
                            .shell
                            .messages
                            .list
                            .iter()
                            .map(|m| format!("[{:.1}s] {:?}: {}", m.at, m.level, m.text))
                            .collect();
                        ui.ctx().copy_text(all.join("\n"));
                    }
                    clear = ui.button("Clear").tag("window.messages.clear", "Clear").clicked();
                });
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    for m in self.shell.messages.list.iter().rev() {
                        ui.horizontal_wrapped(|ui| {
                            let secs = m.at as u64;
                            ui.label(
                                RichText::new(format!("{:02}:{:02}", secs / 60, secs % 60))
                                    .weak()
                                    .monospace(),
                            );
                            let tag = match m.level {
                                Level::Info => "",
                                Level::Warning => "warning: ",
                                Level::Error => "refused: ",
                            };
                            ui.label(
                                RichText::new(format!("{tag}{}", m.text)).color(colour(m.level, palette, ui)),
                            );
                        });
                    }
                });
            });
        if clear {
            self.shell.messages.list.clear();
        }
        self.shell.open.messages = open;
    }

    /// Refusals and warnings for a few seconds above the status bar; a click dismisses one.
    pub(in crate::app) fn toasts(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let toasts = &mut self.shell.messages.toasts;
        toasts.retain(|(m, at)| {
            let keep = if m.level == Level::Error { 6 } else { 4 };
            now.duration_since(*at) < Duration::from_secs(keep)
        });
        if toasts.is_empty() {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(250));
        let palette = self.palette();
        let mut dismiss = None;
        egui::Area::new(egui::Id::unique("toasts"))
            .anchor(egui::Align2::RIGHT_BOTTOM, [-12.0, -40.0])
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                for (i, (m, _)) in self.shell.messages.toasts.iter().enumerate() {
                    let c = colour(m.level, palette, ui);
                    let r = egui::Frame::popup(ui.style())
                        .stroke(egui::Stroke::new(1.5, c))
                        .show(ui, |ui| {
                            ui.label(RichText::new(&m.text).color(c));
                        })
                        .response
                        .interact(egui::Sense::click());
                    if r.on_hover_text("Click to dismiss")
                        .tag(format!("status.toast.{i}"), m.text.as_str())
                        .clicked()
                    {
                        dismiss = Some(i);
                    }
                }
            });
        if let Some(i) = dismiss
            && i < self.shell.messages.toasts.len()
        {
            self.shell.messages.toasts.remove(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refusals_read_with_the_title() {
        let e = CoreError::Refused {
            command: "key.move".into(),
            message: "no key at frame 3".into(),
            hint: None,
        };
        assert_eq!(refusal_text("Move key", &e), "Move key: no key at frame 3");
        let e = CoreError::Params {
            command: "frame.set".into(),
            message: "parameter \"frame\": expected a number".into(),
        };
        assert!(refusal_text("Go to frame", &e).starts_with("Go to frame: parameter"));
        assert!(!refusal_text("Go to frame", &e).contains("frame.set"));
    }

    #[test]
    fn messages_keep_the_newest_and_toast_problems() {
        let mut m = Messages::default();
        m.push(Level::Info, "saved".into());
        assert!(m.toasts.is_empty());
        m.push(Level::Error, "Move key: no".into());
        m.note(Level::Info, "Move key: no".into());
        assert_eq!(m.list.len(), 2, "a status line just pushed isn't kept twice");
        assert_eq!(m.toasts.len(), 1);
        assert_eq!(m.unseen_errors, 1);
        for i in 0..(MAX_MESSAGES + 10) {
            m.push(Level::Warning, format!("w{i}"));
        }
        assert_eq!(m.list.len(), MAX_MESSAGES);
        assert!(m.toasts.len() <= MAX_TOASTS);
        assert_eq!(level_of("couldn't open x"), Level::Warning);
        assert_eq!(level_of("saved walk"), Level::Info);
    }
}
