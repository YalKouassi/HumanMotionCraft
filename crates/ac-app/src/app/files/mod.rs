//! Files around the open animation: autosave and crash recovery, the Export and Export preview
//! dialogs, importing by drag-and-drop or File → Import file, recent animations and reopening the
//! last one, the first-run welcome and tips, the block-out wizard and hot-reloading assets.
//!
//! Everything here acts through commands (`recovery.*`, `clip.export*`, `clip.video`,
//! `clip.sheet`, `library.add_file`, `library.import`, `anim.block`, `assets.reload`); this module
//! only keeps the window's state and draws the dialogs. The window calls four hooks:
//! [`App::files_start`] once, [`App::files_input`] before its shortcuts, [`App::files_windows`]
//! after its panels, and [`App::file_menu`] inside File.

mod autosave;
mod block;
mod drop;
mod export;
mod guide;
mod preview;
mod state;
mod watch;
mod welcome;

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::App;
pub use state::AppState;

/// The window's file state (one field of `App`).
#[derive(Default)]
pub struct Files {
    pub state: AppState,
    pub autosave: autosave::Autosave,
    /// The recovery dialog's items (`recovery.list`), while it is on screen.
    pub recover: Option<Vec<Value>>,
    pub export: Option<export::Export>,
    pub guide: Option<guide::Guide>,
    pub preview: Option<preview::Preview>,
    pub welcome: bool,
    /// The tips tour's page, while it is on screen.
    pub tips: Option<usize>,
    pub block: Option<block::Block>,
    pub watch: watch::Watch,
    /// Started for a script, a screenshot or an agent: no prompts at start-up.
    pub automation: bool,
    /// The open animation's id last frame (to keep the recent list).
    seen_id: Option<String>,
    /// Files are hovering over the window (a drag on its way in).
    pub dropping: bool,
    /// The library's keyboard selection and its small dialogs.
    pub nav: super::library::menu::Nav,
}

/// `explorer /select`, `open -R` or the folder: show a written file to the person.
pub fn reveal(path: &Path) -> Result<(), String> {
    reveal_command(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("couldn't show {}: {e}", path.display()))
}

#[cfg(windows)]
fn reveal_command(path: &Path) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut c = std::process::Command::new("explorer");
    // explorer reads `/select,"<path>"` itself (backslashes), not the usual argument quoting
    c.raw_arg(format!(
        "/select,\"{}\"",
        path.to_string_lossy().replace('/', "\\")
    ));
    c
}

#[cfg(target_os = "macos")]
fn reveal_command(path: &Path) -> std::process::Command {
    let mut c = std::process::Command::new("open");
    c.arg("-R").arg(path);
    c
}

#[cfg(not(any(windows, target_os = "macos")))]
fn reveal_command(path: &Path) -> std::process::Command {
    let mut c = std::process::Command::new("xdg-open");
    c.arg(if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(path)
    });
    c
}

/// Open a web address in the person's browser.
pub fn open_url(url: &str) -> Result<(), String> {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program)
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("couldn't open {url}: {e}"))
}

/// Where a dialog window first appears: centred, near the top of the viewport.
pub fn dialog_pos(ctx: &egui::Context) -> egui::Pos2 {
    let r = ctx.content_rect();
    egui::pos2(r.center().x, r.top() + 90.0)
}

/// The person's documents folder, where exports go until they choose another.
pub fn documents() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let docs = home.join("Documents");
    if docs.is_dir() { docs } else { home }
}

impl App {
    /// After start-up: read the remembered state (the preferences' `files` section; a library's
    /// old `.app-state.json` is moved in once); then offer unsaved work back, reopen the last
    /// animation and welcome a first run. `scripted` (a script, a screenshot, an animation to
    /// open): none of these, the status bar says when unsaved work waits. `agent` (the control
    /// port): only the offer, as an agent drives a fresh window.
    pub(super) fn files_start(&mut self, scripted: bool, agent: bool) {
        self.files.automation = scripted || agent;
        let Some(lib) = self.session.document.library.clone() else {
            return;
        };
        self.files.state = self.shell.prefs.get().files.state_for(&lib);
        if !self.shell.prefs.get().files.knows(&lib) {
            self.migrate_state(&lib);
        }
        self.files.watch.start(&self.assets, &lib);
        self.files.seen_id = self.session.document.clip_id.clone();
        if scripted {
            // a script drives this window: keep the unsaved work, but don't ask now
            if let Some(n) = self.recovery_offered() {
                self.status =
                    format!("unsaved work from last time ({n}) is kept: File > Restore unsaved work…");
            }
            return;
        }
        if let Some(id) = self.files.state.last_open.clone().filter(|_| !agent) {
            let exists = self.library["animations"]
                .as_array()
                .is_some_and(|a| a.iter().any(|it| it["id"].as_str() == Some(id.as_str())));
            if exists {
                if self.run("clip.open", json!({ "id": id })).is_some() {
                    self.status = format!("reopened {id}");
                    self.files.seen_id = Some(id);
                }
            } else {
                self.status = format!("the last animation, {id}, is no longer in the library");
            }
        }
        if self.recovery_offered().is_some() {
            self.open_recovery();
        }
        if !agent && !self.files.state.welcome_seen {
            self.files.welcome = true;
        }
        self.save_state();
    }

    /// A library this window hasn't seen in the preferences: take what an older build kept in
    /// `<library>/.app-state.json` (its recent list, and the person's choices where the
    /// preferences have none yet). The old file is left as it was.
    fn migrate_state(&mut self, lib: &Path) {
        match AppState::load(lib) {
            Ok(Some(old)) => {
                self.files.state.merge_older(old);
                self.shell.messages.push(
                    super::shell::Level::Info,
                    format!(
                        "moved the window's remembered state ({}) into the preferences",
                        AppState::path(lib).display()
                    ),
                );
            }
            Ok(None) => {}
            Err(e) => {
                self.status =
                    format!("the window's old remembered state couldn't be read, starting afresh: {e}");
            }
        }
    }

    /// Keep the remembered state in the preferences (saved with them).
    pub(super) fn save_state(&mut self) {
        let Some(lib) = self.session.document.library.clone() else {
            return;
        };
        let state = self.files.state.clone();
        self.shell.prefs.update(|p| p.files.keep(&lib, &state));
    }

    /// Before the window's shortcuts: this module's keys, drops, autosave and watching.
    pub(super) fn files_input(&mut self, ctx: &egui::Context) {
        // Ctrl+E, Ctrl+Shift+E and Ctrl+I are in the keymap (`clip.export`, `app.export_preview`,
        // `app.import_file`), and so are the library's keys (`app.lib_*`)
        self.take_drops(ctx);
        self.track_recent();
        self.autosave_tick();
        self.watch_tick();
        // the watch and the autosave look at the clock: wake up to do so while idle
        ctx.request_repaint_after(std::time::Duration::from_millis(watch::POLL_MS));
    }

    /// After the panels: the dialogs on screen.
    pub(super) fn files_windows(&mut self, ctx: &egui::Context) {
        self.recovery_dialog(ctx);
        self.export_window(ctx);
        self.preview_window(ctx);
        self.block_window(ctx);
        self.guide_window(ctx);
        self.welcome_dialog(ctx);
        self.tips_window(ctx);
        self.library_dialogs(ctx);
        self.drop_overlay(ctx);
    }

    /// `--ui dialog=<name>`: open one of this module's dialogs (for screenshots and agents).
    pub(super) fn open_dialog(&mut self, name: &str) -> Result<(), String> {
        match name {
            "export" => self.open_export(),
            "preview" => self.open_preview(),
            "block" => self.open_block(),
            "welcome" => self.files.welcome = true,
            "tips" => self.files.tips = Some(0),
            "recover" => self.open_recovery(),
            "none" => {
                self.files.export = None;
                self.files.preview = None;
                self.files.block = None;
                self.files.guide = None;
                self.files.welcome = false;
                self.files.tips = None;
                self.files.recover = None;
            }
            _ => return Err("export, preview, block, welcome, tips, recover or none".into()),
        }
        Ok(())
    }

    /// The window changed the library itself (a save, an import): the watch takes it as seen.
    pub(super) fn library_synced(&mut self) {
        self.files.watch.synced();
    }

    /// A new library id is open: remember it.
    fn track_recent(&mut self) {
        let id = self.session.document.clip_id.clone();
        if id == self.files.seen_id {
            return;
        }
        self.files.seen_id = id.clone();
        if let Some(id) = id {
            self.files.state.opened(&id);
            self.save_state();
        }
    }
}
