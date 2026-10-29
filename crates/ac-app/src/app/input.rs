//! Pointer and key input from the control port (op `input`), so an agent can use the window the
//! way a person does: one event a frame, through egui's own input (`raw_input_hook`), and a
//! reply once the last one has been handled. Op `layout` says where things are on screen.

use std::collections::VecDeque;
use std::sync::mpsc::Sender;

use egui::{Event, Modifiers, PointerButton, Pos2, Vec2};
use serde_json::{Value, json};

use super::App;

/// Events waiting to be played, and the reply owed once the last one is.
#[derive(Default)]
pub struct Injected {
    /// One entry a frame: its events and the modifiers held.
    queue: VecDeque<(Vec<Event>, Modifiers, Option<Sender<Value>>)>,
    /// The batch's reply, sent after the frame that handled its last event.
    reply: Option<Sender<Value>>,
    /// Modifiers held for the event being played.
    modifiers: Modifiers,
    /// Frames the owed reply has waited for the window to settle.
    waited: u32,
    /// The widget id a `click` pressed (said in the reply).
    pub clicked: Option<String>,
    /// The agent's input has the window, as a person's click would, until the OS moves the
    /// focus: a window started from a background process may never get the OS focus, and
    /// without it no widget keeps the keyboard.
    window: bool,
}

impl Injected {
    /// Nothing queued and no reply owed.
    pub fn idle(&self) -> bool {
        self.queue.is_empty() && self.reply.is_none()
    }
}

/// Most frames a reply waits for the window to settle.
const MAX_SETTLE_FRAMES: u32 = 120;

/// Most events in one request.
const MAX_EVENTS: usize = 2000;

fn modifiers(v: &Value) -> Modifiers {
    let has = |m: &str| {
        v["modifiers"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|x| x.as_str() == Some(m))
    };
    Modifiers {
        alt: has("alt"),
        ctrl: has("ctrl"),
        shift: has("shift"),
        mac_cmd: false,
        command: has("ctrl"),
    }
}

fn button(v: &Value) -> Result<PointerButton, String> {
    match v["button"].as_str().unwrap_or("primary") {
        "primary" | "left" => Ok(PointerButton::Primary),
        "secondary" | "right" => Ok(PointerButton::Secondary),
        "middle" => Ok(PointerButton::Middle),
        b => Err(format!("button {b:?}: primary, secondary or middle")),
    }
}

/// One request event as egui events, a frame each. A `click` is a move, a press and a release in
/// one frame, so it counts as a click however long frames take; a key is a press and a release.
fn events(v: &Value) -> Result<Vec<(Vec<Event>, Modifiers)>, String> {
    let m = modifiers(v);
    let at = || -> Result<Pos2, String> {
        match (v["x"].as_f64(), v["y"].as_f64()) {
            (Some(x), Some(y)) if x.is_finite() && y.is_finite() => Ok(Pos2::new(x as f32, y as f32)),
            _ => Err(format!("{v}: needs x and y (window points)")),
        }
    };
    let press = |pressed: bool| -> Result<Event, String> {
        Ok(Event::PointerButton {
            pos: at()?,
            button: button(v)?,
            pressed,
            modifiers: m,
        })
    };
    Ok(match v["kind"].as_str().unwrap_or_default() {
        "move" => vec![(vec![Event::PointerMoved(at()?)], m)],
        "down" => vec![(vec![Event::PointerMoved(at()?)], m), (vec![press(true)?], m)],
        "up" => vec![(vec![Event::PointerMoved(at()?)], m), (vec![press(false)?], m)],
        "click" => vec![
            (vec![Event::PointerMoved(at()?)], m),
            (vec![press(true)?, press(false)?], m),
        ],
        "scroll" => vec![
            (vec![Event::PointerMoved(at()?)], m),
            (
                vec![Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(
                        v["dx"].as_f64().unwrap_or(0.0) as f32,
                        v["dy"].as_f64().unwrap_or(0.0) as f32,
                    ),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Wheel,
                    modifiers: m,
                }],
                m,
            ),
        ],
        "key" => {
            let name = v["key"].as_str().unwrap_or_default();
            let key = egui::Key::from_name(name)
                .ok_or_else(|| format!("key {name:?}: an egui key name (Delete, K, Space, Z, ...)"))?;
            let k = |pressed| Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: m,
            };
            vec![(vec![k(true)], m), (vec![k(false)], m)]
        }
        other => {
            return Err(format!(
                "event kind {other:?}: move, down, up, click, scroll or key"
            ));
        }
    })
}

impl App {
    /// Queue a request's events (`{events: [...]}`); the reply goes out after the last is
    /// handled. Refused whole if any event is malformed.
    pub(super) fn queue_input(&mut self, msg: &Value, reply: Sender<Value>) -> Result<(), String> {
        let list = msg["events"]
            .as_array()
            .ok_or("input needs events: [{kind, x, y, ...}]")?;
        let mut all = Vec::new();
        for e in list {
            all.extend(events(e)?);
        }
        if all.is_empty() || all.len() > MAX_EVENTS {
            return Err(format!("1 to {MAX_EVENTS} events"));
        }
        let last = all.len() - 1;
        for (i, (e, m)) in all.into_iter().enumerate() {
            self.injected
                .queue
                .push_back((e, m, (i == last).then(|| reply.clone())));
        }
        Ok(())
    }

    /// Feed one queued event into this frame's input.
    pub(super) fn play_input(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if raw.events.iter().any(|e| matches!(e, Event::WindowFocused(_))) {
            self.injected.window = false;
        }
        if let Some((e, m, reply)) = self.injected.queue.pop_front() {
            self.injected.window = true;
            if m != self.injected.modifiers {
                self.injected.modifiers = m;
                raw.events.push(Event::ModifiersChanged(m));
            }
            raw.events.extend(e);
            if reply.is_some() {
                self.injected.reply = reply;
            }
            ctx.request_repaint();
        } else if self.injected.modifiers != Modifiers::NONE {
            self.injected.modifiers = Modifiers::NONE;
            raw.events.push(Event::ModifiersChanged(Modifiers::NONE));
        }
        if self.injected.window {
            raw.focused = true;
        }
    }

    /// After a frame: answer a finished batch with the layout and the editor state it left, once
    /// the window has settled (a wheel's smooth scroll runs on for a few frames).
    pub(super) fn finish_input(&mut self, ctx: &egui::Context) {
        // a wheel's smooth scroll runs on, and panels fit new content a frame later: wait (a
        // few frames at most)
        let settling = ctx.input(|i| i.smooth_scroll_delta != Vec2::ZERO) || self.shell.layout_settling;
        if settling && self.injected.reply.is_some() && self.injected.waited < MAX_SETTLE_FRAMES {
            self.injected.waited += 1;
            ctx.request_repaint();
            return;
        }
        if self.injected.queue.is_empty()
            && let Some(reply) = self.injected.reply.take()
        {
            self.injected.waited = 0;
            let state = self
                .session
                .run("editor.state", &json!({}))
                .unwrap_or(Value::Null);
            let clicked = self.injected.clicked.take();
            let _ = reply.send(json!({ "ok": { "layout": self.layout, "status": self.status, "frame": state["frame"], "keys": state["keys"].as_array().map_or(0, Vec::len), "selected_keys": self.timeline.selected, "clicked": clicked } }));
        }
    }
}
