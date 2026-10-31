//! Editing the document (a port of cf-animate's `editor.py`): frames, keys, drags and pins.
//!
//! A drag changes the shown pose; [`Document::set_key`] stores it in the clip. Moving to another
//! frame shows the clip's sample there (unkeyed edits are dropped, as in most animation tools).
//!
//! Drag rules, by the bone grabbed:
//! - a hand or a foot: two-bone IK (a foot keeps its orientation);
//! - an elbow or a knee: the hand or foot stays, the joint turns toward the pointer (the pole);
//! - the root: the body moves, the feet stay planted;
//! - any other bone: its parent turns so that the bone's head follows the pointer.

use crate::clip::{Interp, Key, KeySource, LayerMode, LayerSource};
use crate::document::{Document, Pose};
use crate::ik;
use crate::parts;
use crate::quat::{self, Vec3, sub3};

impl Document {
    /// Show the clip's sample at `frame` (any unkeyed edit is dropped); with the physics switch on,
    /// the physics pass's nearest whole frame (a clip over `physics::MAX_FRAMES` plays as keyed).
    pub fn go_to(&mut self, frame: f64) -> Result<(), String> {
        if !frame.is_finite() {
            return Err(format!("frame {frame} is not a number"));
        }
        self.frame = frame;
        if self.clip.base().keys.is_empty() {
            return Ok(());
        }
        if self.physics_shown() {
            // the physics pass works on whole frames: show the nearest one
            let cache = self.physics()?;
            let report = &cache.report;
            let f = (frame.round().max(0.0) as usize).min(report.roots.len().saturating_sub(1));
            if let (Some(r), Some(o)) = (report.rotations.get(f), report.roots.get(f)) {
                self.pose = Pose {
                    rotations: r.clone(),
                    root: *o,
                };
            }
            return Ok(());
        }
        let (r, o) = self.clip.sample_with(&[frame], &self.links)?;
        if let (Some(r), Some(o)) = (r.into_iter().next(), o.first()) {
            self.pose = Pose {
                rotations: r,
                root: *o,
            };
        }
        Ok(())
    }

    /// The physics pass over the whole clip (every frame, all layers), reused while the clip and
    /// its links are unchanged.
    pub fn physics(&mut self) -> Result<std::sync::Arc<crate::document::PhysicsCache>, String> {
        if let Some(c) = &self.physics_cache
            && c.clip == self.clip
            && c.links == self.links
        {
            return Ok(c.clone());
        }
        let n = self.clip.length_with(&self.links).max(1) as usize;
        if n > crate::physics::MAX_FRAMES {
            return Err(format!(
                "{n} frames is too long for the physics pass (most {})",
                crate::physics::MAX_FRAMES
            ));
        }
        let fr: Vec<f64> = (0..n).map(|f| f as f64).collect();
        let (rots, roots) = self.clip.sample_with(&fr, &self.links)?;
        let report = crate::physics::solve_held(
            &self.skeleton,
            &rots,
            &roots,
            self.clip.fps,
            &self.clip.surfaces,
            &self.clip.contacts,
            self.limits.as_deref(),
        )?;
        let c = std::sync::Arc::new(crate::document::PhysicsCache {
            clip: self.clip.clone(),
            links: self.links.clone(),
            report,
        });
        self.physics_cache = Some(c.clone());
        Ok(c)
    }

    /// Whether playback shows the physics pass (the switch is on and the clip is short enough).
    pub fn physics_shown(&self) -> bool {
        self.clip.physics
            && !self.clip.base().keys.is_empty()
            && (self.clip.length_with(&self.links).max(1) as usize) <= crate::physics::MAX_FRAMES
    }

    /// The clip's own sample at `frame`, without the physics pass (the keyed pose).
    pub fn keyed_pose(&self, frame: f64) -> Result<Pose, String> {
        let (r, o) = self.clip.sample_with(&[frame], &self.links)?;
        match (r.into_iter().next(), o.first()) {
            (Some(rotations), Some(root)) => Ok(Pose {
                rotations,
                root: *root,
            }),
            _ => Ok(self.pose.clone()),
        }
    }

    /// `count` frames (1..=64) spread evenly over the clip, first and last included: a contact
    /// sheet's frames.
    pub fn spread_frames(&self, count: usize) -> Vec<f64> {
        let last = (self.clip.length_with(&self.links) - 1).max(0) as f64;
        let n = count.clamp(1, 64);
        if n == 1 {
            return vec![0.0];
        }
        (0..n)
            .map(|i| (last * i as f64 / (n - 1) as f64).round())
            .collect()
    }

    /// A contact sheet's frames from tool arguments: `frames` as given (1 to 64, each 0 or
    /// more), else `count` (default 6) spread over the clip.
    pub fn sheet_frames(&self, args: &serde_json::Value) -> Result<Vec<f64>, String> {
        match args.get("frames").and_then(serde_json::Value::as_array) {
            Some(list) => {
                let f: Vec<f64> = list.iter().filter_map(serde_json::Value::as_f64).collect();
                if f.is_empty()
                    || f.len() != list.len()
                    || f.len() > 64
                    || f.iter().any(|x| !(0.0..=1e7).contains(x))
                {
                    return Err("frames: 1 to 64 frame numbers, 0 or more".into());
                }
                Ok(f)
            }
            None => Ok(self.spread_frames(args["count"].as_u64().unwrap_or(6) as usize)),
        }
    }

    /// The motion shown at these frames, as playback shows it (the physics pass's nearest whole
    /// frame where it plays, else the keys); the document is not changed (a copy runs the pass
    /// when it is not cached yet).
    pub fn shown_poses(&self, frames: &[f64]) -> Result<Vec<Pose>, String> {
        if self.physics_shown() {
            let mut d = self.clone();
            let whole: Vec<usize> = frames.iter().map(|f| f.round().max(0.0) as usize).collect();
            return d.shown_frames(&whole);
        }
        let (r, o) = self.clip.sample_with(frames, &self.links)?;
        Ok(r.into_iter()
            .zip(o)
            .map(|(rotations, root)| Pose { rotations, root })
            .collect())
    }

    /// The motion shown on whole frames (the physics pass where it plays, else the keys).
    fn shown_frames(&mut self, frames: &[usize]) -> Result<Vec<Pose>, String> {
        if self.physics_shown() {
            let cache = self.physics()?;
            let last = cache.report.roots.len().saturating_sub(1);
            return Ok(frames
                .iter()
                .map(|&f| Pose {
                    rotations: cache.report.rotations[f.min(last)].clone(),
                    root: cache.report.roots[f.min(last)],
                })
                .collect());
        }
        let fr: Vec<f64> = frames.iter().map(|&f| f as f64).collect();
        let (r, o) = self.clip.sample_with(&fr, &self.links)?;
        Ok(r.into_iter()
            .zip(o)
            .map(|(rotations, root)| Pose { rotations, root })
            .collect())
    }

    /// The physics overlay on the whole frame nearest `frame` of the motion shown (see
    /// [`crate::physics::Overlay`]); the flight arc comes from the physics pass, so only while it
    /// plays.
    pub fn overlay(&mut self, frame: f64) -> Result<crate::physics::Overlay, String> {
        use crate::physics;
        if !frame.is_finite() {
            return Err(format!("frame {frame} is not a number"));
        }
        let f = frame.round().max(0.0) as usize;
        let body = physics::Body::new(&self.skeleton)?;
        let shown = self.shown_frames(&[f.saturating_sub(1), f])?;
        let (before, here) = (&shown[0], &shown[1]);
        let surfaces = self.clip.surfaces.clone();
        let (com, contacts) = physics::contacts(
            &self.skeleton,
            &body,
            (&here.rotations, here.root),
            (f > 0).then_some((&before.rotations[..], before.root)),
            self.clip.fps,
            &surfaces,
        )?;
        let balance = physics::balance(&self.skeleton, &body, &here.rotations, here.root, &surfaces)?;
        let mut flight = None;
        if self.physics_shown() {
            let cache = self.physics()?;
            if let Some(&(a, b)) = cache.report.flights.iter().find(|(a, b)| *a <= f && f <= *b) {
                let poses = self.shown_frames(&(a..=b).collect::<Vec<_>>())?;
                let arc = poses
                    .iter()
                    .map(|p| {
                        Ok(body.com(
                            &self
                                .skeleton
                                .fk(&p.rotations, p.root)
                                .map_err(|e| e.to_string())?
                                .1,
                        ))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                flight = Some((a, b, arc));
            }
        }
        let keyed = self.keyed_pose(f as f64)?;
        let sk = &self.skeleton;
        let a = sk.fk(&keyed.rotations, keyed.root).map_err(|e| e.to_string())?.1;
        let b = sk.fk(&here.rotations, here.root).map_err(|e| e.to_string())?.1;
        let keyed_moved = match (a.first(), b.first()) {
            (Some(a), Some(b)) => quat::len3(sub3(*a, *b)),
            _ => 0.0,
        };
        Ok(physics::Overlay {
            frame: f,
            com,
            balance,
            contacts,
            flight,
            keyed_moved,
        })
    }

    /// The integer frame keys go on.
    pub fn key_frame(&self) -> i64 {
        self.frame.round() as i64
    }

    /// Bones of some body parts (`None`: the whole body).
    pub fn part_bones(&self, names: &[String]) -> Result<Option<Vec<usize>>, String> {
        parts::union(&self.skeleton, &self.custom_parts, names)
    }

    /// Key the pose shown; `only` keys just those bones. A part keyed on a frame that has a manual
    /// key already goes into that key.
    pub fn set_key(&mut self, interp: Option<Interp>, only: Option<Vec<usize>>) -> Result<(), String> {
        let f = self.key_frame();
        let edit = self.clip.edit;
        let lay = self.clip.layers.get(edit).ok_or("no layer to edit")?;
        if lay.source == LayerSource::Link {
            return Err(format!(
                "layer {:?} plays another animation: key on a keys layer",
                lay.name
            ));
        }
        let old = lay.keys.iter().find(|k| k.frame == f).cloned();
        let interp = interp
            .or(old.as_ref().map(|k| k.interp))
            .unwrap_or(Interp::Smooth);
        if lay.mode == LayerMode::Additive {
            // an additive layer keys the turn on top of what is below it
            let (br, bo) = self.clip.sample_below(&[f as f64], edit, &self.links)?;
            let (br, bo) = (
                br.into_iter().next().unwrap_or_default(),
                bo.first().copied().unwrap_or_default(),
            );
            let rots = br
                .iter()
                .zip(&self.pose.rotations)
                .map(|(b, q)| quat::normalize(quat::mul(quat::conj(*b), *q)))
                .collect();
            let key = Key::new(f, rots, sub3(self.pose.root, bo), interp).with_only(only);
            return self.clip.set_key(key);
        }
        let Some(only) = only else {
            // keying a frame by hand makes it yours, even where the in-betweener had put a key
            return self
                .clip
                .set_key(Key::new(f, self.pose.rotations.clone(), self.pose.root, interp));
        };
        match old {
            Some(old) if old.source == KeySource::Manual => {
                let mut rots = old.rotations.clone();
                let mut root = old.root;
                for &b in &only {
                    if let (Some(slot), Some(q)) = (rots.get_mut(b), self.pose.rotations.get(b)) {
                        *slot = *q;
                    }
                }
                if only.contains(&0) {
                    root = self.pose.root;
                }
                let keep = old.only.map(|o| {
                    let mut s: Vec<usize> = o.into_iter().chain(only.iter().copied()).collect();
                    s.sort_unstable();
                    s.dedup();
                    s
                });
                self.clip.set_key(Key::new(f, rots, root, interp).with_only(keep))
            }
            _ => self.clip.set_key(
                Key::new(f, self.pose.rotations.clone(), self.pose.root, interp).with_only(Some(only)),
            ),
        }
    }

    /// Take bones out of the key on `frame`; a key left with none is deleted.
    pub fn remove_parts(&mut self, frame: i64, only: Option<Vec<usize>>) -> Result<(), String> {
        let n = self.skeleton.len();
        let edit = self.clip.edit;
        let lay = self.clip.layers.get_mut(edit).ok_or("no layer to edit")?;
        let k = lay
            .keys
            .iter_mut()
            .find(|k| k.frame == frame)
            .ok_or_else(|| format!("no key on frame {frame}"))?;
        let Some(only) = only else {
            return self.clip.remove_key(frame);
        };
        let left: Vec<usize> = k
            .only
            .clone()
            .unwrap_or_else(|| (0..n).collect())
            .into_iter()
            .filter(|b| !only.contains(b))
            .collect();
        if left.is_empty() {
            return self.clip.remove_key(frame);
        }
        k.only = Some(left);
        Ok(())
    }

    pub fn delete_key(&mut self, only: Option<Vec<usize>>) -> Result<(), String> {
        let f = self.key_frame();
        let last_whole = only.is_none() && self.clip.keys().len() == 1 && self.clip.edit == 0;
        if last_whole {
            return Err("an animation keeps at least one key".into());
        }
        self.remove_parts(f, only)?;
        self.go_to(self.frame)
    }

    /// After a solve: the locked parts go back to `before`.
    pub fn keep_locked_pub(&mut self, before: &Pose) -> Result<(), String> {
        self.keep_locked(before)
    }

    /// After a drag: the locked parts go back to `before`.
    fn keep_locked(&mut self, before: &Pose) -> Result<(), String> {
        if self.locked.is_empty() {
            return Ok(());
        }
        let bones = self
            .part_bones(&self.locked)?
            .unwrap_or_else(|| (0..self.skeleton.len()).collect());
        for &b in &bones {
            if let (Some(slot), Some(q)) = (self.pose.rotations.get_mut(b), before.rotations.get(b)) {
                *slot = *q;
            }
        }
        if bones.contains(&0) {
            self.pose.root = before.root;
        }
        Ok(())
    }

    /// World joint positions of the pose shown.
    pub fn joints(&self) -> Result<Vec<Vec3>, String> {
        Ok(self
            .skeleton
            .fk(&self.pose.rotations, self.pose.root)
            .map_err(|e| e.to_string())?
            .1)
    }

    /// Pin (or unpin) a bone's head where it is now.
    pub fn pin(&mut self, bone: &str, on: bool) -> Result<(), String> {
        let i = self.bone(bone)?;
        if on {
            let at = self.joints()?.get(i).copied().ok_or("no joint")?;
            self.pins.insert(bone.to_string(), at);
        } else {
            self.pins.remove(bone);
        }
        Ok(())
    }

    /// Move `bone`'s head toward `target` (metres, rig world): with the analytic rules, or with the
    /// full-body solver when joints are pinned (so they stay).
    pub fn drag(&mut self, bone: &str, target: Vec3) -> Result<(), String> {
        let before = self.pose.clone();
        let pinned_others = self.pins.keys().any(|p| p != bone);
        if pinned_others {
            self.reach(bone, target)?;
        } else {
            self.drag_inner(bone, target)?;
        }
        self.keep_locked(&before)
    }

    /// Full-body solve: `bone` toward `target`, every other pin held, inside the joint limits.
    /// Returns the distance (m) left between the bone and its target.
    pub fn reach(&mut self, bone: &str, target: Vec3) -> Result<f64, String> {
        let i = self.bone(bone)?;
        let mut goals = vec![crate::fullbody::Goal { bone: i, target }];
        let now = self.joints()?;
        for (name, at) in &self.pins {
            if name != bone {
                let j = self.bone(name)?;
                goals.push(crate::fullbody::Goal { bone: j, target: *at });
                // a pin holds the bone, not just its head: its first child stays too (a pinned foot
                // stays flat instead of rolling onto its toes)
                if let Some(c) = (0..self.skeleton.len()).find(|&c| self.skeleton.parents[c] == Some(j)) {
                    goals.push(crate::fullbody::Goal {
                        bone: c,
                        target: now[c],
                    });
                }
            }
        }
        let s = crate::fullbody::solve(
            &self.skeleton,
            (&self.pose.rotations, self.pose.root),
            &goals,
            self.limits.as_deref(),
        )?;
        self.pose = Pose {
            rotations: s.rotations,
            root: s.root,
        };
        let at = self.joints()?.get(i).copied().ok_or("no joint")?;
        Ok(quat::len3(quat::sub3(at, target)))
    }

    fn drag_inner(&mut self, bone: &str, target: Vec3) -> Result<(), String> {
        let sk = self.skeleton.clone();
        let i = self.bone(bone)?;
        let (rots, root) = (&self.pose.rotations, self.pose.root);
        let e = |err: crate::CoreError| err.to_string();
        if let Some(limb) = ik::limb_of_effector(bone) {
            self.pose.rotations = ik::two_bone(&sk, rots, root, limb, target, None, None).map_err(e)?;
        } else if let Some((limb, bones)) = ik::LIMBS.iter().find(|(_, b)| b[1] == bone) {
            let end = self
                .joints()?
                .get(self.bone(bones[2])?)
                .copied()
                .ok_or("no joint")?;
            self.pose.rotations = ik::two_bone(&sk, rots, root, limb, end, Some(target), None).map_err(e)?;
        } else if sk.parents.get(i).copied().flatten().is_none() {
            let (r, o) = ik::move_root(&sk, rots, root, target, &["leg.L", "leg.R"]).map_err(e)?;
            self.pose = Pose {
                rotations: r,
                root: o,
            };
        } else {
            self.pose.rotations = ik::turn_parent(&sk, rots, root, i, target).map_err(e)?;
        }
        Ok(())
    }
}
