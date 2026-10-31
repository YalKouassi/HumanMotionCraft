//! Timeline key edits beyond single keys: moving one body part's channels between frames (the
//! dope sheet rows), copying and pasting several keys (mirrored or not), and planting one foot
//! over a span of frames (the foot-contact strip).

use crate::clip::Contact;
use std::sync::Arc;

use crate::clip::{Interp, Key, KeySource, LayerMode, LayerSource};
use crate::document::{Document, Pose};
use crate::{edits, ik, quat};

type R<T> = Result<T, String>;

/// The tag on keys a foot plant writes (the same as `feet.lock`'s, so both read alike).
pub const PLANT_TAG: &str = "foot-lock";
/// Most frames one plant may span.
pub const MAX_PLANT: i64 = 10_000;
/// Most keys one copy may hold.
pub const MAX_COPY: usize = 10_000;

impl Document {
    fn edited_keys_layer(&self) -> R<()> {
        let lay = self.clip.layers.get(self.clip.edit).ok_or("no layer to edit")?;
        if lay.source == LayerSource::Link {
            return Err(format!(
                "layer {:?} plays another animation: edit keys on a keys layer",
                lay.name
            ));
        }
        Ok(())
    }

    /// Move the channels of `bones` from the key on `from` to frame `to`. A key on `to` takes
    /// them (and becomes yours); otherwise a partial key is made there. The source key loses
    /// them, and goes when it has none left. Returns the bones moved.
    pub fn move_parts(&mut self, from: i64, to: i64, bones: &[usize]) -> R<Vec<usize>> {
        self.edited_keys_layer()?;
        let src = self
            .clip
            .keys()
            .iter()
            .find(|k| k.frame == from)
            .cloned()
            .ok_or_else(|| format!("no key on frame {from}"))?;
        let moved: Vec<usize> = bones.iter().copied().filter(|&b| src.sets(b)).collect();
        if moved.is_empty() {
            return Err(format!("the key on frame {from} does not key those parts"));
        }
        if to == from {
            return Ok(moved);
        }
        // the pose on `to` before the move: the moved bones are replaced, the rest are the same
        // after it (only the moved bones leave the source key)
        let (r, o) = self.clip.sample_with(&[to as f64], &self.links)?;
        let here = Pose {
            rotations: r.into_iter().next().unwrap_or_default(),
            root: o.first().copied().unwrap_or_default(),
        };
        self.remove_parts(from, Some(moved.clone()))?;
        let n = self.skeleton.len();
        let e = self.clip.edit;
        let lay = self.clip.layers.get_mut(e).ok_or("no layer to edit")?;
        if let Some(dst) = lay.keys.iter_mut().find(|k| k.frame == to) {
            for &b in &moved {
                if let (Some(slot), Some(q)) = (dst.rotations.get_mut(b), src.rotations.get(b)) {
                    *slot = *q;
                }
            }
            if moved.contains(&0) {
                dst.root = src.root;
            }
            if let Some(o) = &mut dst.only {
                o.extend(moved.iter().copied());
                o.sort_unstable();
                o.dedup();
                if o.len() == n {
                    dst.only = None;
                }
            }
            dst.source = KeySource::Manual;
            dst.model.clear();
        } else {
            let mut rots = here.rotations;
            rots.resize(n, quat::IDENTITY);
            for &b in &moved {
                if let (Some(slot), Some(q)) = (rots.get_mut(b), src.rotations.get(b)) {
                    *slot = *q;
                }
            }
            let root = if moved.contains(&0) { src.root } else { here.root };
            self.clip
                .set_key(Key::new(to, rots, root, src.interp).with_only(Some(moved.clone())))?;
        }
        self.go_to(self.frame)?;
        Ok(moved)
    }

    /// Copy the edited layer's keys on `frames` (each must be a key).
    pub fn copy_keys(&mut self, frames: &[i64]) -> R<usize> {
        if frames.is_empty() {
            return Err("no keys to copy".into());
        }
        if frames.len() > MAX_COPY {
            return Err(format!("copy at most {MAX_COPY} keys at once"));
        }
        let mut keys = Vec::with_capacity(frames.len());
        for f in frames {
            let k = self
                .clip
                .keys()
                .iter()
                .find(|k| k.frame == *f)
                .ok_or_else(|| format!("no key on frame {f}"))?;
            if !keys.iter().any(|x: &Key| x.frame == *f) {
                keys.push(k.clone());
            }
        }
        keys.sort_by_key(|k| k.frame);
        let n = keys.len();
        self.key_clipboard = Some(Arc::new(keys));
        Ok(n)
    }

    /// Paste the copied keys with the first on frame `at`, the rest at their distances from it.
    /// A pasted key replaces a key on its frame. `mirror`: left and right swap, and the root's
    /// sideways (x) motion is mirrored about where the first copied key stood. Returns the frames
    /// pasted and the frames whose keys were replaced.
    pub fn paste_keys(&mut self, at: i64, mirror: bool) -> R<(Vec<i64>, Vec<i64>)> {
        self.edited_keys_layer()?;
        let keys = self
            .key_clipboard
            .clone()
            .ok_or("no keys copied yet: select keys and copy them first")?;
        let first = keys.first().ok_or("no keys copied yet")?;
        let (f0, x0) = (first.frame, first.root[0]);
        let mut pasted = Vec::with_capacity(keys.len());
        let mut replaced = Vec::new();
        for k in keys.iter() {
            let f = at.saturating_add(k.frame - f0);
            if !(0..=edits_max_frame()).contains(&f) {
                return Err(format!(
                    "a key would land on frame {f}, outside 0..={}",
                    edits_max_frame()
                ));
            }
            let mut key = k.clone();
            key.frame = f;
            if mirror {
                let m = edits::mirrored(
                    &self.skeleton,
                    &Pose {
                        rotations: k.rotations.clone(),
                        root: k.root,
                    },
                )?;
                key.rotations = m.rotations;
                key.root = [m.root[0] + 2.0 * x0, m.root[1], m.root[2]];
                key.only = edits::mirror_bones(&self.skeleton, k.only.as_deref());
            }
            if self.clip.keys().iter().any(|o| o.frame == f) {
                replaced.push(f);
            }
            self.clip.set_key(key)?;
            pasted.push(f);
        }
        self.go_to(at as f64)?;
        Ok((pasted, replaced))
    }

    /// Plant one foot (`L` or `R`); see [`Document::plant`].
    pub fn plant_foot(&mut self, side: &str, a: i64, b: i64) -> R<usize> {
        self.plant("leg", "foot", side, a, b)
    }

    /// Hold one hand (`L` or `R`); see [`Document::plant`].
    pub fn plant_hand(&mut self, side: &str, a: i64, b: i64) -> R<usize> {
        self.plant("arm", "wrist", side, a, b)
    }

    /// Hold a hand or foot through frames `a..=b`, at any height: its head held at its mean
    /// position there, the limb bent to reach it. Frames with a key get the limb's new rotations;
    /// frames without get an in-between key tagged [`PLANT_TAG`]. The hold is recorded on the clip
    /// ([`crate::clip::Contact`]), so the physics pass keeps it. Returns the frames changed.
    fn plant(&mut self, limb: &str, end: &str, side: &str, a: i64, b: i64) -> R<usize> {
        self.check_plant(side, a, b)?;
        let additive = self
            .clip
            .layers
            .get(self.clip.edit)
            .is_some_and(|l| l.mode == LayerMode::Additive);
        if additive {
            return Err("plant feet on a layer that replaces the pose, not an additive one".into());
        }
        if self.clip.base().keys.is_empty() {
            return Err("the animation has no keys".into());
        }
        let sk = self.skeleton.clone();
        let bone = format!("{end}.{side}");
        let foot = sk.bone(&bone).ok_or_else(|| format!("the rig has no {bone}"))?;
        let frames: Vec<f64> = (a..=b).map(|f| f as f64).collect();
        let (rots, roots) = self.clip.sample_with(&frames, &self.links)?;
        let mut sum = [0.0; 3];
        for (r, o) in rots.iter().zip(&roots) {
            let (_, p) = sk.fk(r, *o).map_err(|e| e.to_string())?;
            sum = quat::add3(sum, *p.get(foot).ok_or("no foot joint")?);
        }
        let at = quat::scale3(sum, 1.0 / frames.len() as f64);
        let mut changed = 0;
        for (i, (r, o)) in rots.iter().zip(&roots).enumerate() {
            let f = a + i as i64;
            let leg = format!("{limb}.{side}");
            let pole = ik::own_pole(&sk, r, *o, &leg).map_err(|e| e.to_string())?;
            let solved = ik::two_bone(&sk, r, *o, &leg, at, pole, None).map_err(|e| e.to_string())?;
            let legs: Vec<usize> = (0..solved.len()).filter(|&j| solved.get(j) != r.get(j)).collect();
            if legs.is_empty() {
                continue;
            }
            changed += 1;
            let e = self.clip.edit;
            let lay = self.clip.layers.get_mut(e).ok_or("no layer to edit")?;
            if let Some(k) = lay.keys.iter_mut().find(|k| k.frame == f) {
                for &j in &legs {
                    if let (Some(slot), Some(q)) = (k.rotations.get_mut(j), solved.get(j)) {
                        *slot = quat::normalize(*q);
                    }
                }
                if let Some(only) = &mut k.only {
                    only.extend(legs.iter().copied());
                    only.sort_unstable();
                    only.dedup();
                }
            } else {
                let mut k = Key::new(f, solved, *o, Interp::Linear);
                k.source = KeySource::Ai;
                k.model = PLANT_TAG.into();
                self.clip.set_key(k)?;
            }
        }
        // the new hold replaces any of this limb it overlaps
        self.clip
            .contacts
            .retain(|c| !(c.bone == bone && c.from <= b && a <= c.to));
        self.clip.contacts.push(Contact {
            bone,
            from: a,
            to: b,
            at,
        });
        self.clip.contacts.sort_by_key(|c| (c.from, c.to));
        self.go_to(self.frame)?;
        Ok(changed)
    }

    /// Take a foot's plant out; see [`Document::unplant_limb`].
    pub fn unplant(&mut self, side: &str, a: i64, b: i64) -> R<usize> {
        self.unplant_limb("foot", side, a, b)
    }

    /// Take a hold out: the in-between keys a plant (or `feet.lock`) wrote on frames `a..=b` go,
    /// and this limb's holds that overlap them. Keys of yours there keep their planted limbs
    /// (undo restores them). Returns how many keys went.
    pub fn unplant_limb(&mut self, end: &str, side: &str, a: i64, b: i64) -> R<usize> {
        self.check_plant(side, a, b)?;
        let e = self.clip.edit;
        let lay = self.clip.layers.get_mut(e).ok_or("no layer to edit")?;
        let before = lay.keys.len();
        lay.keys
            .retain(|k| !(k.source == KeySource::Ai && k.model == PLANT_TAG && a <= k.frame && k.frame <= b));
        let n = before - lay.keys.len();
        let bone = format!("{end}.{side}");
        let holds = self.clip.contacts.len();
        self.clip
            .contacts
            .retain(|c| !(c.bone == bone && c.from <= b && a <= c.to));
        if n == 0 && self.clip.contacts.len() == holds {
            return Err(format!("nothing is planted on frames {a}–{b}"));
        }
        self.go_to(self.frame)?;
        Ok(n)
    }

    fn check_plant(&self, side: &str, a: i64, b: i64) -> R<()> {
        if side != "L" && side != "R" {
            return Err(format!("foot {side:?}: L or R"));
        }
        if b <= a {
            return Err(format!("a plant runs over two frames or more, not {a}–{b}"));
        }
        if b - a > MAX_PLANT {
            return Err(format!("plant at most {MAX_PLANT} frames at once"));
        }
        self.edited_keys_layer()
    }
}

fn edits_max_frame() -> i64 {
    crate::clipfile::MAX_FRAME
}
