//! Where a helper is installed and what with: `install.json`, written last by its setup. The folder
//! is the helper's environment variable, else the per-user local data folder (the setup picks it
//! the same way): `%LOCALAPPDATA%/<product>/<name>` on Windows.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// One helper: where it lives and how to install it.
pub struct Kind {
    /// The folder under the product's data folder (`textmotion`, `posefill`).
    pub folder: &'static str,
    /// The environment variable naming another folder (`AC_TEXTMOTION_DIR`).
    pub env: &'static str,
    /// What it is, for refusals ("motion from text").
    pub what: &'static str,
    /// How to install it, for refusals.
    pub how: &'static str,
}

/// What a setup wrote.
#[derive(Clone, Debug, Deserialize)]
pub struct Install {
    #[serde(skip)]
    pub root: PathBuf,
    pub version: u32,
    pub python: PathBuf,
    pub helper: PathBuf,
    pub model: String,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
}

/// The `install.json` version this app reads.
pub const VERSION: u32 = 1;

/// The install folder (`None`: no home or data folder is known).
pub fn dir(kind: &Kind) -> Option<PathBuf> {
    let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(d) = var(kind.env) {
        return Some(d);
    }
    let base = if cfg!(windows) {
        var("LOCALAPPDATA").or_else(|| var("APPDATA"))?
    } else if cfg!(target_os = "macos") {
        var("HOME")?.join("Library/Application Support")
    } else {
        var("XDG_DATA_HOME").or_else(|| var("HOME").map(|h| h.join(".local/share")))?
    };
    Some(base.join(ac_core::product::NAME).join(kind.folder))
}

/// The install in `dir` (`Err`: it is missing or unreadable, and how to fix it).
pub fn read(kind: &Kind, dir: &Path) -> Result<Install, String> {
    let path = dir.join("install.json");
    let text = std::fs::read_to_string(&path).map_err(|_| {
        ac_core::error::hinted(
            format!("{} is not installed (no {})", kind.what, path.display()),
            kind.how,
        )
    })?;
    let mut install: Install = serde_json::from_str(&text).map_err(|e| {
        ac_core::error::hinted(
            format!("{} can't be read: {e}", path.display()),
            "run the setup again to rewrite it",
        )
    })?;
    if install.version != VERSION {
        return Err(ac_core::error::hinted(
            format!(
                "{} is version {}; this app reads version {VERSION}",
                path.display(),
                install.version
            ),
            "run the setup from this app's source tree again",
        ));
    }
    for (what, p) in [("Python", &install.python), ("helper", &install.helper)] {
        if !p.is_file() {
            return Err(ac_core::error::hinted(
                format!("the install's {what} {} is missing", p.display()),
                "run the setup again",
            ));
        }
    }
    install.root = dir.to_path_buf();
    Ok(install)
}

/// The install in [`dir`].
pub fn find(kind: &Kind) -> Result<Install, String> {
    let d = dir(kind).ok_or_else(|| {
        ac_core::error::hinted(
            format!(
                "{} is not installed (no data folder is known: set {})",
                kind.what, kind.env
            ),
            kind.how,
        )
    })?;
    read(kind, &d)
}
