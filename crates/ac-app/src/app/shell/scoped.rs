//! Keys that work only somewhere: one keymap holds every key, and a key bound in a context (the
//! text before `": "` in an action's keys, e.g. `"Viewport: F"`) runs only while that context is
//! on, ahead of the same key's application-wide binding. So Ctrl+C copies the selected keys over
//! the timeline and the pose elsewhere, and the arrows walk the keys while the timeline has the
//! keyboard and step frames otherwise. [`App::contexts`] says which contexts are on each frame;
//! [`CONTEXTS`] explains them in Help > Keyboard shortcuts.
//!
//! **Adding a context key:** a row in `ACTIONS` whose key names the context, and an arm in
//! [`App::act_scoped`]. Two contexts that can be on together must not share a key (the keymap
//! test `spec_shortcuts_parse_and_defaults_dont_clash` checks).

use craft_keymap::Scopes;
use serde_json::json;

use super::super::App;

/// The pointer is over the 3D view.
pub const VIEWPORT: &str = "Viewport";
/// The pointer is over the timeline, or it has the keyboard.
pub const TIMELINE: &str = "Timeline";
/// The timeline has the keyboard (it was clicked).
pub const TIMELINE_FOCUS: &str = "Timeline focus";
/// The timeline is on (pointer or keyboard) and keys are selected.
pub const TIMELINE_SELECTION: &str = "Timeline selection";
/// The timeline is on (pointer or keyboard) and keys were copied.
pub const TIMELINE_CLIPBOARD: &str = "Timeline clipboard";
/// The library has the keyboard (an item was clicked last).
pub const LIBRARY: &str = "Library";

/// Every context and when it is on, for the shortcuts sheet.
pub const CONTEXTS: &[(&str, &str)] = &[
    (VIEWPORT, "the pointer is over the 3D view"),
    (
        TIMELINE,
        "the pointer is over the timeline, or the timeline has the keyboard",
    ),
    (
        TIMELINE_FOCUS,
        "the timeline has the keyboard (click it; click elsewhere to leave)",
    ),
    (
        TIMELINE_SELECTION,
        "Timeline, with keys selected (else Ctrl+C copies the pose)",
    ),
    (
        TIMELINE_CLIPBOARD,
        "Timeline, with keys copied (else Ctrl+V pastes the pose)",
    ),
    (LIBRARY, "the library has the keyboard (an item was clicked last)"),
];

/// The keymap's scopes: the contexts above; a key bound in one never runs outside it.
pub fn scopes() -> Scopes {
    CONTEXTS
        .iter()
        .fold(Scopes::new().with_panel_fallthrough(false), |s, (c, _)| {
            s.with_context(c)
        })
}

impl App {
    /// The contexts on now (from where the pointer and the keyboard were last frame).
    pub(in crate::app) fn contexts(&self) -> Vec<&'static str> {
        let tl = &self.timeline;
        let mut on = Vec::new();
        if self.view.hovered {
            on.push(VIEWPORT);
        }
        if tl.focused || tl.hovered {
            on.push(TIMELINE);
            if !tl.selected.is_empty() {
                on.push(TIMELINE_SELECTION);
            }
            if self.session.document.key_clipboard.is_some() {
                on.push(TIMELINE_CLIPBOARD);
            }
        }
        if tl.focused {
            on.push(TIMELINE_FOCUS);
        }
        let nav = &self.files.nav;
        if nav.focus && nav.dialog.is_none() && !tl.focused {
            on.push(LIBRARY);
        }
        on
    }

    /// The context actions (camera views, timeline keys, library keys); false: not one of them.
    pub(in crate::app) fn act_scoped(&mut self, id: &str, ctx: &egui::Context) -> bool {
        let view = |name: &'static str| Some(name);
        let preset = match id {
            "app.view_front" => view("front"),
            "app.view_back" => view("back"),
            "app.view_right" => view("side"),
            "app.view_left" => view("left"),
            "app.view_top" => view("top"),
            "app.view_bottom" => view("bottom"),
            "app.view_three_quarter" => view("three-quarter"),
            _ => None,
        };
        if let Some(p) = preset {
            if let Err(e) = self.view_preset(p) {
                self.status = e;
            }
            return true;
        }
        match id {
            "app.frame_selection" => {
                let aspect = self
                    .shell
                    .viewport_rect
                    .map_or(1.5, |r| r.width() / r.height().max(1.0));
                self.frame_selection(aspect);
            }
            "app.ortho" => self.toggle_ortho(),
            "app.follow" => self.view.follow = !self.view.follow,
            "app.copy_keys" => self.copy_keys(ctx),
            "app.paste_keys" => self.paste_keys(false),
            "app.paste_keys_mirrored" => self.paste_keys(true),
            "app.select_next_key" => self.walk_keys(Walk::Next, false),
            "app.select_prev_key" => self.walk_keys(Walk::Prev, false),
            "app.extend_next_key" => self.walk_keys(Walk::Next, true),
            "app.extend_prev_key" => self.walk_keys(Walk::Prev, true),
            "app.select_first_key" => self.walk_keys(Walk::First, false),
            "app.select_last_key" => self.walk_keys(Walk::Last, false),
            "app.extend_first_key" => self.walk_keys(Walk::First, true),
            "app.extend_last_key" => self.walk_keys(Walk::Last, true),
            "app.clear_keys" => {
                self.timeline.selected.clear();
                self.timeline.part = None;
            }
            "app.range_in" | "app.range_out" => {
                let doc = &self.session.document;
                let len = doc.clip.length_with(&doc.links).max(1);
                let f = doc.key_frame();
                self.timeline.playback.set_point(id == "app.range_out", f, len);
            }
            "app.lib_prev" | "app.lib_next" | "app.lib_open" | "app.lib_rename" | "app.lib_delete" => {
                self.library_key(id);
            }
            _ => return false,
        }
        true
    }

    /// Select the next, previous, first or last key (`extend`: add it to the selection) and put
    /// the playhead on it.
    fn walk_keys(&mut self, walk: Walk, extend: bool) {
        let frames: Vec<i64> = self
            .session
            .document
            .clip
            .keys()
            .iter()
            .map(|k| k.frame)
            .collect();
        let now = self.session.document.frame;
        let to = match walk {
            Walk::Next => frames.iter().copied().find(|&f| f as f64 > now + 1e-9),
            Walk::Prev => frames.iter().rev().copied().find(|&f| (f as f64) < now - 1e-9),
            Walk::First => frames.first().copied(),
            Walk::Last => frames.last().copied(),
        };
        let Some(f) = to else {
            self.status = match walk {
                Walk::Next => format!("no key after frame {}", now.round()),
                Walk::Prev => format!("no key before frame {}", now.round()),
                Walk::First | Walk::Last => "no keys yet".into(),
            };
            return;
        };
        let tl = &mut self.timeline;
        if !extend {
            tl.selected.clear();
            tl.part = None;
        }
        tl.selected.insert(f);
        self.playing = false;
        self.run("frame.set", json!({ "frame": f }));
    }
}

#[derive(Clone, Copy)]
enum Walk {
    Next,
    Prev,
    First,
    Last,
}
