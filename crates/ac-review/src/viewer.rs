//! The body in the middle of the window: the app's own renderer and mannequin, so a take looks as
//! it will in the app. The camera follows the hips; right-drag turns it, the wheel zooms.

use std::path::Path;

use ac_core::quat::{Quat, Vec3 as V3};
use ac_core::rig::Skeleton;
use ac_render::glam::{Mat4, Vec3};
use ac_render::{Body, Camera, Mesh, Renderer, wgpu};
use egui::{Color32, Pos2, Rect, Sense};

const BODY: [f32; 4] = [0.72, 0.74, 0.78, 1.0];

pub struct Viewer {
    pub skeleton: Skeleton,
    renderer: Renderer,
    skin_to_bone: Vec<usize>,
    texture: Option<egui::TextureId>,
    pub camera: Camera,
    /// The camera keeps the hips in the middle.
    pub follow: bool,
}

impl Viewer {
    pub fn new(cc: &eframe::CreationContext, assets: &Path) -> Result<Viewer, String> {
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
            .ok_or("the wgpu renderer is not available")?;
        Ok(Viewer {
            renderer: Renderer::new(&rs.device, rs.target_format, &mesh),
            skeleton,
            skin_to_bone,
            texture: None,
            camera: Camera {
                yaw: 0.6,
                pitch: 0.12,
                distance: 4.2,
                ..Camera::default()
            },
            follow: true,
        })
    }

    /// Draw one pose filling the space left; a message instead when there is none.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        frame: &eframe::Frame,
        pose: Option<(&[Quat], V3)>,
        message: &str,
    ) {
        let size = ui.available_size();
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
        if resp.dragged_by(egui::PointerButton::Secondary) || resp.dragged_by(egui::PointerButton::Primary) {
            let d = resp.drag_delta();
            self.camera.yaw -= d.x * 0.008;
            self.camera.pitch = (self.camera.pitch + d.y * 0.008).clamp(-1.4, 1.4);
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            self.camera.distance = (self.camera.distance * (-scroll * 0.002).exp()).clamp(0.8, 40.0);
        }
        let painter = ui.painter_at(rect);
        let Some((rots, root)) = pose else {
            painter.rect_filled(rect, 0.0, Color32::from_rgb(28, 29, 32));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                message,
                egui::FontId::proportional(16.0),
                Color32::GRAY,
            );
            return;
        };
        let (wr, pos) = match self.skeleton.fk(rots, root) {
            Ok(x) => x,
            Err(e) => {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("pose: {e}"),
                    egui::FontId::proportional(16.0),
                    Color32::LIGHT_RED,
                );
                return;
            }
        };
        if self.follow
            && let Some(h) = pos.first()
        {
            self.camera.target = Vec3::new(h[0] as f32, h[1] as f32, 0.9);
        }
        let all = ac_render::skin::skin_matrices(&wr, &pos, &self.skeleton.heads);
        let joints: Vec<Mat4> = self
            .skin_to_bone
            .iter()
            .map(|&b| all.get(b).copied().unwrap_or(Mat4::IDENTITY))
            .collect();
        let Some(rs) = frame.wgpu_render_state() else {
            return;
        };
        let ppp = ui.pixels_per_point();
        let (w, h) = (
            (rect.width() * ppp).max(1.0) as u32,
            (rect.height() * ppp).max(1.0) as u32,
        );
        let bodies = [Body {
            joints: &joints,
            tint: BODY,
        }];
        match self
            .renderer
            .render(&rs.device, &rs.queue, w, h, &self.camera, &bodies)
        {
            Ok(view) => {
                let mut r = rs.renderer.write();
                match self.texture {
                    Some(id) => r.update_egui_texture_from_wgpu_texture(
                        &rs.device,
                        view,
                        wgpu::FilterMode::Linear,
                        id,
                    ),
                    None => {
                        self.texture =
                            Some(r.register_native_texture(&rs.device, view, wgpu::FilterMode::Linear))
                    }
                }
            }
            Err(e) => {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("render: {e}"),
                    egui::FontId::proportional(16.0),
                    Color32::LIGHT_RED,
                );
                return;
            }
        }
        if let Some(id) = self.texture {
            painter.image(
                id,
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
    }
}
