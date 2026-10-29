//! The inspector's panels, one file each: the selected joint, layers, physics report, scene boxes,
//! the AI in-between (with its worker thread) and motion from text (with its own, and its install). Each reads session state and acts through
//! `Session::run`; this module only holds the window state they keep between frames.

pub mod ai;
pub mod ai_worker;
pub mod joint;
pub mod layers;
pub mod physics;
pub mod scene;
pub mod text;
pub mod text_install;

/// What the panels remember between frames (window state, never saved).
#[derive(Default)]
pub struct Panels {
    pub joint: joint::JointState,
    pub ai: ai::AiState,
    pub physics: physics::PhysicsState,
    pub scene: scene::SceneState,
    pub layers: layers::LayersState,
    pub text: text::TextState,
}

impl super::App {
    /// The panels' window state for `--ui` and the control port: `joint=<bone>` selects a joint,
    /// `box=<n>` picks a scene box, `layer=<n>` picks a layer, `prompt=<text>` fills in motion from
    /// text's prompt.
    pub(in crate::app) fn set_panel_ui(&mut self, name: &str, value: &str) -> Result<(), String> {
        let index = || value.parse::<usize>().map_err(|e| format!("{value:?}: {e}"));
        match name {
            "joint" => self.selected = Some(self.session.document.bone(value)?),
            "box" => {
                let i = index()?;
                if i >= self.session.document.clip.surfaces.len() {
                    return Err(format!("no box {i}"));
                }
                self.panels.scene.selected = Some(i);
            }
            "layer" => {
                let i = index()?;
                if i >= self.session.document.clip.layers.len() {
                    return Err(format!("no layer {i}"));
                }
                self.panels.layers.pick(i);
            }
            "prompt" => self.panels.text.prompt = value.to_string(),
            "travel" => match value {
                "preview" => self.preview_travel(),
                "off" => self.panels.layers.clear_travel(),
                _ => return Err("preview or off".into()),
            },
            _ => {
                return self.set_timeline_ui(name, value);
            }
        }
        Ok(())
    }
}

/// Run `edit.begin` when a drag on `r` starts and `edit.end` when it stops, so a drag is one undo
/// step; returns whether the value changed this frame.
pub(in crate::app) fn gesture(app: &mut super::App, r: &egui::Response, label: &str) -> bool {
    if r.drag_started() {
        app.run("edit.begin", serde_json::json!({ "label": label }));
    }
    r.changed()
}

/// Close the drag's undo step once the pointer lets go (after the change was run).
pub(in crate::app) fn gesture_end(app: &mut super::App, r: &egui::Response) {
    if r.drag_stopped() {
        app.run("edit.end", serde_json::json!({}));
    }
}
