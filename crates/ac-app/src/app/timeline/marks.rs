//! Problem marks: the hook other panels use to show something on the timeline (a foot that
//! slides, a balance problem, a fill that failed). A panel owns a *source* (`"physics"`, ...) and
//! replaces its marks whenever it has new ones; the timeline draws every source's marks on a lane
//! under the ruler, with the tooltip on hover, and a click jumps to the mark.
//!
//! ```ignore
//! use crate::app::timeline::marks::Mark;
//! self.timeline.marks.set("physics", vec![
//!     Mark::new(42.0, "foot-slide", egui::Color32::from_rgb(230, 120, 60), "Left foot slides 6 cm"),
//!     Mark::new(60.0, "balance", egui::Color32::RED, "Off balance").until(72.0),
//! ]);
//! self.timeline.marks.clear("physics");
//! ```
//!
//! Marks are window state (not saved, not undone): a panel recomputes them from the document.

use std::collections::BTreeMap;

use egui::Color32;

/// Most marks one source may show; past this the rest are dropped and the last mark says how many.
pub const MAX_PER_SOURCE: usize = 5000;

/// One mark: a frame (or a span), what kind of thing it is, its colour and a tooltip.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    pub frame: f64,
    /// The span's last frame (`None`: a single frame).
    pub to: Option<f64>,
    /// A short machine name for the kind (`"foot-slide"`); shown in the tooltip's first line.
    pub kind: String,
    pub colour: Color32,
    pub tooltip: String,
}

impl Mark {
    pub fn new(frame: f64, kind: &str, colour: Color32, tooltip: &str) -> Mark {
        Mark {
            frame,
            to: None,
            kind: kind.to_string(),
            colour,
            tooltip: tooltip.to_string(),
        }
    }

    /// The mark covers `frame..=to`.
    pub fn until(mut self, to: f64) -> Mark {
        self.to = Some(to);
        self
    }

    /// Where it ends (its frame for a single-frame mark).
    pub fn end(&self) -> f64 {
        self.to.unwrap_or(self.frame).max(self.frame)
    }
}

/// Every source's marks.
#[derive(Debug, Default)]
pub struct TimelineMarks {
    by_source: BTreeMap<String, Vec<Mark>>,
}

impl TimelineMarks {
    /// Replace `source`'s marks (marks with a frame that is not a finite number are left out).
    pub fn set(&mut self, source: &str, marks: Vec<Mark>) {
        let total = marks.len();
        let mut keep: Vec<Mark> = marks
            .into_iter()
            .filter(|m| m.frame.is_finite() && m.to.is_none_or(f64::is_finite))
            .take(MAX_PER_SOURCE)
            .collect();
        if total > MAX_PER_SOURCE
            && let Some(last) = keep.last_mut()
        {
            last.tooltip = format!(
                "{}\n({} more marks from {source} not shown)",
                last.tooltip,
                total - MAX_PER_SOURCE
            );
        }
        if keep.is_empty() {
            self.by_source.remove(source);
        } else {
            self.by_source.insert(source.to_string(), keep);
        }
    }

    /// Add one mark to `source`.
    pub fn push(&mut self, source: &str, mark: Mark) {
        let mut v = self.get(source).to_vec();
        v.push(mark);
        self.set(source, v);
    }

    pub fn clear(&mut self, source: &str) {
        self.by_source.remove(source);
    }

    pub fn get(&self, source: &str) -> &[Mark] {
        self.by_source.get(source).map_or(&[], Vec::as_slice)
    }

    /// Every mark with its source, sources in name order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Mark)> {
        self.by_source
            .iter()
            .flat_map(|(s, v)| v.iter().map(move |m| (s.as_str(), m)))
    }

    pub fn is_empty(&self) -> bool {
        self.by_source.is_empty()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn sources_replace_their_own_marks() {
        let mut m = TimelineMarks::default();
        m.set("physics", vec![Mark::new(3.0, "slide", Color32::RED, "a")]);
        m.push("ai", Mark::new(9.0, "fail", Color32::RED, "b").until(12.0));
        m.set("physics", vec![Mark::new(f64::NAN, "x", Color32::RED, "bad")]);
        assert!(m.get("physics").is_empty());
        assert_eq!(m.iter().count(), 1);
        assert_eq!(m.get("ai").first().unwrap().end(), 12.0);
        m.clear("ai");
        assert!(m.is_empty());
    }
}
