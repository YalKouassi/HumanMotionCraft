//! Where poses can be borrowed from: a front end's library of animations, as the core sees it.
//!
//! The core never reads files, so a feature that needs a pose from the library (anchor poses
//! inside a long AI fill, `crate::anchors`) asks the [`PoseSource`] the front end set on
//! [`crate::document::Document::pose_source`] (`ac-io` serves the session's library). Without one
//! the feature says so and refuses (Rule 1), it never guesses.

use std::sync::Arc;

use crate::clip::Clip;

/// One animation of the library: its id (`<group>/<name>`) and the clip as the file holds it.
#[derive(Debug, Clone)]
pub struct SourceClip {
    pub id: String,
    pub clip: Arc<Clip>,
}

/// What a source gives: the clips it could read, in id order, and a note for each it couldn't
/// (never silently fewer).
#[derive(Debug, Clone, Default)]
pub struct Sourced {
    pub clips: Vec<SourceClip>,
    pub notes: Vec<String>,
}

pub trait PoseSource: Send + Sync + std::fmt::Debug {
    /// Where the clips come from, in words ("the library at C:/.../library").
    fn name(&self) -> String;

    /// Every animation of the source. The same source in the same state gives the same clips in
    /// the same order (the features on top of it are deterministic).
    fn clips(&self) -> Result<Sourced, String>;
}
