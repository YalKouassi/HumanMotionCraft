//! Where the helper is installed and what with: `install.json`, written last by
//! `tools/posefill/setup_posefill.py`. The folder is `AC_POSEFILL_DIR`, else the per-user local
//! data folder (the setup picks it the same way). The reading is [`ac_helper::install`]'s.

use std::path::{Path, PathBuf};

pub use ac_helper::install::{Install, VERSION};

/// How to install it, for refusals.
pub const HOW: &str = "install it with Install in the Pose fill panel (posefill.install), or run \
     `python tools/posefill/setup_posefill.py` from the source tree (an NVIDIA GPU is required; about 6 GB of free disk)";

/// This helper, for [`ac_helper::install`] and [`ac_helper::setup`].
pub static KIND: ac_helper::install::Kind = ac_helper::install::Kind {
    folder: "posefill",
    env: "AC_POSEFILL_DIR",
    what: "pose fill",
    how: HOW,
};

/// The install folder (`None`: no home or data folder is known).
pub fn dir() -> Option<PathBuf> {
    ac_helper::install::dir(&KIND)
}

/// The install in `dir` (`Err`: it is missing or unreadable, and how to fix it).
pub fn read(dir: &Path) -> Result<Install, String> {
    ac_helper::install::read(&KIND, dir)
}

/// The install in [`dir`].
pub fn find() -> Result<Install, String> {
    ac_helper::install::find(&KIND)
}

/// Whether the install in `dir` holds the plan the first fill builds for this GPU
/// (`<dir>/trt-cache/*.plan`), without starting the helper.
pub fn plan_built(dir: &Path) -> bool {
    std::fs::read_dir(dir.join("trt-cache"))
        .map(|d| {
            d.flatten()
                .any(|e| e.path().extension().is_some_and(|x| x == "plan"))
        })
        .unwrap_or(false)
}
