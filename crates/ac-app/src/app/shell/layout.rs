//! The panels around the viewport: library and inspector at the sides (either way round), the
//! timeline below, and `craft_ui::docking` hosting the 3D viewport canvas alongside any dockable
//! or pop-out tool panels (`Range`, `Picker`, `Joint`, `Layers`, `Physics`, `Review`, `Scene`, `AI`,
//! `Text`, `Pins`). With `craft-ui`'s `native-windows` feature, any floating panel opens in its
//! own native OS window so it can be dragged onto a separate monitor and docked back by dragging
//! its title bar into the main window (or clicking its header button).

use craft_ui::docking::{
    Action, Column, DockArea, DockContent, DockStyle, Group, HidePanels, Layout as DockLayout, Node,
    PanelLimits, Permissions, Side as DockSide, Target, WindowKind, Workspace,
};
use craft_ui::layout::{SplitAxis, SplitSize};
use serde::{Deserialize, Serialize};

use super::super::App;
use super::super::prefs::{Layout, Side};
use crate::app::ui_ids::Tag;

/// Every panel that `craft_ui::docking` can dock, tab, split or float onto a second monitor.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "snake_case")]
pub enum DockPanel {
    Viewport,
    Range,
    Picker,
    Joint,
    Layers,
    Physics,
    Review,
    Scene,
    Ai,
    Text,
    PoseFill,
    Pins,
}

impl DockPanel {
    /// All tool panels (everything except the protected `Viewport` canvas).
    pub const TOOL_PANELS: &[DockPanel] = &[
        DockPanel::Range,
        DockPanel::Picker,
        DockPanel::Joint,
        DockPanel::Layers,
        DockPanel::Physics,
        DockPanel::Review,
        DockPanel::Scene,
        DockPanel::Ai,
        DockPanel::Text,
        DockPanel::PoseFill,
        DockPanel::Pins,
    ];

    pub const fn id_str(self) -> &'static str {
        match self {
            DockPanel::Viewport => "viewport",
            DockPanel::Range => "range",
            DockPanel::Picker => "picker",
            DockPanel::Joint => "joint",
            DockPanel::Layers => "layers",
            DockPanel::Physics => "physics",
            DockPanel::Review => "review",
            DockPanel::Scene => "scene",
            DockPanel::Ai => "ai",
            DockPanel::Text => "text",
            DockPanel::PoseFill => "posefill",
            DockPanel::Pins => "pins",
        }
    }

    pub const fn short_label(self) -> &'static str {
        match self {
            DockPanel::Viewport => "Viewport",
            DockPanel::Range => "Range",
            DockPanel::Picker => "Picker",
            DockPanel::Joint => "Joint",
            DockPanel::Layers => "Layers",
            DockPanel::Physics => "Physics",
            DockPanel::Review => "Review",
            DockPanel::Scene => "Scene boxes",
            DockPanel::Ai => "AI in-between",
            DockPanel::Text => "Motion from text",
            DockPanel::PoseFill => "Pose fill",
            DockPanel::Pins => "Pins",
        }
    }

    pub fn title(self, app: &App) -> String {
        match self {
            DockPanel::Viewport => "Viewport".into(),
            DockPanel::Range => {
                let n = app.panels.range.outside_frames;
                if n > 0 {
                    format!("Range ({n} past limit)")
                } else {
                    "Range".into()
                }
            }
            DockPanel::Picker => "Picker".into(),
            DockPanel::Joint => "Joint".into(),
            DockPanel::Layers => {
                format!("Layers ({})", app.session.document.clip.layers.len())
            }
            DockPanel::Physics => {
                let n = app.panels.physics.marks.len();
                if n > 0 {
                    format!("Physics ({n} problems)")
                } else {
                    "Physics".into()
                }
            }
            DockPanel::Review => {
                let n = app.panels.review.count();
                if n > 0 {
                    format!("Review ({n})")
                } else {
                    "Review".into()
                }
            }
            DockPanel::Scene => {
                format!("Scene boxes ({})", app.session.document.clip.surfaces.len())
            }
            DockPanel::Ai => "AI in-between".into(),
            DockPanel::Text => "Motion from text".into(),
            DockPanel::PoseFill => "Pose fill".into(),
            DockPanel::Pins => "Pins".into(),
        }
    }
}

pub fn dock_permissions(panel: &DockPanel) -> Permissions {
    match panel {
        DockPanel::Viewport => Permissions {
            accepts_splits: true,
            ..Permissions::PROTECTED
        },
        _ => Permissions::default(),
    }
}

/// Default docking workspace: `Viewport` is the stripless central canvas, and all tool panels have
/// their home location in a right-hand dock column (`DockSide::End`), hidden until toggled on.
pub fn default_dock_workspace() -> Workspace<DockPanel> {
    let canvas = Node::Tabs {
        panels: vec![DockPanel::Viewport],
        active: 0,
    };
    let dock = Node::Dock {
        side: DockSide::End,
        columns: vec![Column::new(
            vec![
                Group::new(vec![DockPanel::Range], 0),
                Group::new(vec![DockPanel::Picker], 0),
                Group::new(
                    vec![
                        DockPanel::Joint,
                        DockPanel::Layers,
                        DockPanel::Physics,
                        DockPanel::Review,
                        DockPanel::Scene,
                        DockPanel::Ai,
                        DockPanel::Text,
                        DockPanel::PoseFill,
                        DockPanel::Pins,
                    ],
                    0,
                ),
            ],
            300.0,
        )],
    };
    let layout = DockLayout {
        root: Some(Node::Split {
            axis: SplitAxis::Horizontal,
            size: SplitSize::FixedSecond(300.0),
            first: Box::new(canvas),
            second: Box::new(dock),
        }),
        floating: Vec::new(),
    };
    let mut ws = Workspace::new(layout);
    for &p in DockPanel::TOOL_PANELS {
        let _ = ws.set_visible(p, false, None, dock_permissions);
    }
    ws
}

pub fn load_dock_workspace(json_str: &str) -> Workspace<DockPanel> {
    let def = default_dock_workspace();
    if json_str.trim().is_empty() {
        return def;
    }
    match serde_json::from_str::<Workspace<DockPanel>>(json_str) {
        Ok(mut ws) => {
            ws.reconcile(|_| true, dock_permissions, &def);
            ws
        }
        Err(_) => def,
    }
}

struct DockAdapter<'a> {
    app: &'a mut App,
    to_float: Option<DockPanel>,
    to_dock: Option<DockPanel>,
    to_close: Option<DockPanel>,
}

impl DockContent<DockPanel> for DockAdapter<'_> {
    fn body(&mut self, ui: &mut egui::Ui, panel: &DockPanel) {
        match panel {
            DockPanel::Viewport => {
                let rect = self.app.viewport(ui);
                self.app.shell.viewport_rect = Some(rect);
            }
            DockPanel::Range => {
                egui::ScrollArea::vertical()
                    .id_salt("dock-range")
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.app.range_panel(ui));
            }
            DockPanel::Picker => {
                egui::ScrollArea::vertical()
                    .id_salt("dock-picker")
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.app.picker_body(ui));
            }
            DockPanel::Joint => {
                egui::ScrollArea::vertical()
                    .id_salt("dock-joint")
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.app.joint_panel(ui));
            }
            DockPanel::Layers => {
                egui::ScrollArea::vertical()
                    .id_salt("dock-layers")
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.app.layers_panel(ui));
            }
            DockPanel::Physics => {
                egui::ScrollArea::vertical()
                    .id_salt("dock-physics")
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.app.physics_panel(ui));
            }
            DockPanel::Review => {
                egui::ScrollArea::vertical()
                    .id_salt("dock-review")
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.app.review_panel(ui));
            }
            DockPanel::Scene => {
                egui::ScrollArea::vertical()
                    .id_salt("dock-scene")
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.app.scene_panel(ui));
            }
            DockPanel::Ai => {
                egui::ScrollArea::vertical()
                    .id_salt("dock-ai")
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.app.ai_panel(ui));
            }
            DockPanel::Text => {
                egui::ScrollArea::vertical()
                    .id_salt("dock-text")
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.app.text_panel(ui));
            }
            DockPanel::PoseFill => {
                egui::ScrollArea::vertical()
                    .id_salt("dock-posefill")
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.app.posefill_panel(ui));
            }
            DockPanel::Pins => {
                egui::ScrollArea::vertical()
                    .id_salt("dock-pins")
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.app.pins_panel(ui));
            }
        }
    }

    fn logical_label(&self, panel: &DockPanel) -> String {
        panel.title(self.app)
    }

    fn permissions(&self, panel: &DockPanel) -> Permissions {
        dock_permissions(panel)
    }

    fn limits(&self, panel: &DockPanel) -> PanelLimits {
        match panel {
            DockPanel::Viewport => PanelLimits {
                min: egui::vec2(200.0, 160.0),
                ..PanelLimits::default()
            },
            DockPanel::Picker => PanelLimits {
                min: egui::vec2(200.0, 180.0),
                ..PanelLimits::default()
            },
            _ => PanelLimits {
                min: egui::vec2(220.0, 160.0),
                ..PanelLimits::default()
            },
        }
    }

    fn floating_size(&self, panel: &DockPanel) -> Option<egui::Vec2> {
        Some(match panel {
            DockPanel::Range => egui::vec2(340.0, 440.0),
            DockPanel::Picker => egui::vec2(220.0, 380.0),
            _ => egui::vec2(300.0, 360.0),
        })
    }

    /// Palettes stay above the main window while the app has focus (a `Normal` window could open
    /// or fall behind it), and drop to the normal level, still shown, while another app has focus.
    fn window_kind(&self, _panel: &DockPanel) -> WindowKind {
        WindowKind::Palette
    }

    fn group_header_width(
        &self,
        _panels: &[DockPanel],
        active: &DockPanel,
        _floating: bool,
        _strip_width: f32,
    ) -> f32 {
        if *active == DockPanel::Viewport { 0.0 } else { 64.0 }
    }

    fn group_header(&mut self, ui: &mut egui::Ui, _panels: &[DockPanel], active: &DockPanel, floating: bool) {
        if *active == DockPanel::Viewport {
            return;
        }
        let id = active.id_str();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            if floating {
                if ui
                    .small_button("Dock")
                    .on_hover_text("Dock this panel back into the main window")
                    .tag(format!("dock.header.dock.{id}"), "Dock")
                    .clicked()
                {
                    self.to_dock = Some(*active);
                }
            } else if ui
                .small_button("Pop")
                .on_hover_text("Pop out into a floating OS window (can be dragged to a second monitor)")
                .tag(format!("dock.header.float.{id}"), "Pop")
                .clicked()
            {
                self.to_float = Some(*active);
            }
            if ui
                .small_button("×")
                .on_hover_text("Close this panel")
                .tag(format!("dock.header.close.{id}"), "Close")
                .clicked()
            {
                self.to_close = Some(*active);
            }
        });
    }
}

impl App {
    /// Whether `panel` is currently open in the docking workspace (docked or floating).
    pub(in crate::app) fn is_dock_panel_open(&self, panel: DockPanel) -> bool {
        self.shell.dock.layout.contains(&panel)
    }

    /// Show or hide `panel` in the docking workspace and keep `view.range` / `view.picker` in sync.
    pub(in crate::app) fn set_dock_panel(&mut self, panel: DockPanel, open: bool) {
        let _ = self.shell.dock.set_visible(panel, open, None, dock_permissions);
        self.sync_dock_flags();
    }

    /// Pop `panel` out into its own floating window (a native OS window on desktop).
    pub(in crate::app) fn float_dock_panel(&mut self, panel: DockPanel) {
        if !self.is_dock_panel_open(panel) {
            let _ = self.shell.dock.set_visible(panel, true, None, dock_permissions);
        }
        let size = match panel {
            DockPanel::Range => [340.0, 440.0],
            DockPanel::Picker => [220.0, 380.0],
            _ => [300.0, 360.0],
        };
        let _ = self.shell.dock.apply(
            Action::MovePanel {
                panel,
                to: Target::Float {
                    rect: Some([120.0, 90.0, size[0], size[1]]),
                },
            },
            dock_permissions,
        );
        self.sync_dock_flags();
    }

    /// Dock a floating `panel` back into the right-hand dock column.
    pub(in crate::app) fn dock_panel_home(&mut self, panel: DockPanel) {
        let _ = self.shell.dock.apply(
            Action::MovePanel {
                panel,
                to: Target::NewDock { side: DockSide::End },
            },
            dock_permissions,
        );
        self.sync_dock_flags();
    }

    pub(in crate::app) fn sync_dock_flags(&mut self) {
        self.view.range = self.shell.dock.layout.contains(&DockPanel::Range);
        self.view.picker = self.shell.dock.layout.contains(&DockPanel::Picker);
        if !self.view.picker {
            self.layout["picker"] = serde_json::Value::Null;
        }
    }

    /// The panels and the viewport, laid out as the preferences and docking workspace say.
    pub(in crate::app) fn panels(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Background panel work runs every frame whether the Inspector or a dock panel is open.
        self.ai_poll();
        self.text_poll(ui.ctx());
        self.posefill_poll(ui.ctx());
        self.community_poll(ui.ctx());
        self.community_window(ui.ctx());
        self.community_share_window(ui.ctx());
        self.physics_refresh();
        self.character_refresh();
        self.range_refresh();
        self.fill_marks_drop_stale();
        self.sync_dock_flags();

        let l = self.shell.prefs.get().layout.clone();
        let full = self.shell.maximised;
        let g = self.shell.layout_gen;
        let id = |name: &'static str| (name, g, 0u32);
        let mut sizes = (None, None, None);
        if l.show_timeline && !full {
            let natural = self.shell.timeline_natural;
            let manual = l.timeline_height > 0.0;
            if let Some(edge) = self.shell.timeline_rect {
                ui.ctx().input(|i| {
                    let on_edge =
                        |p: egui::Pos2| (p.y - edge.top()).abs() <= 5.0 && edge.x_range().contains(p.x);
                    if i.pointer.any_pressed() && i.pointer.press_origin().is_some_and(on_edge) {
                        self.shell.timeline_resizing = true;
                    }
                });
            }
            let resizing = self.shell.timeline_resizing;
            // a height of the person's own goes no taller than the timeline with the dope sheet
            // open (a little slack for the edge); a fitted one is its content's height already
            let extra = self.dope_extra();
            let tallest =
                ((manual || resizing) && natural > 0.0).then_some((natural + extra + 6.0).max(40.0));
            let mut p = egui::Panel::bottom(id("timeline")).resizable(true).min_size(40.0);
            if let Some(max) = tallest {
                p = p.max_size(max);
            }
            if manual {
                p = p.default_size(l.timeline_height);
            } else if resizing {
                p = p.default_size(natural.max(40.0));
            } else if natural > 0.0 {
                p = p.exact_size(natural);
            }
            let mut inner = None;
            let r = p.show(ui, |ui| {
                let out = egui::ScrollArea::vertical()
                    .id_salt("timeline-scroll")
                    // filling the panel: egui keeps (and reports) the frame's height, so a
                    // shrunk scroll area would put a dragged height back to the content's
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.timeline(ui));
                inner = Some((out.content_size.y, out.inner_rect.height()));
            });
            self.timeline.hovered = r.response.contains_pointer();
            self.shell.timeline_rect = Some(r.response.rect);
            let outer = r.response.rect.height();
            if let Some((content, shown)) = inner {
                self.shell.timeline_natural = content + (outer - shown).max(0.0);
            }
            self.shell.layout_settling =
                !manual && !resizing && (self.shell.timeline_natural - outer).abs() > 1.0;
            if manual || (resizing && (outer - natural).abs() > 2.0) {
                sizes.0 = Some(outer);
            }
            // dragged up as far as it goes with the dope sheet shut: point the button out, once a drag
            let at_top = tallest.is_some_and(|max| outer >= max - 1.0);
            if resizing && extra > 0.0 && at_top && !self.shell.timeline_hinted {
                self.shell.timeline_hinted = true;
                self.timeline.dope_hint = Some(ui.ctx().input(|i| i.time) + 3.0);
            }
        } else {
            self.shell.timeline_rect = None;
            self.shell.layout_settling = false;
        }
        if !l.show_timeline || full {
            self.timeline.hovered = false;
            self.timeline.focused = false;
        }
        let (lib_left, width_l, width_r) = match l.library_side {
            Side::Left => (true, l.library_width, l.inspector_width),
            Side::Right => (false, l.inspector_width, l.library_width),
        };
        if l.show_library && !full {
            let p = if lib_left {
                egui::Panel::left(id("library"))
            } else {
                egui::Panel::right(id("library"))
            };
            let w = if lib_left { width_l } else { width_r };
            let r = p
                .resizable(true)
                .default_size(w.clamp(120.0, 900.0))
                .show(ui, |ui| self.library_panel(ui));
            sizes.1 = Some(r.response.rect.width());
        }
        if l.show_inspector && !full {
            let p = if lib_left {
                egui::Panel::right(id("inspector"))
            } else {
                egui::Panel::left(id("inspector"))
            };
            let w = if lib_left { width_r } else { width_l };
            let r = p
                .resizable(true)
                .default_size(w.clamp(120.0, 900.0))
                .show(ui, |ui| self.inspector(ui));
            sizes.2 = Some(r.response.rect.width());
        }

        egui::CentralPanel::default().show(ui, |ui| {
            let mut style = DockStyle::from_ui(ui);
            style.drag_bar_width = 24.0;
            let layout_snapshot = self.shell.dock.layout.clone();
            let native_ok = self.screenshot.is_none() && self.control.is_none();
            let hide = if full { HidePanels::All } else { HidePanels::None };
            let mut adapter = DockAdapter {
                app: self,
                to_float: None,
                to_dock: None,
                to_close: None,
            };
            let out = DockArea::new(egui::Id::unique(("workspace-dock", g)))
                .input_blocked(adapter.app.confirm.is_some())
                .native_windows(native_ok)
                .hide_panels(hide)
                .show_content(ui, &layout_snapshot, &style, &mut adapter);
            let to_float = adapter.to_float;
            let to_dock = adapter.to_dock;
            let to_close = adapter.to_close;
            if !out.actions.is_empty() {
                let _ = self.shell.dock.apply_all(out.actions, dock_permissions);
                self.sync_dock_flags();
            }
            if let Some(p) = to_float {
                self.float_dock_panel(p);
            }
            if let Some(p) = to_dock {
                self.dock_panel_home(p);
            }
            if let Some(p) = to_close {
                self.set_dock_panel(p, false);
            }
        });

        // sizes and docking workspace are saved once a resize/drag ends
        if !ui.ctx().input(|i| i.pointer.any_down()) {
            self.shell.timeline_resizing = false;
            self.shell.timeline_hinted = false;
            let dock_json = serde_json::to_string(&self.shell.dock).unwrap_or_default();
            self.shell.prefs.update(|p| {
                let near = |a: f32, b: f32| (a - b).abs() < 0.5;
                if let Some(h) = sizes.0.filter(|h| !near(*h, p.layout.timeline_height)) {
                    p.layout.timeline_height = h;
                }
                if let Some(w) = sizes.1.filter(|w| !near(*w, p.layout.library_width)) {
                    p.layout.library_width = w;
                }
                if let Some(w) = sizes.2.filter(|w| !near(*w, p.layout.inspector_width)) {
                    p.layout.inspector_width = w;
                }
                if p.layout.dock_json != dock_json {
                    p.layout.dock_json = dock_json;
                }
            });
        }
    }

    /// View > Reset layout: default sizes and places, every panel returned to its default home.
    pub(in crate::app) fn reset_layout(&mut self, ctx: &egui::Context) {
        self.shell.dock = default_dock_workspace();
        self.sync_dock_flags();
        self.shell.prefs.update(|p| p.layout = Layout::default());
        self.shell.maximised = false;
        // new panel ids: egui forgets the sizes it kept for the old ones
        self.shell.layout_gen = self.shell.layout_gen.wrapping_add(1);
        ctx.request_repaint();
        self.status = "layout reset".into();
    }

    /// Switch between Simple mode (beginner essentials) and Advanced mode (full tool panels).
    pub(in crate::app) fn set_ui_mode(&mut self, mode: super::super::prefs::UiMode, ctx: &egui::Context) {
        self.shell.prefs.update(|p| p.layout.ui_mode = mode);
        match mode {
            super::super::prefs::UiMode::Simple => {
                for &p in &[
                    DockPanel::Layers,
                    DockPanel::Physics,
                    DockPanel::Review,
                    DockPanel::Scene,
                    DockPanel::Ai,
                    DockPanel::Text,
                    DockPanel::PoseFill,
                    DockPanel::Pins,
                    DockPanel::Range,
                ] {
                    let _ = self.shell.dock.set_visible(p, false, None, dock_permissions);
                }
                self.status = "switched to simple mode".into();
            }
            super::super::prefs::UiMode::Advanced => {
                self.status = "switched to advanced mode".into();
            }
        }
        self.sync_dock_flags();
        ctx.request_repaint();
    }
}
