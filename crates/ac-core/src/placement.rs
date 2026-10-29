//! Moving the body in the scene, rigidly: one frame's pose (and its key, when the frame has one), or
//! the whole animation (every key of the edited layer slides and turns about the vertical axis
//! together). Each pose stays exactly as it is. (Dragging the root is different: it keeps the feet
//! planted, so the legs bend.)
//!
//! For a turn `t` about the vertical line through `pivot`, then a slide `by` (metres), a world point
//! moves as
//!
//! ```text
//! p' = pivot + Rz(t) (p - pivot) + by
//! ```
//!
//! and the root's world rotation (the root has no parent, so its key rotation is its world rotation)
//! becomes `Rz(t) q`. Every other bone is relative to its parent and stays.

use crate::clip::{LayerMode, LayerSource};
use crate::document::{Document, Pose};
use crate::quat::{self, Quat, Vec3};
use crate::rig::Skeleton;

/// A rigid move in the rig's world.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Metres, after the turn.
    pub by: Vec3,
    /// Radians about +Z (counter-clockwise seen from above).
    pub turn: f64,
    /// The turn's vertical line, `[x, y]` metres.
    pub pivot: [f64; 2],
}

impl Placement {
    fn spin(&self) -> Quat {
        let h = self.turn / 2.0;
        [0.0, 0.0, h.sin(), h.cos()]
    }

    /// A world point (metres) moved.
    pub fn point(&self, p: Vec3) -> Vec3 {
        let (s, c) = self.turn.sin_cos();
        let (dx, dy) = (p[0] - self.pivot[0], p[1] - self.pivot[1]);
        [
            self.pivot[0] + c * dx - s * dy + self.by[0],
            self.pivot[1] + s * dx + c * dy + self.by[1],
            p[2] + self.by[2],
        ]
    }

    /// The root's rotation and offset (hip heights from its rest `head`) moved.
    fn root(&self, q: Quat, offset: Vec3, head: Vec3, hip: f64) -> (Quat, Vec3) {
        let at = self.point(quat::add3(head, quat::scale3(offset, hip)));
        (
            quat::normalize(quat::mul(self.spin(), q)),
            quat::scale3(quat::sub3(at, head), 1.0 / hip),
        )
    }

    /// A pose of `sk` moved.
    pub fn pose(&self, sk: &Skeleton, p: &Pose) -> Result<Pose, String> {
        let head = sk.heads.first().copied().ok_or("the rig has no root")?;
        let q = p.rotations.first().copied().ok_or("a pose with no bones")?;
        let (q, root) = self.root(q, p.root, head, sk.hip_height);
        let mut rotations = p.rotations.clone();
        if let Some(slot) = rotations.first_mut() {
            *slot = q;
        }
        Ok(Pose { rotations, root })
    }
}

/// What moving one frame did.
#[derive(Debug, Clone, PartialEq)]
pub struct MovedFrame {
    pub frame: i64,
    /// The frame's key on the edited layer moved too.
    pub keyed: bool,
    /// What the user should know (not keyed yet, a part key that now holds the root, ...).
    pub notes: Vec<String>,
}

/// What a move did.
#[derive(Debug, Clone, PartialEq)]
pub struct Moved {
    pub keys: usize,
    /// What stayed where it was, said to the user.
    pub notes: Vec<String>,
}

impl Document {
    /// Why the edited layer can't be moved (`None`: it can).
    fn unmovable(&self) -> Result<Option<String>, String> {
        let lay = self.clip.layers.get(self.clip.edit).ok_or("no layer to edit")?;
        let why = match (lay.source, lay.mode) {
            (LayerSource::Link, _) => Some("plays another animation: move that animation instead"),
            (LayerSource::Travel, _) => Some("is a travel: move its path (layer.set waypoints) instead"),
            (_, LayerMode::Additive) => {
                Some("adds onto the layers below: edit the base layer to move the body")
            }
            _ => None,
        };
        Ok(why.map(|w| format!("layer {:?} {w}", lay.name)))
    }

    /// Move the body on the frame shown by `m`, and that frame's key on the edited layer when it
    /// has one; every other key stays. A frame without a key keeps the move as an edit not keyed
    /// yet (as a drag does). Refused (nothing moves) beyond `max_root` hip heights.
    pub fn move_frame(&mut self, m: &Placement, max_root: f64) -> Result<MovedFrame, String> {
        if let Some(why) = self.unmovable()? {
            return Err(why);
        }
        let sk = self.skeleton.clone();
        let (head, hip) = (
            sk.heads.first().copied().ok_or("the rig has no root")?,
            sk.hip_height,
        );
        let far = |o: &Vec3| !o.iter().all(|x| x.is_finite() && x.abs() <= max_root);
        let frame = self.key_frame();
        let moved_shown = m.pose(&sk, &self.pose)?;
        if far(&moved_shown.root) {
            return Err(format!(
                "the pose shown would be more than {max_root} hip heights away"
            ));
        }
        let edit = self.clip.edit;
        let key = self
            .clip
            .layers
            .get(edit)
            .and_then(|l| l.keys.iter().find(|k| k.frame == frame))
            .cloned();
        let mut notes = Vec::new();
        let Some(mut key) = key else {
            self.pose = moved_shown;
            notes.push(format!("frame {frame} has no key: press K to keep the move"));
            return Ok(MovedFrame {
                frame,
                keyed: false,
                notes,
            });
        };
        // the edit shown but not keyed yet moves along and stays an edit
        let shown = self.pose.clone();
        self.go_to(self.frame)?;
        let unkeyed = self.pose != shown;
        if key.only.as_ref().is_some_and(|o| !o.contains(&0)) {
            // a part key without the root: it takes the moved root of the pose shown
            let q = moved_shown
                .rotations
                .first()
                .copied()
                .ok_or("a pose with no bones")?;
            if let Some(slot) = key.rotations.first_mut() {
                *slot = q;
            }
            key.root = moved_shown.root;
            if let Some(o) = key.only.as_mut() {
                o.insert(0, 0);
            }
            notes.push(format!("the key on frame {frame} now holds the root too"));
        } else {
            let q = key.rotations.first().copied().ok_or("a key with no bones")?;
            let (q, o) = m.root(q, key.root, head, hip);
            if far(&o) {
                self.pose = shown;
                return Err(format!(
                    "the key on frame {frame} would be more than {max_root} hip heights away"
                ));
            }
            if let Some(slot) = key.rotations.first_mut() {
                *slot = q;
            }
            key.root = o;
        }
        if key.source == crate::clip::KeySource::Ai {
            // moving a key by hand makes it yours
            key.source = crate::clip::KeySource::Manual;
            notes.push(format!("the AI key on frame {frame} is yours now"));
        }
        self.clip.set_key(key)?;
        self.go_to(self.frame)?;
        if unkeyed {
            self.pose = moved_shown;
        }
        Ok(MovedFrame {
            frame,
            keyed: true,
            notes,
        })
    }

    /// Move every key of the edited layer, the pose shown and the pins by `m`. Refused as a whole
    /// (nothing moves) if a key would land more than `max_root` hip heights away.
    pub fn move_animation(&mut self, m: &Placement, max_root: f64) -> Result<Moved, String> {
        let sk = self.skeleton.clone();
        let (head, hip) = (
            sk.heads.first().copied().ok_or("the rig has no root")?,
            sk.hip_height,
        );
        if let Some(why) = self.unmovable()? {
            return Err(why);
        }
        let edit = self.clip.edit;
        let lay = self.clip.layers.get(edit).ok_or("no layer to edit")?;
        let far = |o: &Vec3| !o.iter().all(|x| x.is_finite() && x.abs() <= max_root);
        let mut keys = lay.keys.clone();
        for k in &mut keys {
            let q = k.rotations.first().copied().ok_or("a key with no bones")?;
            let (q, o) = m.root(q, k.root, head, hip);
            if far(&o) {
                return Err(format!(
                    "the key on frame {} would be more than {max_root} hip heights away",
                    k.frame
                ));
            }
            if let Some(slot) = k.rotations.first_mut() {
                *slot = q;
            }
            k.root = o;
        }
        // an edit not keyed yet moves with the body instead of being dropped
        let shown = self.pose.clone();
        self.go_to(self.frame)?;
        let unkeyed = self.pose != shown;
        let moved_shown = m.pose(&sk, &shown)?;
        if far(&moved_shown.root) {
            self.pose = shown;
            return Err(format!(
                "the pose shown would be more than {max_root} hip heights away"
            ));
        }

        let n = keys.len();
        if let Some(l) = self.clip.layers.get_mut(edit) {
            l.keys = keys;
        }
        for p in self.pins.values_mut() {
            *p = m.point(*p);
        }
        self.go_to(self.frame)?;
        let mut notes = Vec::new();
        if unkeyed || n == 0 {
            self.pose = moved_shown;
        }
        if n == 0 {
            notes.push("this layer has no keys: only the pose shown moved (key it to keep it)".into());
        }
        for (i, l) in self.clip.layers.iter().enumerate() {
            let places_body = l.mode == LayerMode::Override && l.only.as_ref().is_none_or(|o| o.contains(&0));
            if i != edit && !l.muted && places_body {
                notes.push(format!(
                    "layer {:?} places the body itself and stayed where it was",
                    l.name
                ));
            }
        }
        Ok(Moved { keys: n, notes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quarter_turn_carries_x_onto_y() {
        let m = Placement {
            by: [0.0, 0.0, 0.0],
            turn: std::f64::consts::FRAC_PI_2,
            pivot: [1.0, 1.0],
        };
        let p = m.point([2.0, 1.0, 0.5]);
        assert!((p[0] - 1.0).abs() < 1e-12 && (p[1] - 2.0).abs() < 1e-12 && p[2] == 0.5);
    }
}
