//! Installing the helper from the app (`posefill.install`): the setup
//! `tools/posefill/setup_posefill.py` run by [`ac_helper::setup`], which reads its numbered step
//! lines for the progress, stops it on request and carries on from where it stopped. The setup
//! needs an NVIDIA GPU (the engine is a TensorRT plan), so the install refuses without one.

use std::path::Path;

use ac_helper::setup::{Installer, Spec};
use serde::Serialize;
use serde_json::Value;

pub use ac_helper::setup::{LOG, Need, Progress, feed};

/// The setup, from the source tree's root.
pub const SCRIPT: &str = "tools/posefill/setup_posefill.py";

static SPEC: Spec = Spec {
    kind: &crate::install::KIND,
    script: SCRIPT,
    python: "3.12",
    msvc: false,
    gpu_required: true,
    without_gpu: "none found: the engine runs as a TensorRT plan on an NVIDIA GPU, and there is no CPU or other-GPU path",
};

/// This helper's installs.
pub static INSTALLER: Installer = Installer::new(&SPEC);

/// One licence installing accepts (the setup prints and records the same list).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Licence {
    pub part: &'static str,
    pub licence: &'static str,
    pub url: &'static str,
}

/// What installing accepts, for the panel and `posefill.install`'s answer: the engine's code,
/// the model, the runtime it needs, and the libraries beside them. The setup records them in
/// `licences.txt` in the install folder.
pub const LICENCES: &[Licence] = &[
    Licence {
        part: "the engine's code",
        licence: "Apache License 2.0",
        url: "https://www.apache.org/licenses/LICENSE-2.0",
    },
    Licence {
        part: "the motion model and the rig file it poses",
        licence: "NVIDIA Open Model Agreement",
        url: "https://www.nvidia.com/en-us/agreements/enterprise-software/nvidia-open-model-agreement/",
    },
    Licence {
        part: "TensorRT",
        licence: "NVIDIA Software Development Kit licence agreement with the TensorRT supplement",
        url: "https://docs.nvidia.com/deeplearning/tensorrt/latest/reference/sla.html",
    },
    Licence {
        part: "the CUDA runtime",
        licence: "NVIDIA CUDA Toolkit EULA",
        url: "https://docs.nvidia.com/cuda/eula/index.html",
    },
    Licence {
        part: "Warp",
        licence: "Apache License 2.0",
        url: "https://github.com/NVIDIA/warp/blob/v1.17.0/LICENSE.md",
    },
    Licence {
        part: "OpenUSD",
        licence: "Tomorrow Open Source Technology License 1.0",
        url: "https://github.com/PixarAnimationStudios/OpenUSD/blob/release/LICENSE.txt",
    },
    Licence {
        part: "numpy and scipy",
        licence: "BSD 3-Clause",
        url: "https://numpy.org/doc/stable/license.html",
    },
];

/// What the install leaves out, for the panel.
pub const NOT_INSTALLED: &str = "The engine's sample motion clips are evaluation-only and aren't part of this install, which checks they are absent. They are a separate optional download once it is installed.";

/// Fixture runs: report the install as there and never start the setup.
pub fn use_stand_in() {
    INSTALLER.use_stand_in();
}

/// What the install into `dir` needs, checked now.
pub fn requirements(assets: Option<&Path>, dir: &Path) -> Vec<Need> {
    INSTALLER.requirements(assets, dir)
}

/// What the install is doing, as `posefill.install` reports it.
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
