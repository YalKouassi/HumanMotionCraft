//! Installing the helper from the app (`text.install`): the setup `tools/textmotion/setup_textmotion.py`
//! run by [`ac_helper::setup`], which reads its numbered step lines (`== [3/6] Text encoder ...`)
//! for the progress ([`feed`]), stops it on request and carries on from where it stopped.

use std::path::Path;

use ac_helper::setup::{Installer, Spec};
use serde_json::Value;

pub use ac_helper::setup::{LOG, Need, Progress, feed};

/// The setup, from the source tree's root.
pub const SCRIPT: &str = "tools/textmotion/setup_textmotion.py";

static SPEC: Spec = Spec {
    kind: &crate::install::KIND,
    script: SCRIPT,
    python: "3.12",
    msvc: true,
    gpu_required: false,
    without_gpu: "none found: it installs, but making motion takes many minutes on the CPU",
};

/// This helper's installs.
pub static INSTALLER: Installer = Installer::new(&SPEC);

/// Fixture runs: report the install as there and never start the setup.
pub fn use_stand_in() {
    INSTALLER.use_stand_in();
}

/// What the install into `dir` needs, checked now.
pub fn requirements(assets: Option<&Path>, dir: &Path) -> Vec<Need> {
    INSTALLER.requirements(assets, dir)
}

/// What the install is doing, as `text.install` reports it.
pub fn status(dir: &Path) -> Value {
    INSTALLER.status(dir)
}

/// Whether a setup started here is still going.
pub fn running() -> bool {
    INSTALLER.running()
}

/// Start the setup into `dir` (`repair`: even when it is installed already).
pub fn start(dir: &Path, assets: Option<&Path>, repair: bool) -> Result<(), String> {
    INSTALLER.start(dir, assets, repair)
}

/// Ask the running setup to stop (it ends what it started, then itself).
pub fn stop() -> Result<(), String> {
    INSTALLER.stop()
}
