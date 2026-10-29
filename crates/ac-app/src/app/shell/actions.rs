//! App actions: what the keymap, menus and palette can do that only changes the window (tools,
//! panels, playback, frame stepping, opening windows). They sit beside the registry's commands in
//! one table, with ids `app.<name>`, and reach the document only through commands (`frame.set`
//! for stepping). [`App::invoke`] runs either kind the way a click would: commands with required
//! inputs open their form, and a few commands go through the app's own flow (`clip.save` saves a
//! built-in as your own copy, `clip.new` and `clip.open` ask about unsaved changes first).
//!
//! **Adding an action:** a row in [`ACTIONS`] and an arm in [`App::act`]; the keymap, menus,
//! palette and Help > Keyboard shortcuts pick it up.

use craft_keymap::CommandRef;
use craft_menu::ItemRole;
use serde_json::{Value, json};

use super::super::App;
use super::super::prefs::{self, Colours, Side, Theme};
use super::super::save::Switch;
use super::super::viewport::Tool;

/// How an action shows in a menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Show {
    Plain,
    /// On or off.
    Check,
    /// One of a group (the tools).
    Radio,
}

pub struct Action {
    pub id: &'static str,
    pub label: &'static str,
    /// The top-level menu it goes in.
    pub menu: &'static str,
    /// Default shortcuts (craft-keymap text; `Cmd` is Ctrl off macOS).
    pub keys: &'static [&'static str],
    pub show: Show,
    /// Runs again while its key is held.
    pub repeat: bool,
    pub role: Option<ItemRole>,
    pub doc: &'static str,
    /// A submenu of `menu` it goes in (`None`: the menu itself).
    pub sub: Option<&'static str>,
}

const fn a(
    id: &'static str,
    label: &'static str,
    menu: &'static str,
    keys: &'static [&'static str],
) -> Action {
    Action {
        id,
        label,
        menu,
        keys,
        show: Show::Plain,
        repeat: false,
        role: None,
        doc: "",
        sub: None,
    }
}

impl Action {
    const fn check(mut self) -> Action {
        self.show = Show::Check;
        self
    }
    const fn radio(mut self) -> Action {
        self.show = Show::Radio;
        self
    }
    const fn repeat(mut self) -> Action {
        self.repeat = true;
        self
    }
    const fn role(mut self, role: ItemRole) -> Action {
        self.role = Some(role);
        self
    }
    const fn doc(mut self, doc: &'static str) -> Action {
        self.doc = doc;
        self
    }
    const fn sub(mut self, sub: &'static str) -> Action {
        self.sub = Some(sub);
        self
    }
}

/// Every app action, in menu order. Separators between groups are the menus' business
/// (`menus.rs`).
pub static ACTIONS: &[Action] = &[
    a("app.settings", "Settings…", "File", &["Cmd+,"])
        .role(ItemRole::Settings)
        .doc("default fps, autosave, UI scale, theme, units, library folder"),
    a("app.import_file", "Import file…", "File", &["Cmd+I"])
        .doc("animations and poses (.clip.json, .pose.json); or drop files on the window"),
    a("app.export_preview", "Export preview…", "File", &["Cmd+Shift+E"])
        .doc("an MP4 or GIF of the animation, or a contact sheet"),
    a("app.block_out", "Block out from poses…", "File", &[])
        .doc("pick library poses, place them on frames and fill between (one undo)"),
    a("app.recover", "Restore unsaved work…", "File", &[]).doc("work kept by autosave"),
    a("app.welcome", "Welcome…", "Help", &[]),
    a("app.quit", "Quit", "File", &["Cmd+Q"])
        .role(ItemRole::Quit)
        .doc("asks first when there are unsaved changes"),
    a("app.palette", "Command palette…", "Edit", &["Cmd+Shift+P"]).doc("search every command and run it"),
    a("app.history", "Undo history", "Edit", &[])
        .check()
        .doc("every undo step; click one to go back or forward to it"),
    a("app.play", "Play / pause", "Key", &["Space"]),
    a("app.frame_prev", "Previous frame", "Key", &["Left"]).repeat(),
    a("app.frame_next", "Next frame", "Key", &["Right"]).repeat(),
    a("app.key_prev", "Previous key", "Key", &["Up"]).repeat(),
    a("app.key_next", "Next key", "Key", &["Down"]).repeat(),
    a("app.frame_start", "First frame", "Key", &["Home"]),
    a("app.frame_end", "Last frame", "Key", &["End"]),
    a("app.delete_keys", "Delete selected keys", "Key", &["Delete"])
        .doc("the keys selected on the timeline, as one undo step"),
    // the timeline's keys (`scoped.rs` says when each context is on)
    a(
        "app.copy_keys",
        "Copy keys",
        "Key",
        &["Timeline selection: Cmd+C"],
    )
    .sub("Timeline")
    .doc("the selected keys; over the timeline Ctrl+C copies keys, elsewhere the pose"),
    a(
        "app.paste_keys",
        "Paste keys",
        "Key",
        &["Timeline clipboard: Cmd+V"],
    )
    .sub("Timeline")
    .doc("at the playhead, as one undo step"),
    a(
        "app.paste_keys_mirrored",
        "Paste keys mirrored",
        "Key",
        &["Timeline clipboard: Cmd+Shift+V"],
    )
    .sub("Timeline"),
    a(
        "app.select_next_key",
        "Select next key",
        "Key",
        &["Timeline focus: Right", "Timeline focus: Tab"],
    )
    .sub("Timeline")
    .repeat(),
    a(
        "app.select_prev_key",
        "Select previous key",
        "Key",
        &["Timeline focus: Left", "Timeline focus: Shift+Tab"],
    )
    .sub("Timeline")
    .repeat(),
    a(
        "app.extend_next_key",
        "Add the next key",
        "Key",
        &["Timeline focus: Shift+Right"],
    )
    .sub("Timeline")
    .repeat(),
    a(
        "app.extend_prev_key",
        "Add the previous key",
        "Key",
        &["Timeline focus: Shift+Left"],
    )
    .sub("Timeline")
    .repeat(),
    a(
        "app.select_first_key",
        "Select first key",
        "Key",
        &["Timeline focus: Home"],
    )
    .sub("Timeline"),
    a(
        "app.select_last_key",
        "Select last key",
        "Key",
        &["Timeline focus: End"],
    )
    .sub("Timeline"),
    a(
        "app.extend_first_key",
        "Add the first key",
        "Key",
        &["Timeline focus: Shift+Home"],
    )
    .sub("Timeline"),
    a(
        "app.extend_last_key",
        "Add the last key",
        "Key",
        &["Timeline focus: Shift+End"],
    )
    .sub("Timeline"),
    a(
        "app.clear_keys",
        "Clear key selection",
        "Key",
        &["Timeline focus: Escape"],
    )
    .sub("Timeline"),
    a(
        "app.range_in",
        "Play range in at the playhead",
        "Key",
        &["Timeline: I"],
    )
    .sub("Timeline"),
    a(
        "app.range_out",
        "Play range out at the playhead",
        "Key",
        &["Timeline: O"],
    )
    .sub("Timeline"),
    // the library's keys, while it has them (it was clicked last)
    a("app.lib_prev", "Select the item above", "File", &["Library: Up"])
        .sub("Library")
        .repeat(),
    a(
        "app.lib_next",
        "Select the item below",
        "File",
        &["Library: Down"],
    )
    .sub("Library")
    .repeat(),
    a(
        "app.lib_open",
        "Open or apply the selected item",
        "File",
        &["Library: Enter"],
    )
    .sub("Library"),
    a(
        "app.lib_rename",
        "Rename the selected item…",
        "File",
        &["Library: F2"],
    )
    .sub("Library"),
    a(
        "app.lib_delete",
        "Delete the selected item…",
        "File",
        &["Library: Delete"],
    )
    .sub("Library")
    .doc("asks first; built-ins stay"),
    a("app.tool_drag", "Drag tool", "View", &["Q"])
        .radio()
        .doc("drag a joint to pose it"),
    a("app.tool_move", "Move tool", "View", &["W"])
        .radio()
        .doc("slide or turn the body on this frame and its key; Alt: the whole animation"),
    a("app.tool_rotate", "Rotate tool", "View", &["E"])
        .radio()
        .doc("turn the selected joint with the rings or the trackball"),
    a("app.tool_reach", "IK tool", "View", &["R"])
        .radio()
        .doc("full-body reach: pins and limits hold"),
    a("app.view_front", "Front", "View", &["Viewport: 1"]).sub("Camera"),
    a("app.view_back", "Back", "View", &["Viewport: Cmd+1"]).sub("Camera"),
    a("app.view_right", "Right side", "View", &["Viewport: 3"]).sub("Camera"),
    a("app.view_left", "Left side", "View", &["Viewport: Cmd+3"]).sub("Camera"),
    a("app.view_top", "Top", "View", &["Viewport: 7"]).sub("Camera"),
    a("app.view_bottom", "Bottom", "View", &["Viewport: Cmd+7"]).sub("Camera"),
    a(
        "app.view_three_quarter",
        "Three-quarter",
        "View",
        &["Viewport: 0"],
    )
    .sub("Camera"),
    a("app.frame_selection", "Frame selection", "View", &["Viewport: F"])
        .sub("Camera")
        .doc("fill the view with the selected joints, or the body"),
    a("app.ortho", "Orthographic", "View", &["Viewport: 5"])
        .sub("Camera")
        .check(),
    a("app.follow", "Follow the body", "View", &[])
        .sub("Camera")
        .check(),
    a("app.overlay", "Physics overlay", "View", &[]).check(),
    a("app.ghost", "Ghosts", "View", &[])
        .check()
        .doc("the keyed pose behind the physics pass, the AI preview"),
    a("app.dope", "Dope sheet", "View", &[]).check(),
    a("app.library", "Library panel", "View", &[]).check(),
    a("app.inspector", "Inspector panel", "View", &[]).check(),
    a("app.timeline", "Timeline panel", "View", &[]).check(),
    a("app.swap_sides", "Library on the right", "View", &[]).check(),
    a("app.maximise", "Maximise viewport", "View", &["Cmd+Space"]).check(),
    a("app.reset_layout", "Reset layout", "View", &[]),
    a("app.zoom_in", "Zoom in", "View", &["Cmd+Plus", "Cmd+="])
        .repeat()
        .doc("UI scale"),
    a("app.zoom_out", "Zoom out", "View", &["Cmd+-"])
        .repeat()
        .doc("UI scale"),
    a("app.zoom_reset", "Actual size", "View", &["Cmd+0"]).doc("UI scale 100%"),
    a("app.light", "Light theme", "View", &[]).check(),
    a("app.colours", "Colour-safe overlays", "View", &[])
        .check()
        .doc("support area, ghosts and preview in colours anyone tells apart"),
    a("app.perf", "Performance", "View", &[])
        .check()
        .doc("fps, frame time, the last command's time, physics rebuilds"),
    a("app.messages", "Messages", "View", &[]).check(),
    a("app.shortcuts", "Keyboard shortcuts…", "Help", &["F1"]).doc("every shortcut; record your own"),
    a("app.clear_thumbs", "Clear thumbnail cache", "Help", &[]).doc("library thumbnails are drawn again"),
    a("app.about", "About", "Help", &[]).role(ItemRole::About),
];

pub fn action(id: &str) -> Option<&'static Action> {
    ACTIONS.iter().find(|a| a.id == id)
}

/// Whether a command id names something [`App::invoke`] can run: a registered command or an
/// app action.
#[cfg(test)]
pub fn runnable(session: &ac_core::Session, id: &str) -> bool {
    action(id).is_some() || session.spec(id).is_some()
}

/// A command's result as the status line reads it: undo and redo by the step's name, a note as
/// it is, a short answer after the command's title.
pub fn result_text(id: &str, label: &str, result: &Value) -> String {
    if matches!(id, "edit.undo" | "edit.redo") {
        let done = if id == "edit.undo" { "Undid" } else { "Redid" };
        return match (result["changed"].as_bool(), result["label"].as_str()) {
            (Some(true), Some(step)) => format!("{done}: {step}"),
            _ => result["note"].as_str().unwrap_or("nothing changed").to_string(),
        };
    }
    let text = match result {
        Value::Null => String::new(),
        Value::Object(m) if m.is_empty() => String::new(),
        Value::Object(m) if m.len() == 1 && m.contains_key("note") => {
            return format!("{label}: {}", result["note"].as_str().unwrap_or_default());
        }
        v => v.to_string(),
    };
    if text.is_empty() {
        return format!("{label}: done");
    }
    let short: String = text.chars().take(240).collect();
    let more = if short.chars().count() < text.chars().count() {
        " …"
    } else {
        ""
    };
    format!("{label}: {short}{more}")
}

/// Commands after which the library list is read again (what it lists may have changed).
fn touches_library(id: &str) -> bool {
    let (area, verb) = id.split_once('.').unwrap_or((id, ""));
    match area {
        "library" => true,
        "clip" => !matches!(
            verb,
            "loop" | "move" | "curves" | "contacts" | "sample" | "export" | "write"
        ),
        "pose" | "parts" => matches!(verb, "save" | "delete" | "rename" | "group" | "apply"),
        _ => false,
    }
}

/// Commands the window runs itself (gesture brackets): never offered in menus or the palette.
pub fn internal(id: &str) -> bool {
    matches!(id, "edit.begin" | "edit.end")
}

impl App {
    /// Run a keymap, menu or palette choice the way a click would.
    pub(in crate::app) fn invoke(&mut self, cmd: &CommandRef, ctx: &egui::Context) {
        if action(&cmd.id).is_some() {
            self.act(&cmd.id, ctx);
        } else {
            self.invoke_command(&cmd.id, &cmd.params);
        }
    }

    /// A registered command, from a menu, key or the palette.
    pub(in crate::app) fn invoke_command(&mut self, id: &str, params: &Value) {
        let given = params.as_object().filter(|m| !m.is_empty());
        // the app's own flow for these (without parameters: as the buttons do)
        match (id, given) {
            ("clip.save", None) => {
                self.save();
                return;
            }
            ("clip.new", None) => {
                self.switch(Switch::New(self.new_animation_params()));
                return;
            }
            ("clip.open", Some(p)) if p.len() == 1 => {
                if let Some(to) = p.get("id").and_then(Value::as_str) {
                    self.switch(Switch::Open(to.to_string()));
                    return;
                }
            }
            // Export… (Ctrl+E): the dialog (profile, several animations, the folder)
            ("clip.export", None) => {
                self.open_export();
                return;
            }
            ("keys.delete", None) => {
                self.delete_keys();
                return;
            }
            // Shift+A: the status line says which way the span went
            ("ai.toggle", None) => {
                self.ai_toggle();
                return;
            }
            _ => {}
        }
        let Some(spec) = self.session.spec(id) else {
            let e = ac_core::CoreError::UnknownCommand {
                command: id.to_string(),
                suggestions: ac_core::suggest::closest(id, self.session.commands().iter().map(|s| s.id)),
            };
            self.refused(id, &e);
            return;
        };
        let missing = spec
            .inputs
            .iter()
            .any(|p| p.required && given.is_none_or(|g| g.get(p.name).is_none_or(Value::is_null)));
        if missing {
            self.open_form(spec, params);
            return;
        }
        if let Some(result) = self.run_from_ui(id, params) {
            // the answer goes in the status line (and so the message history)
            self.status = result_text(id, spec.label, &result);
        }
    }

    /// Run a command for the UI: a refusal is shown (`App::run`), and the library list is read
    /// again after commands that may change it.
    pub(in crate::app) fn run_from_ui(&mut self, id: &str, params: &Value) -> Option<Value> {
        let result = self.run(id, params.clone())?;
        if touches_library(id) {
            self.refresh_library();
        }
        Some(result)
    }

    /// `clip.new`'s parameters from the UI: a fresh name and the default fps from Settings.
    pub(in crate::app) fn new_animation_params(&self) -> Value {
        let fps = self.shell.prefs.get().general.default_fps;
        let fps = if fps.is_finite() {
            fps.clamp(1.0, 240.0)
        } else {
            30.0
        };
        json!({ "name": "Untitled", "fps": fps })
    }

    /// Whether an action shows as on (check and radio items).
    pub(in crate::app) fn action_on(&self, id: &str) -> Option<bool> {
        let p = self.shell.prefs.get();
        Some(match id {
            "app.history" => self.shell.open.history,
            "app.tool_drag" => self.tool == Tool::Drag,
            "app.tool_reach" => self.tool == Tool::Reach,
            "app.tool_move" => self.tool == Tool::Move,
            "app.tool_rotate" => self.tool == Tool::Rotate,
            "app.overlay" => self.show_overlay,
            "app.ghost" => self.show_ghost,
            "app.dope" => self.timeline.dope,
            "app.library" => p.layout.show_library,
            "app.inspector" => p.layout.show_inspector,
            "app.timeline" => p.layout.show_timeline,
            "app.swap_sides" => p.layout.library_side == Side::Right,
            "app.maximise" => self.shell.maximised,
            "app.light" => p.general.theme == Theme::Light,
            "app.colours" => p.general.colours == Colours::Safe,
            "app.perf" => p.view.perf,
            "app.messages" => self.shell.open.messages,
            "app.ortho" => self.camera.ortho,
            "app.follow" => self.view.follow,
            _ => return None,
        })
    }

    /// Run an app action.
    pub(in crate::app) fn act(&mut self, id: &str, ctx: &egui::Context) {
        match id {
            "app.settings" => self.shell.open.settings = true,
            "app.quit" => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            "app.palette" => self.open_palette(""),
            "app.history" => self.shell.open.history = !self.shell.open.history,
            "app.play" => self.toggle_play(),
            "app.frame_prev" => self.step_frame(-1.0),
            "app.frame_next" => self.step_frame(1.0),
            "app.key_prev" => self.step_key(false),
            "app.key_next" => self.step_key(true),
            "app.frame_start" => self.go_frame(0.0),
            "app.frame_end" => {
                let doc = &self.session.document;
                // the length counts frames: the last one is a frame before it
                let end = (doc.clip.length_with(&doc.links) - 1).max(0) as f64;
                self.go_frame(end);
            }
            "app.delete_keys" => self.delete_keys(),
            "app.tool_drag" => self.set_tool(Tool::Drag),
            "app.tool_reach" => self.set_tool(Tool::Reach),
            "app.tool_move" => self.set_tool(Tool::Move),
            "app.tool_rotate" => self.set_tool(Tool::Rotate),
            "app.import_file" => self.import_file_dialog(),
            "app.export_preview" => self.open_preview(),
            "app.block_out" => self.open_block(),
            "app.recover" => self.open_recovery(),
            "app.welcome" => self.files.welcome = true,
            "app.overlay" => self.show_overlay = !self.show_overlay,
            "app.ghost" => self.show_ghost = !self.show_ghost,
            "app.dope" => self.timeline.dope = !self.timeline.dope,
            "app.library" => {
                self.shell
                    .prefs
                    .update(|p| p.layout.show_library = !p.layout.show_library);
            }
            "app.inspector" => {
                self.shell
                    .prefs
                    .update(|p| p.layout.show_inspector = !p.layout.show_inspector);
            }
            "app.timeline" => {
                self.shell
                    .prefs
                    .update(|p| p.layout.show_timeline = !p.layout.show_timeline);
            }
            "app.swap_sides" => {
                self.shell.prefs.update(|p| {
                    p.layout.library_side = match p.layout.library_side {
                        Side::Left => Side::Right,
                        Side::Right => Side::Left,
                    }
                });
            }
            "app.maximise" => self.shell.maximised = !self.shell.maximised,
            "app.reset_layout" => self.reset_layout(ctx),
            "app.zoom_in" | "app.zoom_out" => {
                let up = id == "app.zoom_in";
                self.shell
                    .prefs
                    .update(|p| p.general.ui_scale = prefs::step_scale(p.general.ui_scale, up));
                self.status = format!("UI scale {:.0}%", self.shell.prefs.get().general.ui_scale * 100.0);
            }
            "app.zoom_reset" => {
                self.shell.prefs.update(|p| p.general.ui_scale = 1.0);
                self.status = "UI scale 100%".into();
            }
            "app.light" => {
                self.shell.prefs.update(|p| {
                    p.general.theme = match p.general.theme {
                        Theme::Dark => Theme::Light,
                        Theme::Light => Theme::Dark,
                    }
                });
            }
            "app.colours" => {
                self.shell.prefs.update(|p| {
                    p.general.colours = match p.general.colours {
                        Colours::Standard => Colours::Safe,
                        Colours::Safe => Colours::Standard,
                    }
                });
            }
            "app.perf" => {
                self.shell.prefs.update(|p| p.view.perf = !p.view.perf);
            }
            "app.messages" => self.shell.open.messages = !self.shell.open.messages,
            "app.shortcuts" => self.open_shortcuts(),
            "app.clear_thumbs" => self.clear_thumbnails(),
            "app.about" => self.shell.open.about = true,
            other => {
                if !self.act_scoped(other, ctx) {
                    self.shell
                        .messages
                        .push(super::Level::Error, format!("no app action {other:?} (a bug)"));
                }
            }
        }
    }

    /// Go to a frame (stops playback); a refusal shows like any other.
    fn go_frame(&mut self, frame: f64) {
        self.playing = false;
        self.run("frame.set", json!({ "frame": frame.max(0.0) }));
    }

    /// One frame back or on, from the whole frame shown.
    fn step_frame(&mut self, by: f64) {
        let f = self.session.document.frame.round() + by;
        if f < 0.0 {
            self.status = "already at frame 0".into();
            return;
        }
        self.go_frame(f);
    }

    /// The previous or next key's frame.
    fn step_key(&mut self, forward: bool) {
        let doc = &self.session.document;
        let now = doc.frame.round() as i64;
        let frames = doc.clip.keys().iter().map(|k| k.frame);
        let to = if forward {
            frames.filter(|f| *f > now).min()
        } else {
            frames.filter(|f| *f < now).max()
        };
        match to {
            Some(f) => self.go_frame(f as f64),
            None => {
                self.status = format!("no key {} frame {now}", if forward { "after" } else { "before" });
            }
        }
    }

    /// Delete the library's thumbnail cache (`<library>/.thumbs`); thumbnails are drawn again.
    fn clear_thumbnails(&mut self) {
        let Some(lib) = self.session.document.library.clone() else {
            self.status = "no library folder: there is no thumbnail cache".into();
            return;
        };
        let dir = lib.join(".thumbs");
        let result = if dir.is_dir() {
            std::fs::remove_dir_all(&dir)
        } else {
            Ok(())
        };
        match result {
            Ok(()) => {
                self.thumbs.cache.clear();
                self.status = format!("cleared the thumbnail cache ({})", dir.display());
            }
            Err(e) => {
                self.shell.messages.push(
                    super::Level::Error,
                    format!("Clear thumbnail cache: couldn't delete {}: {e}", dir.display()),
                );
            }
        }
    }
}
