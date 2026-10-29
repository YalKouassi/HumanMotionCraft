//! The Rotate tool's gizmo around the selected joint: three axis rings (world or the joint's own
//! axes), a view ring (turn about the line of sight) and a trackball inside them (turn freely).
//! A drag runs `pose.rotate` inside one `edit.begin`/`edit.end`, so a whole turn is one undo
//! step; Ctrl snaps ring turns to 15 degrees. Drawn with the egui painter over the 3D picture.

use ac_render::Camera;
use ac_render::glam::Vec3;
use egui::{Color32, Pos2, Rect, Stroke, Vec2};
use serde_json::json;

use super::App;

/// The axis rings' radius on screen (points).
pub const RING_PX: f32 = 64.0;
/// The view ring's radius, as a share of the axis rings'.
const VIEW_RING: f32 = 1.22;
/// Ring turns snap to this many degrees with Ctrl.
pub const SNAP_DEG: f64 = 15.0;
/// How near (points) the pointer must be to a ring to take it.
const GRAB_PX: f32 = 8.0;
/// Segments per ring.
const SEGMENTS: usize = 72;

const AXIS_X: Color32 = Color32::from_rgb(232, 84, 84);
const AXIS_Y: Color32 = Color32::from_rgb(120, 205, 90);
const AXIS_Z: Color32 = Color32::from_rgb(80, 145, 245);
const VIEW_RING_COLOUR: Color32 = Color32::from_gray(215);
const BALL_FILL: Color32 = Color32::from_rgba_premultiplied(18, 18, 18, 18);
const HOT: Color32 = Color32::from_rgb(255, 214, 70);
const READOUT: Color32 = Color32::from_rgb(250, 250, 250);

/// The part of the gizmo a drag took.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    /// An axis ring: 0 X, 1 Y, 2 Z (world, or the joint's own).
    Axis(usize),
    /// The outer ring: about the line of sight.
    View,
    /// Inside the rings: a free turn.
    Ball,
}

/// How a drag becomes an angle.
#[derive(Clone, Copy, Debug)]
enum How {
    /// The pointer's ray meets the ring's plane: the angle swept about the axis.
    Plane { from: Vec3 },
    /// The ring is seen edge-on: distance along its tangent on screen.
    Tangent { dir: Vec2, start: Pos2 },
    /// The angle swept about the joint on screen (the view ring).
    Screen { centre: Pos2 },
    /// Every pointer move turns about the axis across it (the trackball).
    Ball { last: Pos2 },
}

/// A turn in progress.
#[derive(Clone, Copy, Debug)]
pub struct Turning {
    bone: usize,
    pub handle: Handle,
    axis: Vec3,
    how: How,
    /// The raw angle last frame (for unwrapping past half a turn).
    prev: f64,
    /// The turn the pointer asks for, before snapping (radians).
    total: f64,
    /// What has been applied (radians).
    pub applied: f64,
}

/// The gizmo around a joint, in the world and on screen.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub bone: usize,
    pub centre: Vec3,
    pub screen: Pos2,
    /// The rings' radius in metres (so they are `RING_PX` on screen).
    pub radius: f32,
    pub axes: [Vec3; 3],
    /// Unit vector from the joint toward the eye.
    pub to_cam: Vec3,
}

/// A world point to view points (`None` behind the camera).
pub fn to_screen(camera: &Camera, rect: Rect, p: Vec3) -> Option<Pos2> {
    camera
        .project(p, rect.width(), rect.height())
        .map(|q| rect.min + Vec2::new(q[0], q[1]))
}

/// Angle (radians) turning `from` onto `to` about `axis` (right-handed), both seen in the plane
/// across `axis`.
pub fn signed_angle(from: Vec3, to: Vec3, axis: Vec3) -> f64 {
    let (a, b, n) = (from.as_dvec3(), to.as_dvec3(), axis.as_dvec3());
    n.dot(a.cross(b)).atan2(a.dot(b))
}

/// An angle brought into (-pi, pi].
pub fn wrap(a: f64) -> f64 {
    let t = std::f64::consts::TAU;
    let w = a.rem_euclid(t);
    if w > std::f64::consts::PI { w - t } else { w }
}

/// The turn applied for a turn asked: to the nearest `SNAP_DEG` with snapping on.
pub fn snapped(total: f64, snap: bool) -> f64 {
    if snap {
        let step = SNAP_DEG.to_radians();
        (total / step).round() * step
    } else {
        total
    }
}

/// A unit vector across `axis`.
fn across(axis: Vec3) -> Vec3 {
    let other = if axis.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
    axis.cross(other).normalize_or_zero()
}

/// Points round a ring (closed: the first point again at the end).
pub fn ring(centre: Vec3, axis: Vec3, radius: f32) -> Vec<Vec3> {
    let u = across(axis);
    let v = axis.cross(u);
    (0..=SEGMENTS)
        .map(|i| {
            let t = i as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
            centre + (u * t.cos() + v * t.sin()) * radius
        })
        .collect()
}

/// Distance from `p` to the segment `a`-`b` (points).
fn to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// Screen angle of `p` about `c`, counter-clockwise (the screen's y runs down).
fn screen_angle(c: Pos2, p: Pos2) -> f64 {
    f64::from(-(p.y - c.y)).atan2(f64::from(p.x - c.x))
}

impl App {
    /// The gizmo around `bone` as it is shown (`joints`: the joints shown, world).
    pub(super) fn gizmo_frame(&self, rect: Rect, joints: &[Vec3], bone: usize) -> Option<Frame> {
        let centre = *joints.get(bone)?;
        let screen = to_screen(&self.camera, rect, centre)?;
        let (right, _) = self.camera.right_up();
        let probe = to_screen(&self.camera, rect, centre + right * 0.1)?;
        let px_per_m = probe.distance(screen) / 0.1;
        if !px_per_m.is_finite() || px_per_m < 1e-3 {
            return None;
        }
        let axes = if self.view.local {
            let doc = &self.session.document;
            let (rots, _) = doc.skeleton.fk(&doc.pose.rotations, doc.pose.root).ok()?;
            let m = rots.get(bone)?;
            let col =
                |j: usize| Vec3::new(m[0][j] as f32, m[1][j] as f32, m[2][j] as f32).normalize_or_zero();
            [col(0), col(1), col(2)]
        } else {
            [Vec3::X, Vec3::Y, Vec3::Z]
        };
        let to_cam = if self.camera.ortho {
            -self.camera.forward()
        } else {
            (self.camera.eye() - centre).normalize_or_zero()
        };
        Some(Frame {
            bone,
            centre,
            screen,
            radius: RING_PX / px_per_m,
            axes,
            to_cam,
        })
    }

    /// The ring (or the inside) under `at`; rings first, the trackball last.
    pub(super) fn gizmo_hit(&self, rect: Rect, g: &Frame, at: Pos2) -> Option<Handle> {
        let mut best: Option<(f32, usize)> = None;
        for (i, axis) in g.axes.iter().enumerate() {
            let pts = ring(g.centre, *axis, g.radius);
            for w in pts.windows(2) {
                let (Some(a), Some(b)) = (
                    to_screen(&self.camera, rect, w[0]),
                    to_screen(&self.camera, rect, w[1]),
                ) else {
                    continue;
                };
                let front = ((w[0] + w[1]) * 0.5 - g.centre).dot(g.to_cam) >= 0.0;
                // the near half wins over the far half under the pointer
                let d = to_segment(at, a, b) + if front { 0.0 } else { 4.0 };
                if d < GRAB_PX && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, i));
                }
            }
        }
        if let Some((_, i)) = best {
            return Some(Handle::Axis(i));
        }
        let r = at.distance(g.screen);
        if (r - RING_PX * VIEW_RING).abs() < GRAB_PX {
            Some(Handle::View)
        } else if r < RING_PX {
            Some(Handle::Ball)
        } else {
            None
        }
    }

    /// Start a turn: the drag began at `at` on `handle`.
    pub(super) fn begin_turn(&mut self, rect: Rect, g: &Frame, handle: Handle, at: Pos2) {
        let (axis, how, prev) = match handle {
            Handle::Axis(i) => {
                let axis = g.axes.get(i).copied().unwrap_or(Vec3::Z);
                let local = at - rect.min;
                let (o, d) = self.camera.ray(local.x, local.y, rect.width(), rect.height());
                let denom = d.dot(axis);
                let hit = (denom.abs() > 0.2).then(|| o + d * ((g.centre - o).dot(axis) / denom));
                match hit.map(|p| p - g.centre).filter(|v| v.length() > 1e-6) {
                    Some(from) => (axis, How::Plane { from }, 0.0),
                    None => (axis, self.tangent(rect, g, axis, at), 0.0),
                }
            }
            Handle::View => (
                g.to_cam,
                How::Screen { centre: g.screen },
                screen_angle(g.screen, at),
            ),
            Handle::Ball => (g.to_cam, How::Ball { last: at }, 0.0),
        };
        if self
            .run("edit.begin", json!({ "label": "Rotate joint" }))
            .is_some()
        {
            self.view.turning = Some(Turning {
                bone: g.bone,
                handle,
                axis,
                how,
                prev,
                total: 0.0,
                applied: 0.0,
            });
        }
    }

    /// An edge-on ring: the screen direction a point of the ring near `at` moves as it turns.
    fn tangent(&self, rect: Rect, g: &Frame, axis: Vec3, at: Pos2) -> How {
        let near = ring(g.centre, axis, g.radius)
            .into_iter()
            .filter_map(|p| to_screen(&self.camera, rect, p).map(|s| (p, s)))
            .min_by(|a, b| a.1.distance(at).total_cmp(&b.1.distance(at)));
        let dir = near
            .and_then(|(p, s)| {
                let w = (p - g.centre).normalize_or_zero();
                let ahead = p + axis.cross(w) * g.radius * 0.1;
                to_screen(&self.camera, rect, ahead).map(|q| (q - s).normalized())
            })
            .filter(|d| d.length() > 0.5)
            .unwrap_or(Vec2::X);
        How::Tangent { dir, start: at }
    }

    /// Follow the pointer: work out the turn asked and apply what is new of it.
    pub(super) fn update_turn(&mut self, rect: Rect, at: Pos2, snap: bool) {
        let Some(mut t) = self.view.turning else { return };
        let name = self
            .session
            .document
            .skeleton
            .names
            .get(t.bone)
            .cloned()
            .unwrap_or_default();
        match t.how {
            How::Plane { from } => {
                let local = at - rect.min;
                let (o, d) = self.camera.ray(local.x, local.y, rect.width(), rect.height());
                let centre = self.joints().get(t.bone).copied().unwrap_or_default();
                let denom = d.dot(t.axis);
                if denom.abs() > 1e-4 {
                    let v = o + d * ((centre - o).dot(t.axis) / denom) - centre;
                    let raw = signed_angle(from, v, t.axis);
                    t.total += wrap(raw - t.prev);
                    t.prev = raw;
                }
            }
            How::Tangent { dir, start } => {
                t.total = f64::from((at - start).dot(dir) / RING_PX);
            }
            How::Screen { centre } => {
                let raw = screen_angle(centre, at);
                t.total += wrap(raw - t.prev);
                t.prev = raw;
            }
            How::Ball { last } => {
                let d = at - last;
                if d.length() > 0.0 {
                    let (right, up) = self.camera.right_up();
                    let to_cam = -self.camera.forward();
                    let m = right * d.x - up * d.y;
                    let axis = to_cam.cross(m).normalize_or_zero();
                    let angle = f64::from(d.length() / RING_PX);
                    if axis.length() > 0.5
                        && self
                            .run(
                                "pose.rotate",
                                json!({ "bone": name, "axis": [axis.x, axis.y, axis.z], "angle": angle }),
                            )
                            .is_some()
                    {
                        t.applied += angle;
                    }
                }
                t.how = How::Ball { last: at };
                self.view.turning = Some(t);
                return;
            }
        }
        let want = snapped(t.total, snap);
        let delta = want - t.applied;
        if delta.abs() > 1e-7
            && self
                .run(
                    "pose.rotate",
                    json!({ "bone": name, "axis": [t.axis.x, t.axis.y, t.axis.z], "angle": delta }),
                )
                .is_some()
        {
            t.applied = want;
        }
        self.view.turning = Some(t);
    }

    /// The pointer is up: one undo step for the whole turn.
    pub(super) fn end_turn(&mut self) {
        if let Some(t) = self.view.turning.take() {
            self.run("edit.end", json!({}));
            let name = self
                .session
                .document
                .skeleton
                .names
                .get(t.bone)
                .cloned()
                .unwrap_or_default();
            self.status = format!("turned {name} {:.1}°", t.applied.to_degrees());
        }
    }

    /// Draw the gizmo; `hot` is the part under the pointer or being dragged.
    pub(super) fn draw_gizmo(&self, painter: &egui::Painter, rect: Rect, g: &Frame, hot: Option<Handle>) {
        let ball_hot = hot == Some(Handle::Ball);
        painter.circle(
            g.screen,
            RING_PX,
            BALL_FILL,
            Stroke::new(
                1.0,
                if ball_hot {
                    HOT
                } else {
                    Color32::from_white_alpha(40)
                },
            ),
        );
        painter.circle_stroke(
            g.screen,
            RING_PX * VIEW_RING,
            Stroke::new(
                if hot == Some(Handle::View) { 3.0 } else { 1.5 },
                if hot == Some(Handle::View) {
                    HOT
                } else {
                    VIEW_RING_COLOUR
                },
            ),
        );
        // the far halves first, then the near halves over them
        for front_pass in [false, true] {
            for (i, (axis, colour)) in g.axes.iter().zip([AXIS_X, AXIS_Y, AXIS_Z]).enumerate() {
                let is_hot = hot == Some(Handle::Axis(i));
                let pts = ring(g.centre, *axis, g.radius);
                for w in pts.windows(2) {
                    let front = ((w[0] + w[1]) * 0.5 - g.centre).dot(g.to_cam) >= 0.0;
                    if front != front_pass {
                        continue;
                    }
                    let (Some(a), Some(b)) = (
                        to_screen(&self.camera, rect, w[0]),
                        to_screen(&self.camera, rect, w[1]),
                    ) else {
                        continue;
                    };
                    let c = if is_hot { HOT } else { colour };
                    let stroke = if front {
                        Stroke::new(if is_hot { 3.5 } else { 2.2 }, c)
                    } else {
                        Stroke::new(1.2, c.gamma_multiply(0.3))
                    };
                    painter.line_segment([a, b], stroke);
                }
            }
        }
        if let Some(t) = &self.view.turning {
            let text = match t.handle {
                Handle::Ball => format!("free {:.0}°", t.applied.to_degrees()),
                _ => format!("{:+.1}°", t.applied.to_degrees()),
            };
            painter.text(
                g.screen + Vec2::new(0.0, -RING_PX * VIEW_RING - 6.0),
                egui::Align2::CENTER_BOTTOM,
                text,
                egui::FontId::proportional(13.0),
                READOUT,
            );
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// A quarter turn about Z carries X onto Y: +90 degrees; back the other way: -90.
    #[test]
    fn angles_are_right_handed() {
        let a = signed_angle(Vec3::X, Vec3::Y, Vec3::Z);
        assert!((a - std::f64::consts::FRAC_PI_2).abs() < 1e-6, "{a}");
        let b = signed_angle(Vec3::Y, Vec3::X, Vec3::Z);
        assert!((b + std::f64::consts::FRAC_PI_2).abs() < 1e-6, "{b}");
    }

    /// 350 degrees wraps to -10; snapping 22 degrees gives 15, 23 gives 30, and off leaves it.
    #[test]
    fn wrap_and_snap() {
        assert!((wrap(350f64.to_radians()) - (-10f64).to_radians()).abs() < 1e-9);
        assert!((wrap(-190f64.to_radians()) - 170f64.to_radians()).abs() < 1e-9);
        let deg = |d: f64| snapped(d.to_radians(), true).to_degrees();
        assert!((deg(22.0) - 15.0).abs() < 1e-9);
        assert!((deg(23.0) - 30.0).abs() < 1e-9);
        assert!((deg(-8.0) + 15.0).abs() < 1e-9);
        assert_eq!(snapped(0.3, false), 0.3);
    }

    /// Every point of a ring is `radius` from the centre and square to the axis.
    #[test]
    fn rings_lie_across_their_axis() {
        let axis = Vec3::new(0.3, -0.4, 0.866).normalize();
        let c = Vec3::new(1.0, 2.0, 3.0);
        let pts = ring(c, axis, 0.5);
        assert_eq!(pts.len(), SEGMENTS + 1);
        for p in pts {
            assert!(((p - c).length() - 0.5).abs() < 1e-5);
            assert!((p - c).dot(axis).abs() < 1e-5);
        }
    }

    /// The screen's y runs down: a point above the centre is at +90 degrees.
    #[test]
    fn screen_angles_run_counter_clockwise() {
        let a = screen_angle(Pos2::new(10.0, 10.0), Pos2::new(10.0, 0.0));
        assert!((a - std::f64::consts::FRAC_PI_2).abs() < 1e-6);
    }
}
