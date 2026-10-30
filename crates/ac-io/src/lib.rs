//! The app's files: the animation and pose library and the commands that read and write it.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod builtins;
pub mod commands;
pub mod data_folder;
pub mod export;
pub mod export_batch;
pub mod import;
pub mod layer_tools;
pub mod layers;
pub mod library;
pub mod recovery;
pub mod reload;
mod returns;
pub mod setup;

pub use commands::SPECS;

/// Every command of this crate, for a front end's session.
pub const ALL: &[&[ac_core::CommandSpec]] = &[
    commands::SPECS,
    import::SPECS,
    layers::SPECS,
    layer_tools::SPECS,
    export::SPECS,
    // files, recovery and reloading for the window (batch A2)
    export_batch::SPECS,
    recovery::SPECS,
    reload::SPECS,
];

/// Register every command of this crate; returns the clashes (empty: all registered).
pub fn register(session: &mut ac_core::Session) -> Vec<ac_core::CoreError> {
    ALL.iter().flat_map(|s| session.register(s)).collect()
}
