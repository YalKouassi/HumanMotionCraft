//! Stable ids for the window's widgets, so an agent can find and press them by name
//! (`ui_layout` lists them; control op `click {id}` presses one).
//!
//! Each frame, a widget reports itself with [`Tag::tag`] on its `Response`: an id that doesn't
//! change between runs (`toolbar.tool.rotate`, `timeline.dope`, `panel.layers.solo.0`,
//! `library.item.Mine/walk`), its label, its rect and whether it is enabled. The registry lives in
//! egui's memory, so any code that draws a widget can report it without the `App`. At the end of
//! the frame the list is complete ([`publish`]); the menu bar's items, which craft-menu draws,
//! are listed from the menu model with no rect (`menu.File.Export preview`), and `click` runs
//! them the way choosing them does.
//!
//! **Naming:** `<area>.<thing>[.<index or name>]`, lower case, the area first: `menu`, `toolbar`
//! (the viewport's), `picker`, `timeline`, `curves`, `library`, `panel.<section>`, `status`,
//! `dialog.<name>`, `window.<name>`. An id used twice in a frame gets `#2`, `#3` (and that is a
//! bug to fix where it is drawn).

use egui::{Context, Id, Rect, Response};
use serde_json::{Value, json};

/// Most widgets kept a frame (a long library list stops being listed past it, and says so).
pub const MAX_WIDGETS: usize = 4000;

/// One widget as drawn this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Widget {
    pub id: String,
    pub label: String,
    /// Where it is (window points); `None`: not on screen (a menu item of a closed menu).
    pub rect: Option<Rect>,
    pub enabled: bool,
}

impl Widget {
    /// Where a click lands: the middle of the part that can be seen.
    pub fn target(&self) -> Option<egui::Pos2> {
        self.rect.filter(|r| r.is_positive()).map(|r| r.center())
    }

    pub fn json(&self) -> Value {
        json!({
            "id": self.id,
            "label": self.label,
            "rect": self.rect.map(|r| [r.left(), r.top(), r.right(), r.bottom()]),
            "enabled": self.enabled,
        })
    }
}

#[derive(Clone, Default)]
struct Registry {
    /// This frame's widgets, as they report.
    this: Vec<Widget>,
    /// Ids seen this frame, with how many times.
    seen: std::collections::HashMap<String, u32>,
    /// More widgets reported than [`MAX_WIDGETS`] this frame.
    dropped: usize,
}

fn key() -> Id {
    Id::unique("ac-ui-ids")
}

/// Report a widget (see the module docs for the naming).
pub fn report(ctx: &Context, id: String, label: String, rect: Option<Rect>, enabled: bool) {
    ctx.data_mut(|d| {
        let reg = d.get_temp_mut_or_default::<Registry>(key());
        if reg.this.len() >= MAX_WIDGETS {
            reg.dropped += 1;
            return;
        }
        let n = reg.seen.entry(id.clone()).or_insert(0);
        *n += 1;
        let id = if *n > 1 { format!("{id}#{n}") } else { id };
        reg.this.push(Widget {
            id,
            label,
            rect,
            enabled,
        });
    });
}

/// `.tag(id, label)` on a widget's response reports it and hands the response back.
pub trait Tag: Sized {
    fn tag(self, id: impl Into<String>, label: impl Into<String>) -> Self;
}

impl Tag for Response {
    fn tag(self, id: impl Into<String>, label: impl Into<String>) -> Response {
        // the part of it that can be clicked (a widget scrolled half out of view is clipped)
        let r = self.interact_rect;
        let rect = (r.is_positive() && self.ctx.content_rect().intersects(r)).then_some(r);
        report(&self.ctx, id.into(), label.into(), rect, self.enabled());
        self
    }
}

/// End of the frame: this frame's widgets, complete, and how many didn't fit; the registry
/// starts over for the next frame.
pub fn publish(ctx: &Context) -> (Vec<Widget>, usize) {
    ctx.data_mut(|d| {
        let reg = d.get_temp_mut_or_default::<Registry>(key());
        let out = (std::mem::take(&mut reg.this), reg.dropped);
        reg.seen.clear();
        reg.dropped = 0;
        out
    })
}

/// A name or label as an id part: trailing "…" and outer spaces dropped.
pub fn part(text: &str) -> String {
    text.trim()
        .trim_end_matches('…')
        .trim_end_matches("...")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn widgets_report_once_a_frame_and_repeats_are_numbered() {
        let ctx = Context::default();
        let mut got = Vec::new();
        for _ in 0..2 {
            let mut full = ctx.run_ui(egui::RawInput::default(), |ui| {
                ui.button("Play").tag("timeline.play", "Play");
                ui.add_enabled(false, egui::Button::new("Delete"))
                    .tag("timeline.delete", "Delete");
                ui.button("Again").tag("timeline.play", "Again");
                got = publish(ui.ctx()).0;
            });
            full.textures_delta.clear();
        }
        let ids: Vec<&str> = got.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["timeline.play", "timeline.delete", "timeline.play#2"]);
        assert!(got[0].enabled && !got[1].enabled);
        assert!(got[0].target().is_some(), "on screen: it can be clicked");
        assert_eq!(part(" Export… "), "Export");
    }
}
