//! The window: menu bar, library (left), inspector (right), timeline (bottom), 3D viewport.

mod agent;
mod character;
mod community;
mod curves;
mod files;
mod gizmo;
mod input;
mod inspector;
mod library;
mod onion;
mod panels;
mod picker;
pub mod prefs;
pub mod record;
mod save;
mod shell;
mod theme;
mod timeline;
mod trail;
mod ui_click;
mod ui_ids;
mod view_camera;
mod view_state;
mod view_toolbar;
mod viewport;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use ac_core::{Session, Skeleton};
use ac_render::glam::Vec3;
use ac_render::{Camera, Mesh, Renderer};
use serde_json::{Value, json};

use crate::Args;

/// Screenshot state: 0 waiting, 1 requested, 2 written, 3 failed.
type Shot = Arc<AtomicU8>;

pub struct App {
    session: Session,
    renderer: Renderer,
    render_state: eframe::egui_wgpu::RenderState,
    texture: Option<egui::TextureId>,
    /// Skin joint i is rig bone `skin_to_bone[i]`.
    skin_to_bone: Vec<usize>,
    camera: Camera,
    library: Value,
    selected: Option<usize>,
    dragging: Option<(usize, f32)>,
    moving: Option<viewport::Moving>,
    playing: bool,
    status: String,
    screenshot: Option<(PathBuf, Shot)>,
    /// `--record`, on its way (`app/record.rs`).
    record: Option<record::Recording>,
    /// `--size`, in pixels.
    window_size: Option<(u32, u32)>,
    size_tries: u32,
    frames_drawn: u32,
    assets: PathBuf,
    /// Requests from the control port (`--control`).
    control: Option<std::sync::mpsc::Receiver<crate::control::Request>>,
    /// What happens in the window, for the control port's `events` and `subscribe`.
    events: Option<crate::control::Events>,
    watch: agent::Watch,
    /// A headless renderer for pictures an agent asks for (made on first use).
    offscreen: Option<ac_render::offscreen::Offscreen>,
    /// Window screenshots asked for and not yet taken (the window keeps drawing until they are).
    shots_pending: Arc<std::sync::atomic::AtomicUsize>,
    /// What dragging a joint does.
    tool: viewport::Tool,
    show_overlay: bool,
    show_ghost: bool,
    /// The AI preview's root path, for the preview it was worked out for.
    preview_path: Option<(usize, Vec<Vec3>)>,
    timeline: timeline::Timeline,
    lib_tab: library::Tab,
    /// The library's Community tab.
    community: community::State,
    lib_filter: String,
    thumbs: library::Thumbs,
    /// The library panel's filters, folds and preview.
    lib_view: library::View,
    /// An AI setting changed while a preview is live: preview again.
    ai_dirty: bool,
    ai_previewed: Option<std::time::Instant>,
    /// Pointer and key events from the control port, played one a frame.
    injected: input::Injected,
    /// Where things are on screen (window points), for the control port's `layout`.
    layout: Value,
    /// A question on screen (save before switching or quitting, delete).
    confirm: Option<save::Confirm>,
    /// The viewport's viewing aids, picker and gizmo state.
    view: view_state::ViewAids,
    /// Files: autosave and recovery, export, import, recent, welcome, block-out, hot reload.
    files: files::Files,
    /// Menus, keymap, palette, status bar, history, settings, preferences, layout (`app/shell`).
    shell: shell::Shell,
    /// The inspector panels' own state (joint, layers, physics, scene boxes, AI worker).
    panels: panels::Panels,
    /// The person's own character beside the mannequin (`character.rs`).
    character: character::CharacterState,
    /// Last frame's widgets with their stable ids (`ui_ids.rs`), for the control port.
    widgets: Vec<ui_ids::Widget>,
    /// A `click` waiting for a popup to open (`ui_click.rs`).
    pending_click: Option<ui_click::Pending>,
}

/// The first `assets` folder at or above the working directory or the executable.
fn find_assets() -> Option<PathBuf> {
    let mut starts = vec![std::env::current_dir().ok()?];
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        starts.push(dir);
    }
    for start in starts {
        for dir in start.ancestors() {
            let a = dir.join("assets");
            if a.join("rig/default_rig.json").is_file() {
                return Some(a);
            }
        }
    }
    None
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, args: Args) -> Result<App, String> {
        let assets = args
            .assets
            .clone()
            .or_else(find_assets)
            .ok_or("no assets folder found (pass --assets <dir>)")?;
        let rig = std::fs::read_to_string(assets.join("rig/default_rig.json"))
            .map_err(|e| format!("can't read the rig: {e}"))?;
        let skeleton = Skeleton::from_json(&rig).map_err(|e| e.to_string())?;
        let glb = std::fs::read(assets.join("rig/mannequin.glb"))
            .map_err(|e| format!("can't read the mannequin: {e}"))?;
        let mesh = Mesh::from_glb(&glb)?;
        let skin_to_bone = mesh
            .joints
            .iter()
            .map(|n| {
                skeleton
                    .names
                    .iter()
                    .position(|b| b == n)
                    .ok_or(format!("the mannequin's joint {n:?} is not in the rig"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let rs = cc
            .wgpu_render_state
            .as_ref()
            .ok_or("the wgpu renderer is not available")?
            .clone();
        let renderer = Renderer::new(&rs.device, rs.target_format, &mesh);

        // started for a script or a screenshot, or for an agent: fewer prompts (files/mod.rs)
        let grabbing = args.screenshot.is_some() || args.record.is_some();
        let scripted = args.open.is_some() || args.script.is_some() || grabbing;
        let agent = args.control.is_some();
        // preferences first: the library folder is one
        let automated = grabbing || args.control.is_some();
        let (prefs, prefs_notes) = prefs::open(prefs::location(args.prefs.as_deref(), automated));
        // the app's own library, left under a former name's folder, moves to this name's once
        let default_library = args.library.is_none() && prefs.get().general.library_dir.is_none();
        let moved = if default_library && !automated {
            ac_io::data_folder::move_former_data()
        } else {
            None
        };
        let library = args
            .library
            .clone()
            .or_else(|| prefs.get().general.library_dir.clone())
            .unwrap_or_else(ac_io::library::app_library_dir);
        // your own model (the app's models folder: one trained on data that isn't shipped) wins
        // over the bundled one, and says so; automated runs keep the bundled one (reproducible)
        let own = ac_io::library::app_models_dir();
        let own_model = !automated && ac_ai::has_model(&own);
        let model = if own_model {
            ac_ai::shared_in(&own)
        } else {
            ac_ai::shared(&assets)
        };
        // the same setup as the CLI and MCP; what failed to load is said in the status line
        let built = ac_io::setup::SessionBuilder::new(&assets)
            .library(&library)
            .ai(model)
            .commands(ac_media::SPECS)
            .commands(ac_textmotion::SPECS)
            .commands(ac_posefill::SPECS)
            .commands(ac_posefill::live_commands::SPECS)
            .commands(ac_community::SPECS)
            .build()?;
        let mut session = built.session;
        // the community: a screenshot run gets the fixtures' fake server (the same picture every
        // time); otherwise the live one, with this computer's account beside the app's library
        if grabbing {
            ac_community::fake::use_fake(&assets.join("rig").join("default_rig.json"));
        } else if let Some(dir) = ac_io::library::app_library_dir().parent() {
            ac_community::http::connect(dir.to_path_buf());
        }
        let mut status = match (built.installed, built.synced) {
            (Some(n), _) => format!("installed the built-in library ({n} items)"),
            (None, Some(s)) => s,
            (None, None) => String::new(),
        };
        if !built.notes.is_empty() {
            status = built.notes.join("; ");
        }
        if let Some(m) = moved {
            let note = m.unwrap_or_else(|e| e);
            status = if status.is_empty() {
                note
            } else {
                format!("{note}; {status}")
            };
        }
        if own_model {
            let note = format!("AI fill uses your own model from {}", own.display());
            status = if status.is_empty() {
                note
            } else {
                format!("{status}; {note}")
            };
        }
        if let Some(id) = &args.open
            && let Err(e) = session.run("clip.open", &json!({ "id": id }))
        {
            status = format!("couldn't open {id}: {e}");
        }
        let mut fills: Vec<(String, Value)> = Vec::new();
        if let Some(path) = &args.script {
            let steps = std::fs::read_to_string(path)
                .map_err(|e| e.to_string())
                .and_then(|t| serde_json::from_str::<Value>(&t).map_err(|e| e.to_string()))
                .map_err(|e| format!("--script {}: {e}", path.display()))?;
            let steps = steps
                .get("script")
                .unwrap_or(&steps)
                .as_array()
                .cloned()
                .unwrap_or_default();
            for step in steps {
                let id = step["command"].as_str().unwrap_or_default();
                match session.run(id, &step.get("params").cloned().unwrap_or(json!({}))) {
                    // a fill's diagnostics mark the timeline once the window exists
                    Ok(v) if id.starts_with("ai.") => fills.push((id.to_string(), v)),
                    Ok(_) => {}
                    Err(e) => {
                        status = format!("script: {id}: {e}");
                        break;
                    }
                }
            }
        }
        // a recording of the wrong animation is no recording
        if args.record.is_some() && (status.starts_with("couldn't open") || status.starts_with("script:")) {
            return Err(status);
        }
        if let Some(f) = args.frame {
            let _ = session.run("frame.set", &json!({ "frame": f }));
        }
        let (control, events) = match args.control {
            Some(port) => match crate::control::listen(port, cc.egui_ctx.clone()) {
                Ok((rx, events)) => {
                    status = format!("control port {port} open");
                    (Some(rx), Some(events))
                }
                Err(e) => {
                    status = e;
                    (None, None)
                }
            },
            None => (None, None),
        };
        let shell = shell::Shell::new(&session, prefs, prefs_notes, cc.wgpu_render_state.as_ref());
        let mut app = App {
            session,
            renderer,
            render_state: rs,
            texture: None,
            skin_to_bone,
            camera: Camera::default(),
            library: Value::Null,
            selected: None,
            dragging: None,
            moving: None,
            playing: false,
            status,
            screenshot: args.screenshot.map(|p| (p, Arc::new(AtomicU8::new(0)))),
            record: args.record.map(record::Recording::new),
            window_size: args.size,
            size_tries: 0,
            frames_drawn: 0,
            assets,
            control,
            events,
            watch: agent::Watch::default(),
            offscreen: None,
            shots_pending: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            tool: viewport::Tool::Drag,
            show_overlay: true,
            show_ghost: true,
            preview_path: None,
            timeline: timeline::Timeline::default(),
            lib_tab: library::Tab::Animations,
            community: community::State::default(),
            lib_filter: String::new(),
            thumbs: library::Thumbs::default(),
            lib_view: library::View::default(),
            ai_dirty: false,
            ai_previewed: None,
            injected: input::Injected::default(),
            layout: json!({}),
            confirm: None,
            view: view_state::ViewAids::default(),
            files: files::Files::default(),
            shell,
            panels: panels::Panels::default(),
            character: character::CharacterState::default(),
            widgets: Vec::new(),
            pending_click: None,
        };
        app.refresh_library();
        app.restore_prefs();
        app.files_start(scripted, agent);
        for (id, v) in &fills {
            app.fill_marks(id, v);
        }
        for (k, v) in &args.ui {
            app.set_ui(k, v).map_err(|e| format!("--ui {k}={v}: {e}"))?;
        }
        Ok(app)
    }

    /// Set the window's own state (not the document's): see `--ui` in main.rs.
    fn set_ui(&mut self, name: &str, value: &str) -> Result<(), String> {
        if let Some(r) = self.shell_ui(name, value) {
            return r;
        }
        let on = || match value {
            "on" | "true" | "1" => Ok(true),
            "off" | "false" | "0" => Ok(false),
            _ => Err(format!("{value:?}: on or off")),
        };
        match name {
            "tab" => {
                self.lib_tab = match value {
                    "animations" => library::Tab::Animations,
                    // the built-ins were a tab of their own: now the animations, built-ins only
                    "builtin" => {
                        self.lib_view.filters.source = ac_io::find::Source::Builtin;
                        library::Tab::Animations
                    }
                    "poses" => library::Tab::Poses,
                    "community" => library::Tab::Community,
                    _ => return Err("animations, builtin, poses or community".into()),
                }
            }
            "source" => self.lib_view.filters.source = library::filter::parse_source(value)?,
            "sort" => self.lib_view.filters.order = library::filter::Order::parse(value)?,
            "search" => self.lib_filter = value.to_string(),
            "dope" => self.timeline.dope = on()?,
            "overlay" => self.show_overlay = on()?,
            "ghost" => self.show_ghost = on()?,
            "character" => self.character.view = character::CharacterView::parse(value)?,
            "playing" => self.playing = on()?,
            "tool" => self.set_tool(viewport::Tool::parse(value)?),
            "camera" => self.view_preset(value)?,
            "zoom" => {
                let d: f32 = value.parse().map_err(|e| format!("{e}"))?;
                if !(0.5..=40.0).contains(&d) {
                    return Err("0.5 to 40 metres".into());
                }
                self.camera.distance = d;
            }
            "dialog" => self.open_dialog(value)?,
            "community" => {
                self.lib_tab = library::Tab::Community;
                let st = &mut self.community;
                st.show_mine = value == "mine";
                st.selected = (!matches!(value, "mine" | "none")).then(|| value.to_string());
                if st.show_mine {
                    self.community_mine();
                }
            }
            "share" => self.community_share_open(value),
            "community_window" => self.community_float(on()?),
            "community_view" => self.community.view = community::window::View::parse(value)?,
            "community_card" => {
                let w: f32 = value.parse().map_err(|e| format!("{value:?}: {e}"))?;
                if !w.is_finite() {
                    return Err(format!("{value:?}: a width in points"));
                }
                self.community.card = w.clamp(community::window::CARD_MIN, community::window::CARD_MAX);
            }
            _ => return self.set_view_ui(name, value),
        }
        Ok(())
    }

    fn refresh_library(&mut self) {
        self.library = match self.session.run("library.list", &json!({})) {
            Ok(v) => v,
            Err(e) => {
                self.status = format!("the library can't be listed: {e}");
                Value::Null
            }
        };
        self.lib_view.preview.cache.clear();
        // items may have changed: thumbnails come back from the disk cache, or are redrawn
        self.thumbs.cache.clear();
        self.library_synced();
    }

    /// Run a command; a refusal shows in the status bar, a toast and the messages, under the
    /// command's title.
    fn run(&mut self, id: &str, params: Value) -> Option<Value> {
        // motion from text that would wait for the model is made off the UI thread first
        if self.text_deferred(id, &params) {
            return None;
        }
        // and a pose fill (its model works a moment, and minutes the first time)
        if self.posefill_deferred(id, &params) {
            return None;
        }
        // and so is anything from the community (it waits on the network)
        if self.community_deferred(id, &params) {
            return None;
        }
        let before = self.shell.perf.before(&self.session);
        let out = self.session.run(id, &params);
        self.shell.perf.after(before, id, &self.session);
        let told = match &out {
            Ok(v) => Ok(v.clone()),
            Err(e) => Err(e.to_string()),
        };
        self.announce(id, "ui", &told);
        match out {
            Ok(v) => {
                self.fill_marks(id, &v);
                Some(v)
            }
            Err(e) => {
                self.refused(id, &e);
                None
            }
        }
    }

    /// The headless renderer for pictures agents ask for and library thumbnails (made on first
    /// use).
    fn ensure_offscreen(&mut self) -> Result<(), String> {
        if self.offscreen.is_none() {
            let glb = std::fs::read(self.assets.join("rig/mannequin.glb"))
                .map_err(|e| format!("can't read the mannequin: {e}"))?;
            self.offscreen = Some(ac_render::offscreen::Offscreen::new(
                &Mesh::from_glb(&glb)?,
                &self.session.document.skeleton.names,
            )?);
        }
        Ok(())
    }

    fn joints(&self) -> Vec<Vec3> {
        self.session
            .document
            .joints()
            .unwrap_or_default()
            .iter()
            .map(|p| Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32))
            .collect()
    }
}

fn png_bytes(image: &egui::ColorImage) -> Result<Vec<u8>, String> {
    ac_render::offscreen::png(image.size[0] as u32, image.size[1] as u32, image.as_raw())
}

fn save_png(path: &Path, image: &egui::ColorImage) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut enc = png::Encoder::new(
        std::io::BufWriter::new(file),
        image.size[0] as u32,
        image.size[1] as u32,
    );
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(|e| e.to_string())?;
    w.write_image_data(image.as_raw()).map_err(|e| e.to_string())
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // a close with unsaved changes asks first (`app/shell/mod.rs`)
        self.close_requested(ctx);
    }

    fn on_exit(&mut self) {
        self.exit();
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.play_input(ctx, raw);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.serve_control(&ctx);
        self.files_input(&ctx);
        if self.shots_pending.load(Ordering::SeqCst) > 0 {
            ctx.request_repaint();
        }
        // theme and scale; shortcuts and app actions from the keymap (`app/shell/keys.rs`)
        self.begin_frame(&ctx);
        self.advance_playback(&ctx);

        if !self.view.hide_menu {
            egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        }
        if !self.view.hide_status {
            egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        }
        // library, inspector, timeline and the viewport (`app/shell/layout.rs`)
        self.panels(ui, frame);
        self.confirm_dialog(&ctx);
        self.windows(&ctx);
        self.files_windows(&ctx);

        // --screenshot: wait for a few settled frames, grab the window, write it, quit
        let sized = self.screenshot.is_none() || self.window_sized(&ctx);
        if let Some((path, state)) = &self.screenshot {
            self.frames_drawn += 1;
            let busy = self.thumbs.busy || self.community.loading();
            match state.load(Ordering::SeqCst) {
                // settled: a few frames drawn and the thumbnails and community strips on screen
                // ready (or a long wait)
                0 if sized && self.frames_drawn > 5 && (!busy || self.frames_drawn > 600) => {
                    state.store(1, Ordering::SeqCst);
                    let (path, state) = (path.clone(), state.clone());
                    ctx.request_screenshot(move |img| {
                        let ok = match save_png(&path, &img) {
                            Ok(()) => true,
                            Err(e) => {
                                crate::failed(format!("--screenshot: {e}"));
                                false
                            }
                        };
                        state.store(if ok { 2 } else { 3 }, Ordering::SeqCst);
                    });
                }
                2 | 3 => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                _ => {}
            }
            ctx.request_repaint();
        }
        self.recording(&ctx);
        self.publish_widgets(&ctx);
        self.finish_input(&ctx);
        self.end_frame(&ctx);
        self.publish_changes();
    }
}
