//! The core: the rig, quaternions, forward kinematics, the document, and the command
//! registry and session every front end (UI, CLI, MCP, fixtures) goes through.
//!
//! Headless and deterministic: no UI, GPU, I/O dialogs, clocks or unseeded randomness.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod ai;
pub mod anatomy;
pub mod anchors;
mod batch;
pub mod character;
pub mod clip;
pub mod clipfile;
pub mod command;
pub mod commands;
pub mod contact;
pub mod critique;
pub mod diff;
pub mod document;
pub mod ease;
pub mod editor;
pub mod edits;
pub mod error;
pub mod filldiag;
pub mod footroll;
pub mod fullbody;
pub mod gait;
pub mod gaps;
pub mod hash;
mod history;
pub mod ik;
pub mod jointmap;
pub mod keyops;
pub mod markers;
pub mod metrics;
pub mod outputs;
pub mod parts;
pub mod physics;
pub mod placement;
pub mod posequery;
pub mod posesource;
pub mod possible;
pub mod problems;
pub mod product;
pub mod quat;
pub mod ranges;
pub mod retarget;
pub mod retime;
pub mod rig;
pub mod session;
pub mod suggest;
pub mod travel;
pub mod weight;

pub use command::{CommandSpec, Form, Kind, ParamSpec};
pub use document::{Document, Pose, ProjectFile};
pub use error::CoreError;
pub use rig::Skeleton;
pub use session::Session;
