//! Control op `click {id, button?, modifiers?}`: press a widget by its stable id (`ui_ids.rs`),
//! the way a person does: a pointer click at the middle of where it was drawn last frame, through
//! the same input queue as op `input`, answered once the window settles (the same reply as
//! `input`, plus `clicked`).
//!
//! - A widget inside a closed popup (a toolbar menu's item, `toolbar.view.front`) is reached by
//!   clicking the nearest id above it that is on screen (`toolbar.view`) first, then the item once
//!   it appears.
//! - A menu bar item (`menu.File.Export preview`; craft-menu draws those, so they have no rect)
//!   runs its command or action as choosing it does; the reply says `invoked`.
//! - A greyed-out widget, or an id that isn't there, is refused with why (and the ids nearest it).
//!
//! Every frame the widgets are published into the layout (`widgets`), for op `layout`.

use std::sync::mpsc::Sender;

use serde_json::{Value, json};

use super::App;
use super::shell::menus;
use super::ui_ids::{self, Widget};

/// Frames to wait for a popup's item to appear after its opener was clicked.
const MAX_WAIT_FRAMES: u32 = 60;

/// A click on a widget inside a popup, waiting for the popup to open.
pub struct Pending {
    id: String,
    /// The opener that was clicked.
    via: String,
    event: Value,
    reply: Sender<Value>,
    frames: u32,
}

/// The menu bar's items as widgets (no rect: craft-menu draws them) and the command each runs.
/// An item in two places (Undo history in Edit and its More) gets `#2`, as widgets do.
pub fn menu_items(session: &ac_core::Session) -> Vec<(Widget, String)> {
    let mut seen = std::collections::HashMap::<String, u32>::new();
    menus::rows(session)
        .into_iter()
        .filter(|r| r.label != "---")
        .map(|r| {
            let mut parts: Vec<String> = r.path.iter().map(|p| ui_ids::part(p)).collect();
            parts.push(ui_ids::part(&r.label));
            let id = format!("menu.{}", parts.join("."));
            let n = seen.entry(id.clone()).or_insert(0);
            *n += 1;
            let w = Widget {
                id: if *n > 1 { format!("{id}#{n}") } else { id },
                label: r.label.clone(),
                rect: None,
                enabled: true,
            };
            (w, r.command.id.clone())
        })
        .collect()
}

impl App {
    /// End of a frame: this frame's widgets (and the menu bar's items) go into the layout; a
    /// click waiting for a popup goes on.
    pub(super) fn publish_widgets(&mut self, ctx: &egui::Context) {
        let (mut list, dropped) = ui_ids::publish(ctx);
        for (mut w, command) in menu_items(&self.session) {
            w.enabled = self.enabled(&command);
            list.push(w);
        }
        self.layout["widgets"] = json!(list.iter().map(Widget::json).collect::<Vec<_>>());
        self.layout["widgets_dropped"] = json!(dropped);
        self.widgets = list;
        self.advance_click();
    }

    /// Op `click`: queue the click (answered once it's played) or refuse it.
    pub(super) fn click_request(
        &mut self,
        msg: &Value,
        reply: Sender<Value>,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        // `widget`, not `id`: a request's `id` is its own, echoed in the reply (control.rs)
        let id = msg["widget"]
            .as_str()
            .ok_or("click needs widget: a widget id (op layout lists them)")?;
        let event = json!({
            "kind": "click",
            "button": msg.get("button").cloned().unwrap_or(json!("primary")),
            "modifiers": msg.get("modifiers").cloned().unwrap_or(json!([])),
        });
        if let Some(w) = self.widgets.iter().find(|w| w.id == id).cloned() {
            if !w.enabled {
                return Err(format!("{id} ({:?}) is greyed out now", w.label));
            }
            if let Some(at) = w.target() {
                return self.click_at(&event, at, id, reply);
            }
            if id.starts_with("menu.") {
                return self.invoke_menu_item(id, reply, ctx);
            }
            return Err(format!("{id} is not on screen"));
        }
        // inside a popup: open it from the nearest id above that is on screen
        let mut opener = id;
        while let Some((up, _)) = opener.rsplit_once('.') {
            opener = up;
            if let Some(at) = self
                .widgets
                .iter()
                .find(|w| w.id == opener && w.enabled)
                .and_then(Widget::target)
            {
                let (dummy, _) = std::sync::mpsc::channel();
                self.click_at(&event_primary(), at, opener, dummy)?;
                self.pending_click = Some(Pending {
                    id: id.to_string(),
                    via: opener.to_string(),
                    event,
                    reply,
                    frames: 0,
                });
                return Ok(());
            }
        }
        let near = ac_core::suggest::closest(id, self.widgets.iter().map(|w| w.id.as_str()));
        Err(format!(
            "no widget {id:?} on screen (did you mean {}? op layout lists them)",
            near.join(", ")
        ))
    }

    /// A click at `at` through the input queue; the reply is `input`'s with `clicked`.
    fn click_at(
        &mut self,
        event: &Value,
        at: egui::Pos2,
        id: &str,
        reply: Sender<Value>,
    ) -> Result<(), String> {
        let mut e = event.clone();
        e["x"] = json!(at.x);
        e["y"] = json!(at.y);
        self.queue_input(&json!({ "events": [e] }), reply)?;
        self.injected.clicked = Some(id.to_string());
        Ok(())
    }

    /// A menu bar item: run it as choosing it does, and answer at once.
    fn invoke_menu_item(
        &mut self,
        id: &str,
        reply: Sender<Value>,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        let command = menu_items(&self.session)
            .into_iter()
            .find(|(w, _)| w.id == id)
            .map(|(_, c)| c)
            .ok_or_else(|| format!("no menu item {id:?}"))?;
        self.invoke(&craft_keymap::CommandRef::from(command.as_str()), ctx);
        // the connection waiting for it may have gone
        let _ = reply.send(json!({ "ok": { "clicked": id, "invoked": command, "status": self.status } }));
        Ok(())
    }

    /// A click waiting for its popup: click the item once it is drawn, or give up and say so.
    fn advance_click(&mut self) {
        let Some(mut p) = self.pending_click.take() else {
            return;
        };
        if !self.injected.idle() {
            self.pending_click = Some(p);
            return;
        }
        if let Some(w) = self.widgets.iter().find(|w| w.id == p.id).cloned() {
            let out = match (w.enabled, w.target()) {
                (true, Some(at)) => self.click_at(&p.event, at, &p.id, p.reply.clone()),
                (false, _) => Err(format!("{} ({:?}) is greyed out now", p.id, w.label)),
                (true, None) => Err(format!("{} is not on screen", p.id)),
            };
            if let Err(e) = out {
                let _ = p.reply.send(json!({ "error": e }));
            }
            return;
        }
        p.frames += 1;
        if p.frames > MAX_WAIT_FRAMES {
            let under: Vec<&str> = self
                .widgets
                .iter()
                .map(|w| w.id.as_str())
                .filter(|w| w.starts_with(&format!("{}.", p.via)))
                .collect();
            // the connection waiting for it may have gone
            let _ = p.reply.send(json!({ "error": format!(
                "clicked {} but {} didn't appear (there: {under:?})", p.via, p.id
            ) }));
            return;
        }
        self.pending_click = Some(p);
    }
}

fn event_primary() -> Value {
    json!({ "kind": "click", "button": "primary", "modifiers": [] })
}
