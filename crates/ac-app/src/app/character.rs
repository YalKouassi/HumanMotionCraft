//! The person's own character in the viewport (roadmap 147000): once `retarget.load` put one in
//! the document, it is driven live through its mapping from the pose shown, beside the
//! mannequin (1.2 m to the rig's left) or over it. With a skinned mesh in its file it is drawn
//! skinned (the renderer's second mesh slot); with a skeleton only, its joints and bones are
//! drawn as lines. The frames where the retarget struggles (a planted foot that slides, a foot
//! under the floor, a leg that can't reach, the nodes nothing drives) are marks on the timeline
//! (`retarget`), from the same check `retarget.check` reports. Window state: nothing here is
//! saved or undone; the document holds only the character and its mapping.

use std::collections::BTreeMap;
use std::sync::Arc;

use ac_core::Pose;
use ac_core::character::World;
use ac_core::character::drive::{self, Weights};
use ac_core::character::plan::Plan;
use ac_core::clip::Clip;
use ac_render::glam::{Mat4, Vec3};
use egui::{Color32, Pos2, Stroke};

use super::App;
use super::timeline::marks::Mark;

/// The renderer's mesh slot the character's mesh lives in.
pub const MESH_SLOT: usize = 1;
/// The character's colour (a cool grey, apart from the mannequin's warm one).
pub const TINT: [f32; 4] = [0.52, 0.62, 0.8, 1.0];
/// How far to the rig's left (+X, metres) the character stands when beside the mannequin.
pub const BESIDE_X: f64 = 1.2;
const BONE: Color32 = Color32::from_rgb(130, 160, 230);
const JOINT: Color32 = Color32::from_rgb(200, 215, 255);

/// Where the character is shown.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CharacterView {
    Off,
    Beside,
    Over,
}

impl CharacterView {
    pub fn name(self) -> &'static str {
        match self {
            CharacterView::Off => "off",
            CharacterView::Beside => "beside",
            CharacterView::Over => "over",
        }
    }

    pub fn parse(s: &str) -> Result<CharacterView, String> {
        match s {
            "off" => Ok(CharacterView::Off),
            "beside" => Ok(CharacterView::Beside),
            "over" => Ok(CharacterView::Over),
            _ => Err("off, beside or over".into()),
        }
    }

    pub fn next(self) -> CharacterView {
        match self {
            CharacterView::Off => CharacterView::Beside,
            CharacterView::Beside => CharacterView::Over,
            CharacterView::Over => CharacterView::Off,
        }
    }
}

/// What the window keeps between frames for the character.
pub struct CharacterState {
    pub view: CharacterView,
    /// The plan, for the loaded character it was made for (by pointer).
    plan: Option<(usize, Plan)>,
    /// The character whose mesh is in the renderer's slot (by pointer), if it has one.
    mesh_for: Option<usize>,
    /// Why the character is drawn as bones although it has a mesh.
    pub mesh_error: Option<String>,
    /// The IK weights per frame, for (character, clip, links).
    clip_for: Option<(usize, Clip, BTreeMap<String, Clip>)>,
    weights: Vec<Weights>,
    /// Why there is no plan (a mapping not complete yet).
    pub error: Option<String>,
}

impl Default for CharacterState {
    fn default() -> Self {
        CharacterState {
            view: CharacterView::Beside,
            plan: None,
            mesh_for: None,
            mesh_error: None,
            clip_for: None,
            weights: Vec::new(),
            error: None,
        }
    }
}

/// The character on the frame shown: its nodes' worlds (moved beside the mannequin when so),
/// skinning matrices when it has a mesh, else its bones as lines.
pub struct Frame {
    pub skin: Option<Vec<Mat4>>,
    pub bones: Vec<(Vec3, Vec3)>,
    pub joints: Vec<Vec3>,
}

fn v3(p: [f64; 3]) -> Vec3 {
    Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32)
}

fn mat4(m: &[[f64; 4]; 4]) -> Mat4 {
    let mut cols = [[0.0f32; 4]; 4];
    for (r, row) in m.iter().enumerate() {
        for (c, x) in row.iter().enumerate() {
            cols[c][r] = *x as f32;
        }
    }
    Mat4::from_cols_array_2d(&cols)
}

impl App {
    /// Keep the plan, the mesh and the timeline marks in step with the document (every frame;
    /// cheap when nothing changed).
    pub(in crate::app) fn character_refresh(&mut self) {
        let Some(loaded) = self.session.document.character.clone() else {
            if self.character.plan.is_some() || self.character.mesh_for.is_some() {
                self.character = CharacterState {
                    view: self.character.view,
                    ..CharacterState::default()
                };
                self.renderer.clear_mesh(MESH_SLOT);
                self.timeline.marks.clear("retarget");
            }
            return;
        };
        let key = Arc::as_ptr(&loaded) as usize;
        if self.character.plan.as_ref().is_none_or(|(k, _)| *k != key) {
            match Plan::new(
                &self.session.document.skeleton,
                &loaded.character,
                &loaded.mapping,
            ) {
                Ok(p) => {
                    self.character.plan = Some((key, p));
                    self.character.error = None;
                }
                Err(e) => {
                    self.character.plan = None;
                    self.character.clip_for = None;
                    self.timeline.marks.clear("retarget");
                    let msg = format!("character not shown: {e}");
                    if self.character.error.as_deref() != Some(&msg) {
                        self.status = msg.clone();
                    }
                    self.character.error = Some(msg);
                }
            }
        }
        if self.character.mesh_for != Some(key) {
            self.character.mesh_for = Some(key);
            self.character.mesh_error = None;
            self.renderer.clear_mesh(MESH_SLOT);
            if loaded.character.has_mesh {
                let loaded_mesh = std::fs::read(&loaded.file)
                    .map_err(|e| format!("can't read {}: {e}", loaded.file.display()))
                    .and_then(|bytes| ac_render::Mesh::from_glb(&bytes))
                    .and_then(|mesh| {
                        if mesh.joints.len() != loaded.character.skin.len() {
                            return Err(format!(
                                "its mesh has {} skin joints, the skeleton {}",
                                mesh.joints.len(),
                                loaded.character.skin.len()
                            ));
                        }
                        self.renderer
                            .set_mesh(&self.render_state.device, MESH_SLOT, &mesh)
                    });
                if let Err(e) = loaded_mesh {
                    let msg = format!("the character is drawn as bones: {e}");
                    self.status = msg.clone();
                    self.character.mesh_error = Some(msg);
                }
            }
        }
        // the weights and the marks: once per version of the animation
        let doc = &self.session.document;
        let fresh = self
            .character
            .clip_for
            .as_ref()
            .is_some_and(|(k, c, l)| *k == key && *c == doc.clip && *l == doc.links);
        if fresh || self.session.in_gesture() || self.character.plan.is_none() {
            return;
        }
        self.character.clip_for = Some((key, doc.clip.clone(), doc.links.clone()));
        if doc.clip.base().keys.is_empty() {
            self.character.weights.clear();
            self.timeline.marks.clear("retarget");
            return;
        }
        match ac_io::character::check_document(&mut self.session.document) {
            Ok((_, report)) => {
                let pal = self.palette();
                let [penetration, sliding, _, other] = pal.problems;
                let marks: Vec<Mark> = report
                    .problems
                    .iter()
                    .map(|p| {
                        let colour = match p.kind {
                            "foot-slide" => sliding,
                            "below-floor" => penetration,
                            "unreached" => other,
                            _ => Color32::from_gray(140),
                        };
                        Mark::new(p.from as f64, p.kind, colour, &p.text).until(p.to as f64)
                    })
                    .collect();
                self.timeline.marks.set("retarget", marks);
                self.character.weights = report.weights;
            }
            Err(e) => {
                self.character.weights.clear();
                self.timeline.marks.clear("retarget");
                self.status = format!("character check: {e}");
            }
        }
    }

    /// The character driven from `pose` (the pose shown), for the viewport; `None` when there
    /// is none, it is off, or its mapping is not complete.
    pub(in crate::app) fn character_frame(&mut self, pose: &Pose) -> Option<Frame> {
        if self.character.view == CharacterView::Off {
            return None;
        }
        let loaded = self.session.document.character.clone()?;
        let (_, plan) = self.character.plan.as_ref()?;
        let doc = &self.session.document;
        let f = doc.frame.round().max(0.0) as usize;
        let weights = self.character.weights.get(f).copied().unwrap_or(Weights::FEET);
        let posed = match drive::drive(
            plan,
            &loaded.character,
            &doc.skeleton,
            &pose.rotations,
            pose.root,
            weights,
        ) {
            Ok(p) => p,
            Err(e) => {
                self.status = format!("character: {e}");
                return None;
            }
        };
        let shift = if self.character.view == CharacterView::Beside {
            [BESIDE_X, 0.0, 0.0]
        } else {
            [0.0; 3]
        };
        let worlds: Vec<World> = posed
            .worlds
            .iter()
            .map(|w| World {
                p: ac_core::quat::add3(w.p, shift),
                ..*w
            })
            .collect();
        let c = &loaded.character;
        let skin = self
            .renderer
            .has_mesh(MESH_SLOT)
            .then(|| c.skin_matrices(&worlds).iter().map(mat4).collect());
        let mut bones = Vec::new();
        let mut joints = Vec::new();
        for (i, n) in c.nodes.iter().enumerate() {
            if !c.under(i, plan.hips) {
                continue;
            }
            let Some(w) = worlds.get(i) else { continue };
            joints.push(v3(w.p));
            if let Some(p) = n.parent
                && c.under(p, plan.hips)
                && let Some(pw) = worlds.get(p)
            {
                bones.push((v3(pw.p), v3(w.p)));
            }
        }
        Some(Frame { skin, bones, joints })
    }

    /// Draw a skeleton-only character as lines and dots (a skinned one is in the picture).
    pub(in crate::app) fn draw_character(
        &self,
        painter: &egui::Painter,
        frame: &Frame,
        project: &dyn Fn(Vec3) -> Option<Pos2>,
    ) {
        if frame.skin.is_some() {
            return;
        }
        for (a, b) in &frame.bones {
            if let (Some(a), Some(b)) = (project(*a), project(*b)) {
                painter.line_segment([a, b], Stroke::new(2.5, BONE));
            }
        }
        for j in &frame.joints {
            if let Some(s) = project(*j) {
                painter.circle(s, 2.5, JOINT, Stroke::new(0.5, Color32::BLACK));
            }
        }
    }
}
