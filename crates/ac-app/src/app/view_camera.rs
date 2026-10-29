//! Camera presets: the named views (front, side, top and their opposites, three-quarter), frame
//! the selection or the body, orthographic or perspective, and a camera that follows the body.

use ac_render::glam::Vec3;

use super::App;

/// Room around framed joints (metres): a joint or hand, and the whole body (head and feet).
const FRAME_MARGIN: f32 = 0.12;
const BODY_MARGIN: f32 = 0.3;
/// The closest and farthest the camera goes (as the wheel allows).
const MIN_DISTANCE: f32 = 0.5;
const MAX_DISTANCE: f32 = 40.0;

/// The camera distance that fits a sphere of `radius` in a picture `fov_y` tall (and `aspect`
/// wide), perspective or orthographic alike.
pub fn fit_distance(radius: f32, fov_y: f32, aspect: f32) -> f32 {
    let half = (fov_y / 2.0).tan() * aspect.clamp(0.1, 1.0);
    (radius / half.max(1e-3)).clamp(MIN_DISTANCE, MAX_DISTANCE)
}

/// The centre and radius of a set of points (their box's centre; the farthest point's distance).
pub fn bounds(points: &[Vec3]) -> Option<(Vec3, f32)> {
    let finite: Vec<Vec3> = points.iter().copied().filter(|p| p.is_finite()).collect();
    let first = *finite.first()?;
    let (lo, hi) = finite
        .iter()
        .fold((first, first), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
    let centre = (lo + hi) * 0.5;
    let radius = finite.iter().map(|p| (*p - centre).length()).fold(0.0, f32::max);
    Some((centre, radius))
}

impl App {
    /// Look from a named view (see `ac_render::offscreen::view`) at the body, keeping the
    /// distance and the projection.
    pub(super) fn view_preset(&mut self, name: &str) -> Result<(), String> {
        let root = self.joints().first().copied().unwrap_or(Vec3::new(0.0, 0.0, 0.9));
        let c = ac_render::offscreen::view(name, Vec3::new(root.x, root.y, 0.9))?;
        (self.camera.yaw, self.camera.pitch, self.camera.target) = (c.yaw, c.pitch, c.target);
        self.status = format!(
            "view: {name}{}",
            if self.camera.ortho { " (orthographic)" } else { "" }
        );
        Ok(())
    }

    /// Fill the view with the selected joints (a joint alone brings the part below it, so a
    /// wrist frames the hand), or the whole body when nothing is selected.
    pub(super) fn frame_selection(&mut self, aspect: f32) {
        let joints = self.joints();
        let sk = self.session.document.skeleton.clone();
        let sel = self.selection();
        let (points, margin, what): (Vec<Vec3>, f32, &str) = match sel.as_slice() {
            [] => (joints.clone(), BODY_MARGIN, "the body"),
            [one] => {
                let name = sk.names.get(*one).cloned().unwrap_or_default();
                let below = ac_core::parts::subtree(&sk, &name);
                (
                    below.iter().filter_map(|b| joints.get(*b).copied()).collect(),
                    FRAME_MARGIN,
                    "the selection",
                )
            }
            many => (
                many.iter().filter_map(|b| joints.get(*b).copied()).collect(),
                FRAME_MARGIN,
                "the selection",
            ),
        };
        let Some((centre, radius)) = bounds(&points) else {
            self.status = "frame: no joints to frame".into();
            return;
        };
        self.camera.target = centre;
        self.camera.distance = fit_distance(radius + margin, self.camera.fov_y, aspect);
        self.status = format!("framed {what}");
    }

    pub(super) fn toggle_ortho(&mut self) {
        self.camera.ortho = !self.camera.ortho;
        self.status = if self.camera.ortho {
            "orthographic view".into()
        } else {
            "perspective view".into()
        };
    }

    /// With follow on, keep the body's root in the middle of the view (height unchanged).
    pub(super) fn follow_body(&mut self) {
        if !self.view.follow {
            return;
        }
        if let Some(root) = self.joints().first().copied().filter(|r| r.is_finite()) {
            self.camera.target.x = root.x;
            self.camera.target.y = root.y;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A point and one 1 m off: centre half way, radius 0.5. A sphere of 0.5 m in a 0.7 rad
    /// picture needs 0.5 / tan(0.35) = 1.3699 m; a portrait picture needs more; a tiny one is
    /// held at the nearest distance.
    #[test]
    fn framing_fits_the_points() {
        let (c, r) = bounds(&[Vec3::ZERO, Vec3::X, Vec3::NAN]).unwrap_or_default();
        assert!((c - Vec3::new(0.5, 0.0, 0.0)).length() < 1e-6 && (r - 0.5).abs() < 1e-6);
        assert!((fit_distance(0.5, 0.7, 1.5) - 1.369_9).abs() < 1e-3);
        assert!(fit_distance(0.5, 0.7, 0.5) > 2.0);
        assert_eq!(fit_distance(0.01, 0.7, 1.0), MIN_DISTANCE);
        assert!(bounds(&[]).is_none());
    }
}
