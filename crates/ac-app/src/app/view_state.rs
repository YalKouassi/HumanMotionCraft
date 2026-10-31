//! The viewport's own window state (not the document's): onion skin, motion trails, the picker,
//! which joint handles show, the rotate gizmo's axes, the camera's follow and projection, and the
//! joints selected together. Set from the toolbar, the viewport keys, `--ui name=value` and the
//! control port's `ui` op.

use std::collections::BTreeSet;

use super::App;
use super::gizmo::Turning;
use super::onion::OnionCache;
use super::trail::TrailCache;

/// Which frames the onion skin ghosts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OnionMode {
    /// Every `onion_step` frames.
    Frames,
    /// The nearest keys.
    Keys,
}

/// Most frames a trail reaches each side of the playhead (0 is the whole animation).
pub const MAX_TRAIL_SPAN: u32 = 1000;
/// Most frames between onion ghosts.
pub const MAX_ONION_STEP: u32 = 100;

pub struct ViewAids {
    pub onion: bool,
    /// Ghosts on each side (1 to `ac_core::commands::view::MAX_ONION`).
    pub onion_count: u32,
    pub onion_step: u32,
    pub onion_mode: OnionMode,
    pub trails: bool,
    /// Frames each side of the playhead a trail shows (0: the whole animation).
    pub trail_span: u32,
    /// The body-part picker panel (docked or floating).
    pub picker: bool,
    /// The coupled range-of-motion panel (docked or floating).
    pub range: bool,
    /// `--ui menu=off` and the like: the window with less of itself, for a picture of it.
    pub hide_menu: bool,
    pub hide_tools: bool,
    pub hide_status: bool,
    pub hide_joints: bool,
    /// Finger and face handles too (the rig's `game` profile, not just `body`).
    pub all_handles: bool,
    /// The rotate gizmo turns about the joint's own axes (else the world's).
    pub local: bool,
    /// The camera keeps the body in the middle as it moves.
    pub follow: bool,
    /// Joints selected together (picker, Shift+click); `App::selected` is the active one.
    pub picked: BTreeSet<usize>,
    pub turning: Option<Turning>,
    /// The pointer was over the viewport last frame (its context keys work).
    pub hovered: bool,
    pub onion_cache: Option<OnionCache>,
    pub trail_cache: Option<TrailCache>,
}

impl Default for ViewAids {
    fn default() -> Self {
        ViewAids {
            onion: false,
            onion_count: 2,
            onion_step: 2,
            onion_mode: OnionMode::Frames,
            trails: false,
            trail_span: 0,
            picker: false,
            range: false,
            hide_menu: false,
            hide_tools: false,
            hide_status: false,
            hide_joints: false,
            all_handles: false,
            local: false,
            follow: false,
            picked: BTreeSet::new(),
            turning: None,
            hovered: false,
            onion_cache: None,
            trail_cache: None,
        }
    }
}

/// The names `--ui` (and the control port's `ui` op) take, the app's own first.
pub const UI_NAMES: &str = "tab, search, dope, overlay, ghost, character, playing, tool, camera, zoom, onion, onion_count, onion_step, trails, trail_span, picker, range, handles, gizmo, ortho, follow, select";

fn on(value: &str) -> Result<bool, String> {
    match value {
        "on" | "true" | "1" => Ok(true),
        "off" | "false" | "0" => Ok(false),
        _ => Err(format!("{value:?}: on or off")),
    }
}

fn number(value: &str, lo: u32, hi: u32) -> Result<u32, String> {
    value
        .parse::<u32>()
        .ok()
        .filter(|n| (lo..=hi).contains(n))
        .ok_or_else(|| format!("{value:?}: a whole number from {lo} to {hi}"))
}

impl App {
    /// The rig profile whose joints get handles.
    pub(super) fn handle_profile(&self) -> &'static str {
        if self.view.all_handles { "game" } else { "body" }
    }

    /// Which joints get a handle (all of them when the rig has no such profile).
    pub(super) fn handles(&self) -> Vec<bool> {
        let sk = &self.session.document.skeleton;
        let profile = sk.profiles.get(self.handle_profile());
        sk.names
            .iter()
            .map(|n| profile.is_none_or(|p| p.contains(n)))
            .collect()
    }

    /// Select `bone` alone, or (`add`) toggle it in the joints selected together.
    pub(super) fn select_joint(&mut self, bone: Option<usize>, add: bool) {
        match (bone, add) {
            (Some(b), true) => {
                if self.view.picked.remove(&b) {
                    if self.selected == Some(b) {
                        self.selected = self.view.picked.iter().next().copied();
                    }
                } else {
                    if let Some(s) = self.selected {
                        self.view.picked.insert(s);
                    }
                    self.view.picked.insert(b);
                    self.selected = Some(b);
                }
            }
            (Some(b), false) => {
                self.view.picked = BTreeSet::from([b]);
                self.selected = Some(b);
            }
            (None, true) => {}
            (None, false) => {
                self.view.picked.clear();
                self.selected = None;
            }
        }
    }

    /// The joints selected: the picked set, or the active joint alone.
    pub(super) fn selection(&self) -> Vec<usize> {
        let mut s = self.view.picked.clone();
        s.extend(self.selected);
        s.into_iter().collect()
    }

    /// Set a viewport state by name (see [`UI_NAMES`]).
    pub(super) fn set_view_ui(&mut self, name: &str, value: &str) -> Result<(), String> {
        let v = &mut self.view;
        match name {
            "onion" => match value {
                "keys" => (v.onion, v.onion_mode) = (true, OnionMode::Keys),
                "frames" => (v.onion, v.onion_mode) = (true, OnionMode::Frames),
                _ => v.onion = on(value).map_err(|e| format!("{e}, keys or frames"))?,
            },
            "onion_count" => v.onion_count = number(value, 1, ac_core::commands::view::MAX_ONION as u32)?,
            "onion_step" => v.onion_step = number(value, 1, MAX_ONION_STEP)?,
            "trails" => v.trails = on(value)?,
            "trail_span" => v.trail_span = number(value, 0, MAX_TRAIL_SPAN)?,
            "picker" => {
                let b = on(value)?;
                self.set_dock_panel(super::shell::DockPanel::Picker, b);
            }
            "range" => {
                let b = on(value)?;
                self.set_dock_panel(super::shell::DockPanel::Range, b);
            }
            "handles" => {
                v.all_handles = match value {
                    "body" => false,
                    "all" => true,
                    _ => return Err("body or all (fingers and face too)".into()),
                }
            }
            "gizmo" => {
                v.local = match value {
                    "world" => false,
                    "local" => true,
                    _ => return Err("world or local".into()),
                }
            }
            "ortho" => self.camera.ortho = on(value)?,
            "follow" => v.follow = on(value)?,
            "select" => {
                if value == "none" || value.is_empty() {
                    self.select_joint(None, false);
                } else {
                    let mut bones = Vec::new();
                    for n in value.split(',') {
                        bones.push(
                            self.session
                                .document
                                .bone(n.trim())
                                .map_err(|e| format!("{e} (bone names, comma-separated, or none)"))?,
                        );
                    }
                    self.view.picked = bones.iter().copied().collect();
                    self.selected = bones.first().copied();
                }
            }
            _ => return self.set_panel_ui(name, value),
        }
        Ok(())
    }
}
