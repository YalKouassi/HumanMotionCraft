//! What the optional Python helpers (motion from text, `ac-textmotion`; pose-to-pose fills,
//! `ac-posefill`) share, so each crate holds only its own protocol and commands:
//!
//! - [`install`]: where a helper is installed (`<data folder>/<product>/<name>`, or an environment
//!   variable) and `install.json`, which its setup writes last;
//! - [`setup`]: running that setup from the app as a child process with its numbered step lines
//!   read for the progress, Stop, and carrying on;
//! - [`uv_fetch`]: fetching `uv` when the person has none, so no Python need be installed first;
//! - [`process`]: the helper process itself, one JSON request a line on its stdin, replies with the
//!   request's id on its stdout;
//! - [`slot`]: the one engine a process holds behind a lock, its progress line, and the results
//!   kept by their request so a front end can make them off its UI thread first;
//! - [`frames`]: the frame-rate fit between the animation and the model, and reading a reply's
//!   frames as rotations and roots.
//!
//! No UI; threads and child processes only, which is why this is not part of `ac-core`.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod frames;
pub mod install;
pub mod process;
pub mod setup;
pub mod slot;
pub mod uv_fetch;

/// What an engine says about itself.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    /// The process is up.
    pub running: bool,
    /// The model is in memory (the next request starts at once).
    pub loaded: bool,
    /// The model's name.
    pub model: String,
}
