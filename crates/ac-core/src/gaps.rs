//! The gaps between your keys as an AI fill sees them: how long each one is, whether the model has
//! filled it, and whether it can (a gap longer than the model takes is left to the spline). The
//! inspector reads these before anything is clicked; `ai.gaps`, `ai.fill_all` and `ai.toggle`
//! act on them.

use serde::Serialize;

use crate::clip::KeySource;
use crate::document::Document;
use crate::edits;

/// The stretch between two neighbouring keys of yours.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Gap {
    /// Your key before the gap.
    pub from: i64,
    /// Your key after it.
    pub to: i64,
    /// Frames strictly between them.
    pub frames: i64,
    /// AI keys fill it now.
    pub filled: bool,
    /// Why the model can't fill it, in words (`None`: it can).
    pub refused: Option<String>,
}

/// Most frames the loaded model fills in one gap (`None`: no model is loaded).
pub fn max_gap(doc: &Document) -> Option<i64> {
    doc.ai.as_ref().map(|m| {
        i64::try_from(m.info().max_gap)
            .unwrap_or(i64::MAX)
            .min(edits::MAX_FILL)
    })
}

/// Every gap between neighbouring manual keys of the edited layer, in frame order.
pub fn gaps(doc: &Document) -> Vec<Gap> {
    let mut manual = edits::manual_frames(&doc.clip);
    manual.sort_unstable();
    manual.dedup();
    let ai: Vec<i64> = doc
        .clip
        .keys()
        .iter()
        .filter(|k| k.source == KeySource::Ai)
        .map(|k| k.frame)
        .collect();
    let max = max_gap(doc);
    manual
        .windows(2)
        .filter_map(|w| match w {
            [a, b] => Some((*a, *b)),
            _ => None,
        })
        .map(|(a, b)| {
            let frames = b - a - 1;
            let refused = match max {
                None => Some("no AI model is loaded (assets/models)".to_string()),
                Some(_) if frames < 1 => Some("the keys are next to each other: nothing to fill".to_string()),
                Some(m) if frames > m => Some(format!("gap {frames} frames (max {m})")),
                Some(_) => None,
            };
            Gap {
                from: a,
                to: b,
                frames,
                filled: ai.iter().any(|&f| a < f && f < b),
                refused,
            }
        })
        .collect()
}

/// The gap the playhead is in (your keys `from <= frame < to`), if it is between two of them.
pub fn gap_at(doc: &Document, frame: i64) -> Option<Gap> {
    gaps(doc).into_iter().find(|g| g.from <= frame && frame < g.to)
}
