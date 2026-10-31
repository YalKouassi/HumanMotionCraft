//! One session setup for every front end: the app, the CLI, MCP, fixtures and tests all build
//! their session here, so they run the same commands over the same rig, limits and library.
//!
//! ```ignore
//! let built = SessionBuilder::new(&assets)
//!     .library(&library)
//!     .ai(ac_ai::shared(&assets))
//!     .build()?;
//! for note in &built.notes { /* show it: Rule 1 */ }
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ac_core::ai::{Inbetweener, ModelSource};
use ac_core::{CommandSpec, Document, Session, Skeleton};
use serde_json::json;

/// A loaded in-betweener, or none installed.
pub type Model = Option<Arc<dyn Inbetweener>>;

/// Builds a [`Session`] over `<assets>/rig`: every command of `ac-core` and `ac-io` (plus any
/// [`SessionBuilder::commands`]), the joint limits, the library and the AI model.
pub struct SessionBuilder {
    assets: PathBuf,
    library: Option<PathBuf>,
    limits: bool,
    ai: Result<Model, String>,
    models: Option<Arc<dyn ModelSource>>,
    install: bool,
    strict: bool,
    extra: Vec<&'static [CommandSpec]>,
}

/// A built session and what the person should be told about it (a missing model, a library that
/// could not be installed). Empty `notes`: everything loaded.
pub struct Built {
    pub session: Session,
    pub notes: Vec<String>,
    /// How many items were copied when the built-in library was installed into an empty one.
    pub installed: Option<u64>,
    /// What updating an existing library's built-ins changed (`builtins::summary`), if anything.
    pub synced: Option<String>,
}

impl SessionBuilder {
    /// The rig and joint limits from `assets`, no library, no AI model.
    pub fn new(assets: &Path) -> SessionBuilder {
        SessionBuilder {
            assets: assets.to_path_buf(),
            library: None,
            limits: true,
            ai: Ok(None),
            models: None,
            install: true,
            strict: false,
            extra: Vec::new(),
        }
    }

    /// The library folder. An empty one gets the built-in library installed (see
    /// [`SessionBuilder::install`]), and the user's part sets are read from it.
    pub fn library(mut self, dir: &Path) -> SessionBuilder {
        self.library = Some(dir.to_path_buf());
        self
    }

    /// The AI model, as `ac_ai::shared(&assets)` loads it. An error becomes a note: AI fill then
    /// says there is no model, and the rest of the session works.
    pub fn ai(mut self, model: Result<Model, String>) -> SessionBuilder {
        self.ai = model;
        self
    }

    /// Where other models come from (`ac_ai::models::Models`): `ai.models` lists them and
    /// `ai.model` picks one. Without it a session keeps the one model it was given.
    pub fn models(mut self, source: Arc<dyn ModelSource>) -> SessionBuilder {
        self.models = Some(source);
        self
    }

    /// Without the joint limits (`doc.limits` stays empty).
    pub fn limits(mut self, on: bool) -> SessionBuilder {
        self.limits = on;
        self
    }

    /// Whether an empty library gets the built-ins installed (default: yes).
    pub fn install(mut self, on: bool) -> SessionBuilder {
        self.install = on;
        self
    }

    /// A library that can't be installed or read is an error instead of a note (the CLI and
    /// fixtures: a half-made library would fail later, further from the cause).
    pub fn strict(mut self) -> SessionBuilder {
        self.strict = true;
        self
    }

    /// More commands (a front end's own). A clash with a registered id is an error.
    pub fn commands(mut self, specs: &'static [CommandSpec]) -> SessionBuilder {
        self.extra.push(specs);
        self
    }

    pub fn build(self) -> Result<Built, String> {
        let rig_path = self.assets.join("rig/default_rig.json");
        let rig = std::fs::read_to_string(&rig_path)
            .map_err(|e| format!("can't read the rig {}: {e}", rig_path.display()))?;
        let mut doc = Document::new(Skeleton::from_json(&rig).map_err(|e| e.to_string())?);
        doc.library = self.library.clone();
        doc.assets = Some(self.assets.clone());
        // the library's clips, for features that borrow a pose (anchors inside a long AI fill)
        doc.pose_source = self.library.as_deref().map(|l| {
            Arc::new(crate::anchorlib::LibraryPoses::new(l)) as Arc<dyn ac_core::posesource::PoseSource>
        });
        if self.limits {
            let path = self.assets.join("rig/limits.json");
            let limits = std::fs::read_to_string(&path)
                .map_err(|e| format!("can't read the joint limits {}: {e}", path.display()))?;
            doc.limits = Some(Arc::new(ac_core::fullbody::Limits::from_json(
                &limits,
                &doc.skeleton,
            )?));
        }
        let mut notes = Vec::new();
        let mut installed = None;
        let mut synced = None;
        match load_ranges(&self.assets, &doc.skeleton) {
            Ok(r) => doc.ranges = Some(Arc::new(r)),
            Err(e) => {
                tracing::warn!("{e}");
                notes.push(e);
            }
        }
        doc.models = self.models;
        match self.ai {
            Ok(model) => doc.ai = model,
            Err(e) => {
                tracing::warn!("AI model not loaded: {e}");
                notes.push(format!("AI model not loaded: {e}"));
            }
        }
        let mut session = Session::new(doc);
        let mut clashes = crate::register(&mut session);
        for specs in &self.extra {
            clashes.extend(session.register(specs));
        }
        if !clashes.is_empty() {
            let ids: Vec<String> = clashes.iter().map(ToString::to_string).collect();
            return Err(format!("command clash: {}", ids.join("; ")));
        }
        let mut soft = |what: String| -> Result<(), String> {
            if self.strict {
                return Err(what);
            }
            tracing::warn!("{what}");
            notes.push(what);
            Ok(())
        };
        if let Some(lib) = &self.library {
            let from = self.assets.join("library");
            let from = json!({ "from": from.to_string_lossy() });
            if self.install && !lib.join("animations").is_dir() {
                match session.run("library.install", &from) {
                    Ok(v) => installed = Some(v["copied"].as_u64().unwrap_or_default()),
                    Err(e) => soft(format!("couldn't install the built-in library: {e}"))?,
                }
            } else if self.install {
                // built-ins fixed, added or dropped since the library was installed
                match session.run("library.sync", &from) {
                    Ok(v) => synced = crate::builtins::summary(&v),
                    Err(e) => soft(format!("couldn't update the built-in library: {e}"))?,
                }
            }
            if let Err(e) = crate::commands::load_parts(&mut session.document) {
                soft(format!("couldn't read your part sets: {e}"))?;
            }
        }
        Ok(Built {
            session,
            notes,
            installed,
            synced,
        })
    }
}

/// Logging for a front end: `AC_LOG` is a `tracing` filter (`debug`, `ac_core=debug`, ...; default
/// `warn`), written to stderr. Each command run is a `command` span (id, undoable, refused, and its
/// time when it closes). Timings only ever go to the log, never into a result or a file.
pub fn init_logging() {
    use tracing_subscriber::fmt::format::FmtSpan;
    let filter = tracing_subscriber::EnvFilter::try_from_env("AC_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    // a second call (tests, an embedding app) keeps the first subscriber
    if let Err(e) = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_span_events(FmtSpan::CLOSE)
        .with_writer(std::io::stderr)
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .try_init()
    {
        tracing::debug!("logging already set up: {e}");
    }
}

/// The range-of-motion tables (`<assets>/rig/ranges.json`); an error says what the Range panel and
/// `clip.range` then lack, and the rest of the session works without them.
pub fn load_ranges(assets: &Path, sk: &Skeleton) -> Result<ac_core::ranges::Ranges, String> {
    let path = assets.join("rig/ranges.json");
    let text = std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "range-of-motion tables not loaded ({}: {e}); the Range panel is empty",
            path.display()
        )
    })?;
    ac_core::ranges::Ranges::from_json(&text, sk).map_err(|e| {
        format!(
            "range-of-motion tables not loaded ({}: {e}); the Range panel is empty",
            path.display()
        )
    })
}
