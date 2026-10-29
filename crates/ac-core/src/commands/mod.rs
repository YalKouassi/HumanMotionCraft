//! ac-core's own commands. Each area is a file with its `SPECS`; [`AREAS`] is the one registration.

mod ai;
mod clipquery;
mod keys;
mod placement;
mod pose;
mod quality;
mod returns;
mod session;
mod steps;
mod timeline;
pub mod view;

use crate::command::CommandSpec;

/// Whether `spec` is one of the session's own commands (`edit.*`, `session.*`).
pub fn is_session_own(spec: &CommandSpec) -> bool {
    session::is_own(spec)
}

/// Every command area of ac-core.
pub static AREAS: &[&[CommandSpec]] = &[
    session::SPECS,
    pose::SPECS,
    keys::SPECS,
    clipquery::SPECS,
    placement::SPECS,
    ai::SPECS,
    quality::SPECS,
    view::SPECS,
    timeline::SPECS,
    steps::SPECS,
];
