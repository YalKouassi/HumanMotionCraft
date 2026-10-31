//! Where the helper is installed and what with: `install.json`, written by
//! `tools/textmotion/setup_textmotion.py`. The folder is `AC_TEXTMOTION_DIR`, else the per-user
//! local data folder (the setup picks it the same way). The reading is [`ac_helper::install`]'s.

use std::path::{Path, PathBuf};

pub use ac_helper::install::{Install, VERSION};

/// How to install it, for refusals.
pub const HOW: &str = "install it with Install in the Motion from text panel (text.install), or run \
     `python tools/textmotion/setup_textmotion.py` from the source tree (an NVIDIA GPU is best; about 35 GB of free disk)";

/// This helper, for [`ac_helper::install`] and [`ac_helper::setup`].
pub static KIND: ac_helper::install::Kind = ac_helper::install::Kind {
    folder: "textmotion",
    env: "AC_TEXTMOTION_DIR",
    what: "motion from text",
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
