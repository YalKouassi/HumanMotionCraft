//! The 3D viewport: the body, its ghosts (the keyed pose where physics moved it, the AI preview,
//! the onion skin), the physics overlay (support area, centre of mass, planted feet, jump arc),
//! the selected joint's trail and the joint handles: drag to pose (or reach, with full-body IK),
//! right-click to pin. The Move tool slides or turns the whole animation instead; the Rotate tool
//! turns the selected joint with a gizmo (`gizmo.rs`). The toolbar and keys are in
//! `view_toolbar.rs`, the camera presets in `view_camera.rs`, the picker in `picker.rs`.

use ac_core::placement::Placement;
use ac_render::glam::{Mat4, Vec3};
use ac_render::{Body, wgpu};
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};
use serde_json::json;

use super::App;
use super::gizmo::{Handle, RING_PX};

/// What dragging a joint does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    /// The drag rules (hands and feet by two-bone IK, other joints turn their parent).
    Drag,
    /// Full-body reach: the whole body helps, pins and limits hold.
    Reach,
    /// The body on the frame shown slides over the floor (Shift: turns about the root), its key
    /// with it; Alt moves the whole animation. Poses unchanged.
    Move,
    /// The selected joint turns with the gizmo's rings or trackball (`pose.rotate`).
    Rotate,
}

impl Tool {
    /// The tool's name in `--ui tool=...` and the layout.
    pub fn name(self) -> &'static str {
        match self {
            Tool::Drag => "drag",
            Tool::Reach => "reach",
            Tool::Move => "move",
            Tool::Rotate => "rotate",
        }
    }

    pub fn parse(name: &str) -> Result<Tool, String> {
        [Tool::Drag, Tool::Reach, Tool::Move, Tool::Rotate]
            .into_iter()
            .find(|t| t.name() == name)
            .ok_or_else(|| "drag, reach, move or rotate".into())
    }
}

/// A Move-tool drag in progress: shown moved while the button is down, one `pose.move` (this frame)
/// or `clip.move` (Alt: the whole animation) on release.
#[derive(Clone, Copy, Debug)]
pub struct Moving {
    /// Alt was down at the start: every key moves, not just this frame's.
    whole: bool,
    /// Where the drag began on the floor-level plane through the root.
    from: Vec3,
    /// Shift was down at the start: turn instead of slide.
    turn: bool,
    start_x: f32,
    /// The move so far.
    now: Placement,
}

/// Degrees a Shift-drag turns per point of horizontal pointer travel.
const TURN_PER_POINT: f64 = 0.5;

// the ghosts', support area's and AI preview's colours come from the theme (`self.palette()`)
const PIN: Color32 = Color32::from_rgb(230, 80, 80);
const PLANTED: Color32 = Color32::from_rgb(240, 180, 60);
const BODY: [f32; 4] = [0.72, 0.74, 0.78, 1.0];
const HANDLE: Color32 = Color32::from_rgb(90, 200, 255);
const HANDLE_ACTIVE: Color32 = Color32::from_rgb(255, 200, 60);
const HANDLE_PICKED: Color32 = Color32::from_rgb(255, 150, 50);
/// The root's handle: a square, so it reads apart from the hips' dots.
const HANDLE_ROOT: Color32 = Color32::from_rgb(235, 120, 255);

fn v3(p: [f64; 3]) -> Vec3 {
    Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32)
}

/// A pin: a round head above the point and a needle down to it.
fn pin(painter: &egui::Painter, at: Pos2, colour: Color32, size: f32) {
    let head = at - Vec2::new(0.0, size * 2.2);
    painter.line_segment([head, at], Stroke::new(1.5, Color32::from_gray(20)));
    painter.circle(head, size, colour, Stroke::new(1.0, Color32::BLACK));
}

impl App {
    /// Skinning matrices for a pose, in the mannequin's joint order.
    fn skin(&self, rotations: &[ac_core::quat::Quat], root: [f64; 3]) -> Result<Vec<Mat4>, String> {
        let sk = &self.session.document.skeleton;
        let (rots, pos) = sk.fk(rotations, root).map_err(|e| e.to_string())?;
        let all = ac_render::skin::skin_matrices(&rots, &pos, &sk.heads);
        Ok(self.skin_to_bone.iter().map(|&b| all[b]).collect())
    }

    /// The AI preview's root path (the root head on every frame of it), worked out once per
    /// preview.
    fn preview_path(&mut self) -> Vec<Vec3> {
        let doc = &self.session.document;
        let Some(p) = doc.live_preview() else {
            return Vec::new();
        };
        let key = std::sync::Arc::as_ptr(p) as usize;
        if let Some((k, path)) = &self.preview_path
            && *k == key
        {
            return path.clone();
        }
        let frames: Vec<f64> = (p.from..=p.to).map(|f| f as f64).collect();
        let path: Vec<Vec3> = p
            .clip
            .sample_with(&frames, &p.links)
            .ok()
            .map(|(rots, roots)| {
                rots.iter()
                    .zip(&roots)
                    .filter_map(|(r, o)| doc.skeleton.fk(r, *o).ok().map(|(_, pos)| v3(pos[0])))
                    .collect()
            })
            .unwrap_or_default();
        self.preview_path = Some((key, path.clone()));
        path
    }

    pub(super) fn viewport(&mut self, ui: &mut egui::Ui) -> Rect {
        let size = ui.available_size();
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let ppp = ui.pixels_per_point();
        let (w, h) = (
            (rect.width() * ppp).max(1.0) as u32,
            (rect.height() * ppp).max(1.0) as u32,
        );

        // camera: right drag orbits, middle drag pans, wheel zooms
        if resp.dragged_by(egui::PointerButton::Secondary) {
            let d = resp.drag_delta();
            self.camera.yaw -= d.x * 0.008;
            self.camera.pitch = (self.camera.pitch + d.y * 0.008).clamp(-1.4, 1.4);
        }
        if resp.dragged_by(egui::PointerButton::Middle) {
            let d = resp.drag_delta() * self.camera.distance * 0.0015;
            let (sy, cy) = self.camera.yaw.sin_cos();
            let right = Vec3::new(cy, sy, 0.0);
            self.camera.target += -right * d.x + Vec3::Z * d.y;
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            self.camera.distance = (self.camera.distance * (-scroll * 0.002).exp()).clamp(0.5, 40.0);
        }
        // its keys (F, 1 3 7, 0, 5) are context keys in the keymap (`shell/scoped.rs`)
        self.view.hovered = resp.hovered();
        self.follow_body();
        let project = |camera: &ac_render::Camera, p: Vec3| {
            camera
                .project(p, rect.width(), rect.height())
                .map(|q| rect.min + Vec2::new(q[0], q[1]))
        };

        // the Move tool: a drag moves the body on this frame (Alt: the whole animation)
        if self.tool == Tool::Move {
            self.move_drag(ui, &resp, rect, self.joints().first().copied());
        }

        // joints in view pixels (points), moved with a Move drag
        let joints: Vec<Vec3> = match self.moving {
            Some(m) => self
                .joints()
                .iter()
                .map(|p| v3(m.now.point([f64::from(p.x), f64::from(p.y), f64::from(p.z)])))
                .collect(),
            None => self.joints(),
        };
        let screen: Vec<Option<Pos2>> = joints.iter().map(|p| project(&self.camera, *p)).collect();
        // only joints with a handle (and the selected one) can be picked
        let handles = self.handles();
        let active = self.selected;
        let hit = |at: Option<Pos2>| {
            at.and_then(|at| {
                screen
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| handles.get(*i).copied().unwrap_or(false) || active == Some(*i))
                    .filter_map(|(i, s)| s.map(|s| (i, s.distance(at))))
                    .filter(|(_, d)| *d < 12.0)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(i, _)| i)
            })
        };

        // the joint under the pointer gets its name beside it
        let hovered_joint = hit(ui.input(|i| i.pointer.hover_pos()).filter(|p| rect.contains(*p)));

        // right-click a joint: pin it where it is, or unpin it
        if resp.secondary_clicked()
            && let Some(b) = hit(resp.interact_pointer_pos())
        {
            let name = self.session.document.skeleton.names[b].clone();
            let pinned = self.session.document.pins.contains_key(&name);
            if self
                .run("pin.set", json!({ "bone": name, "on": !pinned }))
                .is_some()
            {
                self.status = format!("{} {name}", if pinned { "unpinned" } else { "pinned" });
            }
        }

        // where the button went down, not where a drag had got to when it began
        let press = ui
            .input(|i| i.pointer.press_origin())
            .or(resp.interact_pointer_pos());
        let add = ui.input(|i| i.modifiers.shift);
        let gizmo = match self.tool {
            Tool::Rotate => self.selected.and_then(|b| self.gizmo_frame(rect, &joints, b)),
            _ => None,
        };
        if self.tool == Tool::Rotate {
            let on_gizmo = gizmo.zip(press).and_then(|(g, at)| self.gizmo_hit(rect, &g, at));
            let picked = hit(press);
            // a joint inside the trackball is picked rather than turned
            let other = picked.filter(|b| Some(*b) != self.selected);
            if resp.drag_started_by(egui::PointerButton::Primary) {
                match (gizmo, on_gizmo, press) {
                    (Some(g), Some(h), Some(at)) if !(h == Handle::Ball && other.is_some()) => {
                        self.begin_turn(rect, &g, h, at)
                    }
                    _ => {
                        if picked.is_some() {
                            self.select_joint(picked, add);
                        }
                    }
                }
            } else if resp.clicked() && (other.is_some() || on_gizmo.is_none()) {
                // a click picks a joint even on a ring (only a drag turns); a click on the gizmo
                // away from any joint keeps the selection
                self.select_joint(picked, add);
            }
            if self.view.turning.is_some()
                && resp.dragged_by(egui::PointerButton::Primary)
                && let Some(at) = resp.interact_pointer_pos()
            {
                let snap = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
                self.update_turn(rect, at, snap);
            }
            if resp.drag_stopped_by(egui::PointerButton::Primary) {
                self.end_turn();
            }
        }

        // pick and drag
        if !matches!(self.tool, Tool::Move | Tool::Rotate)
            && (resp.drag_started_by(egui::PointerButton::Primary) || resp.clicked())
        {
            let picked = hit(press);
            if resp.clicked() {
                self.select_joint(picked, add);
            } else if let Some(b) = picked {
                // dragging one of the joints selected together keeps the others selected
                if self.view.picked.contains(&b) {
                    self.selected = Some(b);
                } else {
                    self.select_joint(Some(b), false);
                }
            }
            if resp.drag_started_by(egui::PointerButton::Primary)
                && let Some(b) = picked
            {
                let depth = (joints[b] - self.camera.eye()).length();
                let label = if self.tool == Tool::Reach {
                    "Reach"
                } else {
                    "Drag joint"
                };
                self.run("edit.begin", json!({ "label": label }));
                self.dragging = Some((b, depth));
            }
        }
        if let (Some((b, depth)), Some(at)) = (self.dragging, resp.interact_pointer_pos())
            && resp.dragged_by(egui::PointerButton::Primary)
        {
            let local = at - rect.min;
            let (o, d) = self.camera.ray(local.x, local.y, rect.width(), rect.height());
            // keep the joint at its distance along the view direction
            let fwd = (self.camera.target - self.camera.eye()).normalize_or_zero();
            let t =
                depth * fwd.dot((joints[b] - self.camera.eye()).normalize_or_zero()) / d.dot(fwd).max(1e-3);
            let p = o + d * t;
            let name = self.session.document.skeleton.names[b].clone();
            let command = if self.tool == Tool::Reach {
                "pose.reach"
            } else {
                "pose.drag"
            };
            self.run(command, json!({ "bone": name, "to": [p.x, p.y, p.z] }));
        }
        if resp.drag_stopped_by(egui::PointerButton::Primary) && self.dragging.take().is_some() {
            self.run("edit.end", json!({}));
        }

        // the bodies: the pose shown, then its ghosts
        let f = self.session.document.frame;
        let mut bodies: Vec<(Vec<Mat4>, [f32; 4])> = Vec::new();
        // a Move drag shows the body moved before it is committed (and the ghosts too when the
        // whole animation moves)
        let (moving, rig) = (self.moving, self.session.document.skeleton.clone());
        let shown = move |pose: ac_core::Pose| match moving {
            Some(m) => m.now.pose(&rig, &pose).unwrap_or(pose),
            None => pose,
        };
        let pose = shown(self.session.document.pose.clone());
        match self.skin(&pose.rotations, pose.root) {
            Ok(m) => bodies.push((m, BODY)),
            Err(e) => {
                self.status = format!("pose: {e}");
                return rect;
            }
        }
        let onion = self.onion_ghosts();
        let onion_frames: Vec<i64> = onion.iter().map(|g| g.0).collect();
        for (_, ghost, tint) in onion {
            let ghost = if moving.is_some_and(|m| m.whole) {
                shown(ghost)
            } else {
                ghost
            };
            if let Ok(m) = self.skin(&ghost.rotations, ghost.root) {
                bodies.push((m, tint));
            }
        }
        let overlay = if self.show_overlay || self.show_ghost {
            self.session.document.overlay(f).ok()
        } else {
            None
        };
        if self.show_ghost
            && overlay.as_ref().is_some_and(|o| o.keyed_moved > 0.002)
            && let Ok(keyed) = self.session.document.keyed_pose(f).map(&shown)
            && let Ok(m) = self.skin(&keyed.rotations, keyed.root)
        {
            bodies.push((m, self.palette().ghost_keyed));
        }
        if let Some(p) = self.session.document.preview_pose(f).map(&shown)
            && let Ok(m) = self.skin(&p.rotations, p.root)
        {
            bodies.push((m, self.palette().ghost_preview));
        }
        let mut drawn: Vec<Body> = bodies
            .iter()
            .map(|(m, tint)| Body {
                joints: m,
                tint: *tint,
                mesh: 0,
            })
            .collect();
        // the person's own character, driven live through its mapping (`character.rs`)
        let character = self.character_frame(&pose);
        if let Some(c) = &character
            && let Some(skinned) = &c.skin
        {
            drawn.push(Body {
                joints: skinned,
                tint: super::character::TINT,
                mesh: super::character::MESH_SLOT,
            });
        }
        let rs = &self.render_state;
        match self
            .renderer
            .render(&rs.device, &rs.queue, w, h, &self.camera, &drawn)
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
            Err(e) => self.status = format!("render: {e}"),
        }
        if let Some(id) = self.texture {
            ui.painter().image(
                id,
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        let painter = ui.painter_at(rect);
        if let Some(c) = &character {
            self.draw_character(&painter, c, &|p| project(&self.camera, p));
        }

        // the physics overlay
        if self.show_overlay
            && let Some(o) = &overlay
        {
            if let Some(b) = &o.balance {
                let ground = |q: [f64; 2]| {
                    v3([
                        q[0],
                        q[1],
                        ac_core::physics::ground(&self.session.document.clip.surfaces, q[0], q[1]),
                    ])
                };
                let poly: Vec<Pos2> = b
                    .support
                    .iter()
                    .filter_map(|q| project(&self.camera, ground(*q)))
                    .collect();
                let pal = self.palette();
                let line = if b.inside { pal.support_ok } else { pal.support_off };
                let fill = super::theme::faded(line, 50);
                if poly.len() >= 3 {
                    painter.add(egui::Shape::convex_polygon(
                        poly.clone(),
                        fill,
                        Stroke::new(1.5, line),
                    ));
                }
                // the centre of mass, and where it falls on the floor
                if let (Some(c), Some(g)) = (
                    project(&self.camera, v3(o.com)),
                    project(&self.camera, ground(b.com)),
                ) {
                    painter.line_segment([c, g], Stroke::new(1.0, line));
                    painter.circle(g, 3.0, line, Stroke::NONE);
                }
            }
            if let Some(c) = project(&self.camera, v3(o.com)) {
                painter.circle(
                    c,
                    5.0,
                    Color32::from_rgb(250, 250, 250),
                    Stroke::new(1.5, Color32::BLACK),
                );
                painter.circle(c, 2.0, Color32::BLACK, Stroke::NONE);
            }
            for c in &o.contacts {
                if let Some(s) = project(&self.camera, v3(c.point)) {
                    if c.pinned {
                        painter.circle(s, 2.5, PLANTED, Stroke::new(0.5, Color32::BLACK));
                    } else {
                        painter.circle_stroke(s, 2.5, Stroke::new(1.0, PLANTED));
                    }
                }
            }
            // planted feet: one pin per foot that holds still
            for foot in 0..2 {
                let held: Vec<[f64; 3]> = o
                    .contacts
                    .iter()
                    .filter(|c| c.pinned && c.foot == foot)
                    .map(|c| c.point)
                    .collect();
                if held.len() >= 3 {
                    let n = held.len() as f64;
                    let c = held.iter().fold([0.0; 3], |a, p| {
                        [a[0] + p[0] / n, a[1] + p[1] / n, a[2] + p[2] / n]
                    });
                    if let Some(s) = project(&self.camera, v3(c)) {
                        pin(&painter, s, PLANTED, 4.0);
                    }
                }
            }
            if let Some((a, b, arc)) = &o.flight {
                let pts: Vec<Pos2> = arc.iter().filter_map(|p| project(&self.camera, v3(*p))).collect();
                painter.add(egui::Shape::line(
                    pts.clone(),
                    Stroke::new(1.5, Color32::from_rgb(120, 190, 255)),
                ));
                for (i, p) in pts.iter().enumerate() {
                    let here = a + i == o.frame;
                    painter.circle_filled(*p, if here { 4.0 } else { 2.0 }, Color32::from_rgb(120, 190, 255));
                }
                if let Some(top) = pts.iter().min_by(|p, q| p.y.total_cmp(&q.y)) {
                    painter.text(
                        *top - Vec2::new(0.0, 8.0),
                        egui::Align2::CENTER_BOTTOM,
                        format!("jump {a}–{b}"),
                        egui::FontId::proportional(11.0),
                        Color32::from_rgb(160, 210, 255),
                    );
                }
            }
        }
        // the selected joint's trail, moved with a whole-animation Move drag
        let moved = move |p: Vec3| match moving {
            Some(m) if m.whole => v3(m.now.point([f64::from(p.x), f64::from(p.y), f64::from(p.z)])),
            _ => p,
        };
        self.draw_trail(&painter, rect, &moved);
        // scene boxes and a previewed travel path (the inspector's panels)
        self.scene_overlay(&painter, &|p| project(&self.camera, p));
        // the AI preview's root path
        let path = self.preview_path();
        if path.len() > 1 {
            let pts: Vec<Pos2> = path.iter().filter_map(|p| project(&self.camera, *p)).collect();
            for seg in pts.windows(2).step_by(2) {
                painter.line_segment([seg[0], seg[1]], Stroke::new(2.0, self.palette().preview));
            }
        }

        // the joint handles: dots on the main joints (fingers and face too when asked), a pin on
        // pinned ones; the active joint is the largest, joints selected with it are orange
        let doc = &self.session.document;
        for (i, s) in screen.iter().enumerate() {
            if self.view.hide_joints {
                break;
            }
            let Some(s) = s else { continue };
            let Some(name) = doc.skeleton.names.get(i) else {
                continue;
            };
            let shown = handles.get(i).copied().unwrap_or(false);
            if doc.pins.contains_key(name) {
                painter.circle(*s, 3.0, PIN, Stroke::new(1.0, Color32::BLACK));
                pin(&painter, *s, PIN, 5.0);
            } else if self.selected == Some(i) {
                painter.circle(*s, 6.0, HANDLE_ACTIVE, Stroke::new(1.0, Color32::BLACK));
            } else if self.view.picked.contains(&i) {
                painter.circle(*s, 5.0, HANDLE_PICKED, Stroke::new(1.0, Color32::BLACK));
            } else if shown && doc.skeleton.parents.get(i).is_some_and(Option::is_none) {
                painter.rect(
                    Rect::from_center_size(*s, Vec2::splat(9.0)),
                    1.0,
                    HANDLE_ROOT,
                    Stroke::new(1.0, Color32::BLACK),
                    egui::StrokeKind::Middle,
                );
            } else if shown {
                let r = if self.view.all_handles { 2.8 } else { 3.5 };
                painter.circle(*s, r, HANDLE, Stroke::new(1.0, Color32::BLACK));
            }
        }
        if let Some(i) = hovered_joint
            && let (Some(Some(s)), Some(name)) = (screen.get(i), doc.skeleton.names.get(i))
        {
            let text = if doc.skeleton.parents.get(i).is_some_and(Option::is_none) {
                format!("{name}: the whole body (drag: move it, feet stay; Rotate: turn it)")
            } else {
                name.clone()
            };
            let at = *s + Vec2::new(9.0, -9.0);
            let galley = painter.layout_no_wrap(text, egui::FontId::proportional(12.0), Color32::WHITE);
            painter.rect_filled(
                Rect::from_min_size(at - Vec2::new(3.0, 1.0), galley.size() + Vec2::new(6.0, 2.0)),
                3.0,
                Color32::from_black_alpha(180),
            );
            painter.galley(at, galley, Color32::WHITE);
        }

        // the rotate gizmo, over the handles
        if let Some(g) = &gizmo {
            let hot = self.view.turning.map(|t| t.handle).or_else(|| {
                ui.input(|i| i.pointer.hover_pos())
                    .filter(|p| rect.contains(*p))
                    .and_then(|p| self.gizmo_hit(rect, g, p))
            });
            self.draw_gizmo(&painter, rect, g, hot);
        }

        if !self.view.hide_tools {
            self.viewport_toolbar(ui, rect);
        }
        // last: the toolbar may have changed the tool or the selection
        self.viewport_layout(rect, &screen, &handles, gizmo.as_ref(), &onion_frames);
        rect
    }

    /// Where things are, for the control port's `layout` (window points).
    fn viewport_layout(
        &mut self,
        rect: Rect,
        screen: &[Option<Pos2>],
        handles: &[bool],
        gizmo: Option<&super::gizmo::Frame>,
        onion: &[i64],
    ) {
        let names = &self.session.document.skeleton.names;
        let joints_on_screen: serde_json::Map<String, serde_json::Value> = screen
            .iter()
            .zip(names)
            .zip(handles)
            .filter(|(_, shown)| **shown)
            .filter_map(|((s, n), _)| s.map(|s| (n.clone(), json!([s.x, s.y]))))
            .collect();
        let name = |b: &usize| names.get(*b).cloned();
        let selection = json!({
            "active": self.selected.as_ref().and_then(name),
            "joints": self.selection().iter().filter_map(name).collect::<Vec<_>>(),
        });
        // a grab point on each axis ring: on its near half, well clear of the other rings
        let gizmo = gizmo.map(|g| {
            let axes: Vec<serde_json::Value> = g
                .axes
                .iter()
                .map(|a| {
                    let out = (g.to_cam - *a * a.dot(g.to_cam)).normalize_or_zero();
                    let side = a.cross(g.to_cam).normalize_or_zero();
                    let p = g.centre + (out * 0.5 + side * 0.866).normalize_or_zero() * g.radius;
                    super::gizmo::to_screen(&self.camera, rect, p)
                        .map_or(serde_json::Value::Null, |s| json!([s.x, s.y]))
                })
                .collect();
            json!({ "centre": [g.screen.x, g.screen.y], "ring_px": RING_PX, "view_ring_px": RING_PX * 1.22, "axes": axes })
        });
        let c = &self.camera;
        self.layout["viewport"] = json!([rect.left(), rect.top(), rect.right(), rect.bottom()]);
        self.layout["joints"] = serde_json::Value::Object(joints_on_screen);
        self.layout["selection"] = selection;
        self.layout["tool"] = json!(self.tool.name());
        self.layout["gizmo"] = gizmo.unwrap_or(serde_json::Value::Null);
        self.layout["onion"] = json!(onion);
        self.layout["camera"] = json!({
            "yaw": c.yaw, "pitch": c.pitch, "distance": c.distance, "ortho": c.ortho,
            "target": [c.target.x, c.target.y, c.target.z], "follow": self.view.follow,
        });
    }

    /// A Move-tool drag: a slide across the floor-level plane through the root, or (Shift) a turn
    /// about the root by horizontal travel; the body on this frame (and its key), or with Alt held
    /// at the start the whole animation.
    fn move_drag(&mut self, ui: &egui::Ui, resp: &egui::Response, rect: Rect, root: Option<Vec3>) {
        let floor = |camera: &ac_render::Camera, at: Pos2, z: f32| {
            let local = at - rect.min;
            let (o, d) = camera.ray(local.x, local.y, rect.width(), rect.height());
            let t = (z - o.z) / d.z;
            (d.z.abs() > 1e-4 && t > 0.0).then(|| o + d * t)
        };
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let (Some(root), Some(at)) = (root, ui.input(|i| i.pointer.press_origin()))
            && let Some(from) = floor(&self.camera, at, root.z)
        {
            self.moving = Some(Moving {
                whole: ui.input(|i| i.modifiers.alt),
                from,
                turn: ui.input(|i| i.modifiers.shift),
                start_x: at.x,
                now: Placement {
                    by: [0.0; 3],
                    turn: 0.0,
                    pivot: [f64::from(root.x), f64::from(root.y)],
                },
            });
        }
        if let (Some(m), Some(at)) = (self.moving.as_mut(), resp.interact_pointer_pos())
            && resp.dragged_by(egui::PointerButton::Primary)
        {
            if m.turn {
                m.now.turn = (f64::from(at.x - m.start_x) * TURN_PER_POINT).to_radians();
            } else if let Some(p) = floor(&self.camera, at, m.from.z) {
                m.now.by = [f64::from(p.x - m.from.x), f64::from(p.y - m.from.y), 0.0];
            }
        }
        if resp.drag_stopped_by(egui::PointerButton::Primary)
            && let Some(m) = self.moving.take()
            && (m.now.by != [0.0; 3] || m.now.turn != 0.0)
        {
            let params = json!({
                "by": m.now.by,
                "turn": m.now.turn.to_degrees(),
                "pivot": [m.now.pivot[0], m.now.pivot[1], 0.0],
            });
            let id = if m.whole { "clip.move" } else { "pose.move" };
            if let Some(v) = self.run(id, params) {
                let notes = v["notes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|n| n.as_str())
                    .map(str::to_string);
                let head = if m.whole {
                    format!("moved {} keys", v["keys"])
                } else if v["keyed"] == json!(true) {
                    format!("moved the body and its key on frame {}", v["frame"])
                } else {
                    format!("moved the body on frame {}", v["frame"])
                };
                self.status = std::iter::once(head).chain(notes).collect::<Vec<_>>().join("; ");
            }
        }
    }
}
