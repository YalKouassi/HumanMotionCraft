//! The gaps between your keys as an AI fill sees them: how long each one is, whether the model has
//! filled it, and whether it can (a gap longer than the model takes gets anchor poses from the
//! library, `crate::anchors`; without a library, or past [`crate::anchors::MAX_LONG`], it is left
//! to the spline). The inspector reads these before anything is clicked; `ai.gaps`, `ai.fill_all`
//! and `ai.toggle` act on them.

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
    /// Over the model's limit: how many anchor poses from the library split it (`None`: the
    /// model takes it in one, or it is refused).
    pub anchors: Option<i64>,
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
    let library = doc.pose_source.is_some();
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
                Some(m) if frames > m && !library => Some(format!(
                    "gap {frames} frames (max {m}) and no library to take anchor poses from"
                )),
                Some(_) if frames > crate::anchors::MAX_LONG => Some(format!(
                    "gap {frames} frames (max {} with anchor poses): key a pose inside it",
                    crate::anchors::MAX_LONG
                )),
                Some(_) => None,
            };
            let anchors = match (max, &refused) {
                (Some(m), None) if frames > m => Some(crate::anchors::frames(a, b, m).len() as i64),
                _ => None,
            };
            Gap {
                from: a,
                to: b,
                frames,
                filled: ai.iter().any(|&f| a < f && f < b),
                refused,
                anchors,
            }
        })
        .collect()
}

/// The gap the playhead is in (your keys `from <= frame < to`), if it is between two of them.
pub fn gap_at(doc: &Document, frame: i64) -> Option<Gap> {
    gaps(doc).into_iter().find(|g| g.from <= frame && frame < g.to)
}

/// The passes `ai.fill_all` makes with a model that takes several keys in a window
/// ([`crate::ai::ModelInfo::multi_key`], roadmap 142000): `spans` are the gaps it is about to
/// fill, `(from, to)` in frame order; neighbouring ones (sharing a key) join one pass while the
/// stretch from the pass's first key to the gap's last holds at most `max_gap` frames. Each
/// pass is its keys in order; a gap on its own is a pass of two keys.
pub fn passes(spans: &[(i64, i64)], max_gap: i64) -> Vec<Vec<i64>> {
    let mut out: Vec<Vec<i64>> = Vec::new();
    for &(a, b) in spans {
        match out.last_mut() {
            Some(last) if last.last() == Some(&a) && last.first().is_some_and(|&f| b - f - 1 <= max_gap) => {
                last.push(b);
            }
            _ => out.push(vec![a, b]),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::passes;

    #[test]
    fn neighbouring_gaps_join_a_pass_while_the_stretch_fits_the_window() {
        // 0–10 and 10–20 share key 10 and hold 19 frames: one pass; 20–50 would make 49
        assert_eq!(
            passes(&[(0, 10), (10, 20), (20, 50)], 45),
            vec![vec![0, 10, 20], vec![20, 50]]
        );
        // three short gaps in one pass, the fourth starts another
        assert_eq!(
            passes(&[(0, 5), (5, 12), (12, 20), (20, 60), (60, 70)], 45),
            vec![vec![0, 5, 12, 20], vec![20, 60], vec![60, 70]]
        );
        // gaps that don't share a key never join
        assert_eq!(passes(&[(0, 10), (20, 30)], 45), vec![vec![0, 10], vec![20, 30]]);
        assert!(passes(&[], 45).is_empty());
    }
}
