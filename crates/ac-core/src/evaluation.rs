//! Motion from the optional sample clips (`posefill.samples`). Their licence allows evaluating
//! NVIDIA's technology and nothing else, and forbids making them or anything derived from them
//! available to others. So a clip made from them says so (`evaluation_only` in its file), the mark
//! follows the motion through copy and paste, saved poses, new animations and link layers, and the
//! commands that send motion out of the app refuse. This is honest marking, not protection: the
//! licence is the person's to keep.

use std::collections::BTreeMap;

use crate::Document;
use crate::clip::Clip;
use crate::error::hinted;

/// Whether the open animation, or an animation it plays through a link, is marked.
pub fn holds(doc: &Document) -> bool {
    marked(&doc.clip, &doc.links)
}

/// Whether `clip`, or an animation it plays through a link, is marked.
pub fn marked(clip: &Clip, links: &BTreeMap<String, Clip>) -> bool {
    clip.evaluation_only || links.values().any(|c| c.evaluation_only)
}

/// A link layer's motion can be baked into keys later, so the clip that links is marked too.
pub fn mark_from_links(doc: &mut Document) {
    doc.clip.evaluation_only |= doc.links.values().any(|c| c.evaluation_only);
}

/// The refusal for sending a marked animation out as `what`.
pub fn refusal(what: &str) -> String {
    hinted(
        format!(
            "{what} is blocked: the animation is marked evaluation-only, because it holds motion from NVIDIA's sample clips, whose licence doesn't allow sharing them or anything made from them"
        ),
        "use an animation without them",
    )
}

/// Refuse when the open animation is marked.
pub fn check(doc: &Document, what: &str) -> Result<(), String> {
    if holds(doc) { Err(refusal(what)) } else { Ok(()) }
}
