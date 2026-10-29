//! The viewport's toolbar (the tools; the overlays; the picker and the camera). Its keys are in
//! the keymap (the tools' `app.tool_*`; the camera's `app.view_*`, which work while the pointer is
//! over the viewport: `shell/scoped.rs`), and the tooltips and menu read them from there, so they
//! can't disagree.

use egui::{Rect, Vec2};

use super::App;
use super::ui_ids::Tag;
use super::view_state::{MAX_ONION_STEP, MAX_TRAIL_SPAN, OnionMode};
use super::viewport::Tool;

/// Each tool: its button, its keymap action and what it does.
pub const TOOLS: &[(Tool, &str, &str, &str)] = &[
    (
        Tool::Drag,
        "Drag",
        "app.tool_drag",
        "Drag a joint: hands and feet reach, other joints turn their parent",
    ),
    (
        Tool::Move,
        "Move",
        "app.tool_move",
        "Drag to slide the body on this frame over the floor (its key moves too), Shift+drag to turn it about the root; Alt+drag moves the whole animation",
    ),
    (
        Tool::Rotate,
        "Rotate",
        "app.tool_rotate",
        "Turn the selected joint: drag a ring (red X, green Y, blue Z), the outer ring to turn about your line of sight, or inside to turn freely; Ctrl snaps to 15°",
    ),
    (
        Tool::Reach,
        "IK",
        "app.tool_reach",
        "Drag a joint and the whole body helps it reach; pinned joints stay (right-click a joint to pin it)",
    ),
];

/// The camera's named views in the View menu: (action, label), in pairs.
const VIEWS: &[(&str, &str)] = &[
    ("app.view_front", "Front"),
    ("app.view_back", "Back"),
    ("app.view_right", "Right side"),
    ("app.view_left", "Left side"),
    ("app.view_top", "Top"),
    ("app.view_bottom", "Bottom"),
    ("app.view_three_quarter", "Three-quarter"),
];

impl App {
    /// Switch tools; a turn in progress ends first.
    pub(super) fn set_tool(&mut self, tool: Tool) {
        if self.tool != tool {
            self.end_turn();
            self.tool = tool;
        }
    }

    /// The viewport's own switches, over its top-left corner.
    pub(super) fn viewport_toolbar(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let bar = Rect::from_min_size(
            rect.min + Vec2::new(8.0, 8.0),
            Vec2::new((rect.width() - 16.0).max(60.0), 56.0),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(bar), |ui| {
            // on a backing of the panel colour, so it reads over the 3D view in either theme
            let backing = ui.visuals().panel_fill.gamma_multiply(0.9);
            egui::Frame::new()
                .fill(backing)
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(4, 2))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 2.0);
                    ui.horizontal_wrapped(|ui| {
                        // the tools
                        for (tool, label, action, tip) in TOOLS {
                            let on = self.tool == *tool;
                            let key = self.shortcut_text(action);
                            if ui
                                .selectable_label(on, *label)
                                .on_hover_text(format!("{tip} ({key})"))
                                .tag(format!("toolbar.tool.{}", tool.name()), *label)
                                .clicked()
                            {
                                self.set_tool(*tool);
                            }
                        }
                        if self.tool == Tool::Rotate {
                            ui.toggle_value(&mut self.view.local, "Local")
                                .tag("toolbar.local", "Local")
                                .on_hover_text(
                                    "Rings follow the joint's own axes (off: the world's X, Y and Z)",
                                );
                        }
                        ui.separator();
                        // what is drawn over the body
                        ui.toggle_value(&mut self.show_overlay, "Physics")
                    .tag("toolbar.physics", "Physics")
                    .on_hover_text(
                    "Support area (green: balanced, red: falling), centre of mass, planted feet, jump arcs",
                );
                        ui.toggle_value(&mut self.show_ghost, "Keyed ghost")
                            .tag("toolbar.ghost", "Keyed ghost")
                            .on_hover_text("Where your keys put the body, in blue, where physics moved it");
                        ui.toggle_value(&mut self.view.onion, "Onion")
                            .tag("toolbar.onion", "Onion")
                            .on_hover_text(
                                "Ghosts of the keyed motion before (orange) and after (blue) the playhead",
                            );
                        // the settings buttons say what is set (the default font has no arrow glyphs)
                        let onion = match self.view.onion_mode {
                            OnionMode::Frames => {
                                format!("{}·{}f", self.view.onion_count, self.view.onion_step)
                            }
                            OnionMode::Keys => format!("{}·keys", self.view.onion_count),
                        };
                        ui.menu_button(onion.clone(), |ui| self.onion_menu(ui))
                            .response
                            .tag("toolbar.onion_settings", onion)
                            .on_hover_text("Onion skin: ghosts each side, every so many frames or on keys");
                        ui.toggle_value(&mut self.view.trails, "Trails")
                            .tag("toolbar.trails", "Trails")
                            .on_hover_text(
                                "The selected joint's path: a tick each frame, a yellow dot on each key",
                            );
                        let span = match self.view.trail_span {
                            0 => "all".to_string(),
                            n => format!("±{n}"),
                        };
                        ui.menu_button(span.clone(), |ui| self.trail_menu(ui))
                            .response
                            .tag("toolbar.trail_span", span)
                            .on_hover_text(
                                "Trail: the whole animation, or so many frames each side of the playhead",
                            );
                        ui.separator();
                        let range_on = self.is_dock_panel_open(super::shell::DockPanel::Range);
                        let range_out = self.panels.range.outside_frames;
                        let range_tip = if range_out > 0 {
                            format!(
                                "Coupled joint range-of-motion chart ({range_out} frames past clinical hard limit) — dock or drag to a second monitor"
                            )
                        } else {
                            "Coupled joint range-of-motion chart and clinical hard limits — dock or drag to a second monitor".into()
                        };
                        let range_txt = if range_out > 0 {
                            egui::RichText::new(format!("Range ({range_out})"))
                                .color(self.palette().error)
                        } else {
                            egui::RichText::new("Range")
                        };
                        if ui
                            .selectable_label(range_on, range_txt)
                            .tag("toolbar.range", "Range")
                            .on_hover_text(range_tip)
                            .clicked()
                        {
                            self.set_dock_panel(super::shell::DockPanel::Range, !range_on);
                        }
                        let picker_on = self.is_dock_panel_open(super::shell::DockPanel::Picker);
                        if ui
                            .selectable_label(picker_on, "Picker")
                            .tag("toolbar.picker", "Picker")
                            .on_hover_text(
                                "Pick joints and body parts on a small figure, several at once — dock or drag to a second monitor",
                            )
                            .clicked()
                        {
                            self.set_dock_panel(super::shell::DockPanel::Picker, !picker_on);
                        }
                        ui.menu_button("Panels", |ui| self.panels_menu(ui))
                            .response
                            .tag("toolbar.panels", "Panels")
                            .on_hover_text(
                                "Open any panel as a dockable tab or pop-out window for a second monitor",
                            );
                        ui.menu_button("View", |ui| self.view_menu(ui))
                            .response
                            .tag("toolbar.view", "View")
                            .on_hover_text("Camera views (keys work over the viewport)");
                    });
                });
        });
    }

    fn onion_menu(&mut self, ui: &mut egui::Ui) {
        let v = &mut self.view;
        ui.horizontal(|ui| {
            ui.radio_value(&mut v.onion_mode, OnionMode::Frames, "Every")
                .tag("toolbar.onion_settings.every", "Every")
                .on_hover_text("Ghosts a fixed number of frames apart");
            ui.add_enabled(
                v.onion_mode == OnionMode::Frames,
                egui::DragValue::new(&mut v.onion_step)
                    .range(1..=MAX_ONION_STEP)
                    .suffix(" frames"),
            )
            .tag("toolbar.onion_settings.step", "frames");
        });
        ui.radio_value(&mut v.onion_mode, OnionMode::Keys, "Keys")
            .tag("toolbar.onion_settings.keys", "Keys")
            .on_hover_text("Ghosts on the nearest keys");
        ui.add(
            egui::Slider::new(&mut v.onion_count, 1..=ac_core::commands::view::MAX_ONION as u32)
                .text("each side"),
        )
        .tag("toolbar.onion_settings.count", "each side");
    }

    fn trail_menu(&mut self, ui: &mut egui::Ui) {
        let v = &mut self.view;
        ui.radio_value(&mut v.trail_span, 0, "Whole animation")
            .tag("toolbar.trail_span.all", "Whole animation");
        let mut around = v.trail_span > 0;
        if ui
            .radio(around, "Around the playhead")
            .tag("toolbar.trail_span.around", "Around the playhead")
            .on_hover_text("Only the frames near the playhead")
            .clicked()
            && !around
        {
            around = true;
            v.trail_span = 15;
        }
        if around {
            ui.add(
                egui::DragValue::new(&mut v.trail_span)
                    .range(1..=MAX_TRAIL_SPAN)
                    .suffix(" frames each side"),
            )
            .tag("toolbar.trail_span.frames", "frames each side");
        }
    }

    fn view_menu(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut go: Option<&str> = None;
        for (i, (id, label)) in VIEWS.iter().enumerate() {
            if i % 2 == 0 && i > 0 {
                ui.separator();
            }
            let name = id.trim_start_matches("app.view_");
            if ui
                .add(egui::Button::new(*label).shortcut_text(self.shortcut_text(id)))
                .tag(format!("toolbar.view.{name}"), *label)
                .clicked()
            {
                go = Some(id);
            }
        }
        ui.separator();
        if ui
            .add(
                egui::Button::new("Frame selection").shortcut_text(self.shortcut_text("app.frame_selection")),
            )
            .tag("toolbar.view.frame_selection", "Frame selection")
            .on_hover_text("Fill the view with the selected joints, or the body when none is selected")
            .clicked()
        {
            go = Some("app.frame_selection");
        }
        let mut ortho = self.camera.ortho;
        if ui
            .add(egui::Checkbox::new(&mut ortho, "Orthographic"))
            .tag("toolbar.view.ortho", "Orthographic")
            .on_hover_text(format!(
                "No perspective: sizes and lines compare across the picture ({})",
                self.shortcut_text("app.ortho")
            ))
            .changed()
        {
            go = Some("app.ortho");
        }
        ui.checkbox(&mut self.view.follow, "Follow the body")
            .tag("toolbar.view.follow", "Follow the body")
            .on_hover_text("The camera keeps the body in the middle as it moves");
        if let Some(id) = go {
            self.act_scoped(id, &ctx);
        }
    }

    fn panels_menu(&mut self, ui: &mut egui::Ui) {
        let mut toggle: Option<(super::shell::DockPanel, bool)> = None;
        let mut float: Option<super::shell::DockPanel> = None;
        for &p in super::shell::DockPanel::TOOL_PANELS {
            let mut open = self.is_dock_panel_open(p);
            let id = p.id_str();
            let label = p.short_label();
            ui.horizontal(|ui| {
                if ui
                    .checkbox(&mut open, label)
                    .tag(format!("toolbar.panels.{id}"), label)
                    .on_hover_text("Show or hide this panel in the docking workspace")
                    .changed()
                {
                    toggle = Some((p, open));
                }
                if ui
                    .small_button("Pop")
                    .tag(format!("toolbar.panels.float.{id}"), "Pop")
                    .on_hover_text("Pop out into a floating OS window (drag to a second monitor)")
                    .clicked()
                {
                    float = Some(p);
                }
            });
        }
        if let Some((p, open)) = toggle {
            self.set_dock_panel(p, open);
        }
        if let Some(p) = float {
            self.float_dock_panel(p);
        }
    }
}
