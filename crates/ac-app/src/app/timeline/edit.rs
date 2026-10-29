//! Key edits the timeline makes from the pointer and the keyboard, each one undo step: moving and
//! deleting the selected keys (or, on a dope-sheet row, one part's keys), setting their
//! interpolation, copy and paste (Ctrl+C, Ctrl+V; Ctrl+Shift+V pastes mirrored). The keys
//! (copy and paste, walking the keys, I / O) are context keys in the keymap
//! (`shell/scoped.rs`), which call in here.

use std::collections::BTreeSet;

use ac_core::clip::Interp;
use serde_json::json;

use super::glyphs;
use crate::app::App;

/// "1 key", "3 keys".
fn n_keys(n: usize) -> String {
    if n == 1 {
        "1 key".into()
    } else {
        format!("{n} keys")
    }
}

impl App {
    /// While the timeline has the keyboard, Tab, the arrows and Escape stay with it (egui would
    /// move the focus with them); what they do is in the keymap (`shell/scoped.rs`).
    pub(super) fn keyboard(&mut self, ui: &egui::Ui, resp: &egui::Response) {
        if resp.has_focus() {
            let filter = egui::EventFilter {
                tab: true,
                horizontal_arrows: true,
                vertical_arrows: false,
                escape: true,
            };
            ui.memory_mut(|m| m.set_focus_lock_filter(resp.id, filter));
        }
    }

    /// The keys an action works on: the selection, else the key on the playhead.
    fn chosen(&self) -> Vec<i64> {
        if !self.timeline.selected.is_empty() {
            return self.timeline.selected.iter().copied().collect();
        }
        let f = self.session.document.key_frame();
        let on = self.session.document.clip.keys().iter().any(|k| k.frame == f);
        if on { vec![f] } else { Vec::new() }
    }

    pub(in crate::app) fn copy_keys(&mut self, ctx: &egui::Context) {
        let frames = self.chosen();
        if frames.is_empty() {
            self.status = "nothing to copy: select keys first".into();
            return;
        }
        if let Some(r) = self.run("keys.copy", json!({ "frames": frames })) {
            let whole = if self.timeline.part.is_some() {
                " (whole keys)"
            } else {
                ""
            };
            self.status = format!(
                "copied {} keys{whole}: Ctrl+V pastes at the playhead",
                r["copied"]
            );
            // the system clipboard gets a line too, so Ctrl+V reaches the window as a paste
            ctx.copy_text(format!("{} keys {frames:?}", ac_core::product::NAME));
        }
    }

    pub(in crate::app) fn paste_keys(&mut self, mirror: bool) {
        if self.session.document.key_clipboard.is_none() {
            self.status = "no keys copied yet: select keys and press Ctrl+C".into();
            return;
        }
        let at = self.session.document.key_frame().max(0);
        let n = self
            .session
            .document
            .key_clipboard
            .as_ref()
            .map_or(0, |k| k.len());
        let how = if mirror { " mirrored" } else { "" };
        self.run(
            "edit.begin",
            json!({ "label": format!("Paste {}{how} at {at}", n_keys(n)) }),
        );
        let r = self.run("keys.paste", json!({ "at": at, "mirror": mirror }));
        self.run("edit.end", json!({}));
        if let Some(r) = r {
            let frames: BTreeSet<i64> = r["frames"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_i64())
                .collect();
            let replaced = r["replaced"].as_array().map_or(0, Vec::len);
            self.status = format!("pasted {} keys{how} at {at}; {replaced} replaced", frames.len());
            self.timeline.selected = frames;
            self.timeline.part = None;
        }
    }

    /// Delete the selected keys (or the one on the playhead), as one undo step; on a dope-sheet
    /// row, only that part is taken out of them.
    pub(in crate::app) fn delete_keys(&mut self) {
        if let Some(part) = self.timeline.part
            && !self.timeline.selected.is_empty()
        {
            let frames: Vec<i64> = self.timeline.selected.iter().copied().collect();
            self.run(
                "edit.begin",
                json!({ "label": format!("Remove {part} from {}", n_keys(frames.len())) }),
            );
            for f in &frames {
                if self
                    .run("key.remove_parts", json!({ "frame": f, "parts": [part] }))
                    .is_none()
                {
                    break;
                }
            }
            self.run("edit.end", json!({}));
            self.timeline.selected.clear();
            return;
        }
        if self.timeline.selected.is_empty() {
            self.run("key.delete", json!({}));
            return;
        }
        let frames: Vec<i64> = self.timeline.selected.iter().copied().collect();
        if self.run("keys.delete", json!({ "frames": frames })).is_some() {
            self.timeline.selected.clear();
        }
    }

    /// Set the interpolation of the chosen keys (one undo step); the playhead comes back.
    pub(super) fn set_interp(&mut self, interp: Interp) {
        let frames = self.chosen();
        if frames.is_empty() {
            return;
        }
        let back = self.session.document.frame;
        self.run(
            "edit.begin",
            json!({ "label": format!("{} on {}", glyphs::label(interp), n_keys(frames.len())) }),
        );
        for f in &frames {
            self.run("frame.set", json!({ "frame": f }));
            if self
                .run("key.interp", json!({ "interp": interp.name() }))
                .is_none()
            {
                break;
            }
        }
        self.run("frame.set", json!({ "frame": back }));
        self.run("edit.end", json!({}));
    }

    /// Run `steps` (command, params) as one undo step named `label`; if one is refused, the
    /// ones before it are undone and the refusal stays in the status line. Returns whether all ran.
    fn all_or_nothing(&mut self, label: String, steps: Vec<(&str, serde_json::Value)>) -> bool {
        self.run("edit.begin", json!({ "label": label }));
        let mut ok = true;
        for (id, params) in steps {
            if self.run(id, params).is_none() {
                ok = false;
                break;
            }
        }
        let changed = self
            .run("edit.end", json!({}))
            .is_some_and(|v| v["changed"].as_bool() == Some(true));
        if !ok && changed {
            let why = self.status.clone();
            self.run("edit.undo", json!({}));
            self.status = why;
        }
        ok
    }

    /// Move the selected keys `by` frames, as one undo step; refused as a whole (nothing moves)
    /// if any can't go (off the start, or onto a key that stays).
    pub(super) fn move_keys(&mut self, by: i64) {
        if by == 0 || self.timeline.selected.is_empty() {
            return;
        }
        let mut frames: Vec<i64> = self.timeline.selected.iter().copied().collect();
        let staying: BTreeSet<i64> = self
            .session
            .document
            .clip
            .keys()
            .iter()
            .map(|k| k.frame)
            .filter(|f| !self.timeline.selected.contains(f))
            .collect();
        if let Some(f) = frames
            .iter()
            .find(|f| **f + by < 0 || staying.contains(&(**f + by)))
        {
            self.status = if f + by < 0 {
                format!("can't move key {f} before frame 0")
            } else {
                format!("can't move key {f} onto the key at {}", f + by)
            };
            return;
        }
        if by > 0 {
            frames.reverse(); // the last first, so no key lands on one still to move
        }
        let steps = frames
            .iter()
            .map(|f| ("key.move", json!({ "from": f, "to": f + by })))
            .collect();
        if self.all_or_nothing(format!("Move {} by {by}", n_keys(frames.len())), steps) {
            self.timeline.selected = frames.iter().map(|f| f + by).collect();
        }
    }

    /// Move one part's keys (a dope-sheet row) on the selected frames `by` frames, as one undo
    /// step. A key there that keys other parts keeps them; a key the part lands on takes it.
    /// Refused as a whole when the part would land on a key of that part that stays.
    pub(super) fn move_part_keys(&mut self, by: i64, part: &'static str) {
        if by == 0 || self.timeline.selected.is_empty() {
            return;
        }
        let mut frames: Vec<i64> = self.timeline.selected.iter().copied().collect();
        let sk = self.session.document.skeleton.clone();
        let staying: BTreeSet<i64> = self
            .session
            .document
            .clip
            .keys()
            .iter()
            .filter(|k| ac_core::parts::rows(&sk, k.only.as_deref()).contains(&part))
            .map(|k| k.frame)
            .filter(|f| !self.timeline.selected.contains(f))
            .collect();
        if let Some(f) = frames
            .iter()
            .find(|f| **f + by < 0 || staying.contains(&(**f + by)))
        {
            self.status = if f + by < 0 {
                format!("can't move the {part} key {f} before frame 0")
            } else {
                format!("can't move the {part} key {f} onto the {part} key at {}", f + by)
            };
            return;
        }
        if by > 0 {
            frames.reverse();
        }
        let steps = frames
            .iter()
            .map(|f| {
                (
                    "key.move_parts",
                    json!({ "from": f, "to": f + by, "parts": [part] }),
                )
            })
            .collect();
        if self.all_or_nothing(format!("Move {part} on {} by {by}", n_keys(frames.len())), steps) {
            self.timeline.selected = frames.iter().map(|f| f + by).collect();
        }
    }
}
