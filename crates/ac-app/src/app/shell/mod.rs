//! The app shell around the panels: menus built from the command registry, the keymap, the
//! command palette and its forms, the status bar and messages, the undo history, settings,
//! saved preferences, the performance HUD, the panel layout and asking before quitting.
//!
//! Everything here reaches the document through `Session::run` (via `App::run`); what isn't a
//! command is an app action (`app.*`, [`actions`]), which only changes the window.
//!
//! `app.rs` calls in at a few places: [`Shell::new`] (startup), `begin_frame` (theme, scale, keys),
//! `panels` (the layout), `windows` and `end_frame` (each frame), `close_requested` (eframe's
//! `logic`) and `exit` (`on_exit`).

pub mod actions;
mod form;
mod fuzzy;
mod history;
mod keys;
mod layout;
pub mod menus;
mod palette;
mod perf;
mod remember;
mod scoped;
mod settings;
mod status;
#[cfg(test)]
mod tests;

use std::time::Instant;

use craft_settings::SettingsStore;
use serde_json::json;

use super::App;
use super::prefs::{self, Prefs, Theme};

pub use layout::DockPanel;
pub use status::Level;

/// The shell's state, one field of [`App`].
pub struct Shell {
    pub prefs: SettingsStore<Prefs>,
    pub dock: craft_ui::docking::Workspace<DockPanel>,
    keys: keys::Keys,
    pub palette: Option<palette::PaletteState>,
    pub form: Option<form::Form>,
    pub messages: status::Messages,
    pub perf: perf::Perf,
    /// The windows that are open.
    pub open: Open,
    /// The viewport fills the window (Ctrl+Space): the side panels and the timeline are hidden.
    pub maximised: bool,
    /// The person said Save or Don't save to the quit question: the next close goes through.
    pub quit_ok: bool,
    /// Close the window at the end of this frame (after the quit question).
    pub close_now: bool,
    /// The theme and scale last given to egui, to set them only when they change.
    applied: Option<(Theme, f32)>,
    /// The status line last seen, so that each new one goes into the message history once.
    last_status: String,
    /// Where the viewport was drawn last frame (the performance HUD sits in its corner).
    pub viewport_rect: Option<egui::Rect>,
    /// The timeline's height fitted to its content (measured last frame; 0: not yet).
    pub timeline_natural: f32,
    /// The person is dragging the timeline's top edge (its height becomes their own).
    pub timeline_resizing: bool,
    /// This drag already lit the Dope sheet button (it lights once a drag).
    pub timeline_hinted: bool,
    /// Where the timeline panel was last frame (its top edge is the resize handle).
    pub timeline_rect: Option<egui::Rect>,
    /// The panels are still fitting their content (control-port replies wait for it).
    pub layout_settling: bool,
    /// Bumped by Reset layout: new panel ids, so egui forgets the sizes it kept.
    pub layout_gen: u32,
}

#[derive(Default, Clone, Copy, Debug)]
pub struct Open {
    pub settings: bool,
    pub history: bool,
    pub messages: bool,
    pub about: bool,
}

impl Shell {
    /// After every command is registered: the keymap is built from the registry. `store` and
    /// `notes` come from [`prefs::open`] (opened early: the library folder is a preference).
    pub fn new(
        session: &ac_core::Session,
        store: SettingsStore<Prefs>,
        notes: Vec<String>,
        adapter: Option<&eframe::egui_wgpu::RenderState>,
    ) -> Shell {
        let (keys, key_notes) = keys::Keys::new(
            session,
            store.get().shortcuts.clone(),
            store.get().shortcut_sets.0.clone(),
        );
        let dock = layout::load_dock_workspace(&store.get().layout.dock_json);
        let mut shell = Shell {
            prefs: store,
            dock,
            keys,
            palette: None,
            form: None,
            messages: status::Messages::default(),
            perf: perf::Perf::new(adapter),
            open: Open::default(),
            maximised: false,
            quit_ok: false,
            close_now: false,
            applied: None,
            last_status: String::new(),
            viewport_rect: None,
            layout_gen: 0,
            timeline_natural: 0.0,
            timeline_resizing: false,
            timeline_hinted: false,
            timeline_rect: None,
            layout_settling: false,
        };
        for n in notes.into_iter().chain(key_notes) {
            shell.messages.push(Level::Warning, n);
        }
        if let Some(w) = shell.perf.adapter_warning() {
            shell.messages.push(Level::Warning, w);
        }
        shell
    }
}

impl App {
    /// Put the saved view state back (after `--script`, before `--ui`, which wins).
    pub(super) fn restore_prefs(&mut self) {
        let p = self.shell.prefs.get().clone();
        for (name, value) in [
            ("tool", p.view.tool.as_str()),
            ("tab", p.view.library_tab.as_str()),
            ("source", p.view.library_source.as_str()),
            ("sort", p.view.library_sort.as_str()),
        ] {
            if let Err(e) = self.set_ui(name, value) {
                self.shell.messages.push(
                    Level::Warning,
                    format!("settings: {name} {value:?}: {e}; the default is used"),
                );
            }
        }
        self.show_overlay = p.view.overlay;
        self.show_ghost = p.view.ghost;
        self.timeline.dope = p.view.dope;
        self.restore_view(&p);
        let c = p.view.camera;
        if [c.yaw, c.pitch, c.distance]
            .iter()
            .chain(&c.target)
            .all(|x| x.is_finite())
            && (0.5..=40.0).contains(&c.distance)
        {
            self.camera.yaw = c.yaw;
            self.camera.pitch = c.pitch;
            self.camera.distance = c.distance;
            self.camera.target = ac_render::glam::Vec3::from_array(c.target);
        }
        // the AI sliders: a style this model doesn't know is dropped, and said so
        if self.session.document.ai.is_some() {
            let a = &p.ai;
            let mut params = json!({ "style_force": a.style_force, "ai_force": a.ai_force,
                "trajectory": a.trajectory, "head": a.head });
            if a.style != "any" {
                params["style"] = json!(a.style);
            }
            if let Err(e) = self.session.run("ai.settings", &params) {
                self.shell.messages.push(
                    Level::Warning,
                    format!(
                        "settings: the saved AI fill settings weren't applied ({e}); the defaults are used"
                    ),
                );
            }
        }
    }

    /// Start of a frame: theme and scale, the keys pressed (shortcuts, app actions).
    pub(super) fn begin_frame(&mut self, ctx: &egui::Context) {
        self.shell.perf.frame_start(ctx);
        let g = &self.shell.prefs.get().general;
        // the timeline counts time as Settings > Units says
        self.timeline.time = super::timeline::TimeFormat::from_units(g.units);
        let want = (g.theme, g.ui_scale.clamp(0.5, 3.0));
        if self.shell.applied != Some(want) {
            ctx.set_visuals(super::theme::visuals(want.0));
            ctx.set_zoom_factor(want.1);
            // Ctrl + / - / 0 are the app's (saved) shortcuts, not egui's own
            ctx.options_mut(|o| o.zoom_with_keyboard = false);
            self.shell.applied = Some(want);
        }
        self.dispatch_keys(ctx);
    }

    /// The windows over the panels: palette, forms, settings, history, messages, shortcuts,
    /// performance, toasts.
    pub(super) fn windows(&mut self, ctx: &egui::Context) {
        self.palette_window(ctx);
        self.form_window(ctx);
        self.settings_window(ctx);
        self.history_window(ctx);
        self.messages_window(ctx);
        self.shortcuts_window(ctx);
        self.about_window(ctx);
        self.perf_hud(ctx);
        self.toasts(ctx);
    }

    /// End of a frame: new status lines into the history, preferences saved when changed, the
    /// window closed when the quit question said so.
    pub(super) fn end_frame(&mut self, ctx: &egui::Context) {
        if self.status != self.shell.last_status {
            self.shell.last_status = self.status.clone();
            if !self.status.is_empty() {
                let level = status::level_of(&self.status);
                self.shell.messages.note(level, self.status.clone());
            }
        }
        self.capture_prefs();
        let tick = self.shell.prefs.tick(Instant::now());
        if let craft_settings::SaveStatus::Failed(e) = &tick.status {
            self.shell.messages.push(
                Level::Warning,
                format!("settings: couldn't save ({e}); will try again"),
            );
        }
        if let Some(at) = tick.next_deadline {
            ctx.request_repaint_after(at.saturating_duration_since(Instant::now()));
        }
        if self.shell.close_now {
            self.shell.close_now = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.shell.perf.frame_end();
    }

    /// The window's state as preferences (saved a moment after a change).
    fn capture_prefs(&mut self) {
        let tool = match self.tool {
            super::viewport::Tool::Drag => "drag",
            super::viewport::Tool::Reach => "reach",
            super::viewport::Tool::Move => "move",
            super::viewport::Tool::Rotate => "rotate",
        };
        let tab = match self.lib_tab {
            super::library::Tab::Animations => "animations",
            super::library::Tab::Poses => "poses",
        };
        let c = &self.camera;
        let view = prefs::View {
            tool: tool.into(),
            overlay: self.show_overlay,
            ghost: self.show_ghost,
            dope: self.timeline.dope,
            library_tab: tab.into(),
            library_source: super::library::filter::source_name(self.lib_view.filters.source).into(),
            library_sort: self.lib_view.filters.order.name().into(),
            camera: prefs::CameraPrefs {
                yaw: c.yaw,
                pitch: c.pitch,
                distance: c.distance,
                target: c.target.to_array(),
                ortho: c.ortho,
            },
            perf: self.shell.prefs.get().view.perf,
        };
        let ai = prefs::Ai::from_settings(&self.session.document.ai_settings);
        let (viewport, timeline) = self.remembered_view();
        // a drag in progress settles first
        let busy = self.dragging.is_some() || self.moving.is_some();
        if !busy {
            self.shell.prefs.update(|p| {
                p.view = view;
                p.ai = ai;
                p.viewport = viewport;
                p.timeline = timeline;
            });
        }
    }

    /// Before quitting: what's kept is saved now.
    pub(super) fn exit(&mut self) {
        self.capture_prefs();
        if let Err(e) = self.shell.prefs.flush() {
            eprintln!("settings: couldn't save on exit: {e}");
        }
    }

    /// eframe's `logic`: a close (the window's ×, Alt+F4, File > Exit) with unsaved changes asks
    /// first, with the same Save / Don't save / Cancel question as switching animations.
    pub(super) fn close_requested(&mut self, ctx: &egui::Context) {
        let asked = ctx.input(|i| i.viewport().close_requested());
        // a --screenshot run closes itself when the picture is written
        if asked && self.screenshot.is_none() && !self.shell.quit_ok && self.session.document.unsaved() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.confirm.is_none() {
                self.confirm = Some(super::save::Confirm::Switch(super::save::Switch::Quit));
            }
            ctx.request_repaint();
        }
        craft_quit::on_close_request(ctx);
    }

    /// `--ui` names the shell owns (`None`: not one of them).
    pub(super) fn shell_ui(&mut self, name: &str, value: &str) -> Option<Result<(), String>> {
        let on = || match value {
            "on" | "true" | "1" => Ok(true),
            "off" | "false" | "0" => Ok(false),
            _ => Err(format!("{value:?}: on or off")),
        };
        let r = match (name, value) {
            ("overlay", "perf") | ("perf", "on") => {
                self.shell.prefs.update(|p| p.view.perf = true);
                Ok(())
            }
            ("perf", _) => on().map(|v| {
                self.shell.prefs.update(|p| p.view.perf = v);
            }),
            ("window", w) => self.open_window(w),
            ("palette", q) => {
                self.open_palette(q);
                Ok(())
            }
            ("theme", t) => match t {
                "dark" => Ok(Theme::Dark),
                "light" => Ok(Theme::Light),
                _ => Err("dark or light".to_string()),
            }
            .map(|t| {
                self.shell.prefs.update(|p| p.general.theme = t);
            }),
            ("scale", s) => match s.parse::<f32>() {
                Ok(s) if (0.5..=3.0).contains(&s) => {
                    self.shell.prefs.update(|p| p.general.ui_scale = s);
                    Ok(())
                }
                _ => Err("0.5 to 3".to_string()),
            },
            ("colours", c) => match c {
                "standard" => Ok(prefs::Colours::Standard),
                "safe" => Ok(prefs::Colours::Safe),
                _ => Err("standard or safe".to_string()),
            }
            .map(|c| {
                self.shell.prefs.update(|p| p.general.colours = c);
            }),
            ("units", u) => match u {
                "frames" => Ok(prefs::Units::Frames),
                "seconds" => Ok(prefs::Units::Seconds),
                "timecode" => Ok(prefs::Units::Timecode),
                _ => Err("frames, seconds or timecode".to_string()),
            }
            .map(|u| {
                self.shell.prefs.update(|p| p.general.units = u);
            }),
            ("maximise", _) => on().map(|v| self.shell.maximised = v),
            ("library" | "inspector" | "timeline", _) => on().map(|v| {
                self.shell.prefs.update(|p| match name {
                    "library" => p.layout.show_library = v,
                    "inspector" => p.layout.show_inspector = v,
                    _ => p.layout.show_timeline = v,
                });
            }),
            ("side", s) => match s {
                "left" => Ok(prefs::Side::Left),
                "right" => Ok(prefs::Side::Right),
                _ => Err("left or right".to_string()),
            }
            .map(|s| {
                self.shell.prefs.update(|p| p.layout.library_side = s);
            }),
            _ => return None,
        };
        Some(r)
    }

    /// The names [`App::shell_ui`] takes, for `--ui`'s error message.
    pub(super) const SHELL_UI: &'static str = "perf, window (palette, settings, history, messages, shortcuts, about, form:<command>), palette (a search), theme, scale, colours, units, maximise, library, inspector, timeline, side";

    fn open_window(&mut self, w: &str) -> Result<(), String> {
        match w {
            "palette" => self.open_palette(""),
            "settings" => self.shell.open.settings = true,
            "history" => self.shell.open.history = true,
            "messages" => self.shell.open.messages = true,
            "about" => self.shell.open.about = true,
            "shortcuts" => self.open_shortcuts(),
            w => match w.strip_prefix("form:") {
                Some(id) => match self.session.spec(id) {
                    Some(spec) => self.open_form(spec, &json!({})),
                    None => return Err(format!("no command {id:?}")),
                },
                None => {
                    return Err(
                        "palette, settings, history, messages, shortcuts, about or form:<command>".into(),
                    );
                }
            },
        }
        Ok(())
    }
}
