//! Training data review, for developers (never shipped with the app): the retargeted takes of
//! `training/data/rig`, the quality scan's flags on them, and a reviewer's decisions in
//! `training/review/decisions.json`. No UI here: the window (`main.rs`) and the headless
//! `ac-review run` reach the same [`review::Review`].

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod decisions;
pub mod flags;
pub mod npz;
pub mod review;
pub mod scan;
pub mod store;
