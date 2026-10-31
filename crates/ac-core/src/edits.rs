//! Clip edits behind the timeline and the posing tools (a port of cf-animate's `edits.py`).
//!
//! Keys are manual (posed by hand) or AI (made by the in-betweener). A *span* runs between two
//! neighbouring manual keys; the AI keys inside it are that span's fill. Copying or moving a
//! manual key refills every span it touches that was filled, so the in-between follows the key.

use std::collections::{BTreeMap, BTreeSet};

use crate::clip::{Clip, Interp, Key, KeySource, LayerMode, Links};
use crate::document::{Document, Pose};
use crate::ik;
use crate::parts::mirror_name;
use crate::quat::{self, Mat3, Quat, Vec3};
use crate::rig::Skeleton;

pub const MAX_FILL: i64 = 45;
/// The model tag of AI keys made from a text prompt (the text-to-motion helper): `"text"` and the
/// prompt, `"text A person waves."` (the timeline's band shows the prompt). Such a span is never
/// cleared or refilled on its own: an edit next to it leaves it as it is and says so, and only an
/// explicit command (`ai.fill`, `ai.clear`, `ai.toggle`, making it again) replaces it.
pub const GENERATED: &str = "text";
/// The model tag of AI keys made from the poses around a gap (the pose-to-pose helper,
/// `posefill.fill`): held like [`GENERATED`] spans.
pub const POSE_FILL: &str = "posefill";
/// Every tag of motion a helper made, which an edit beside it leaves as it is, and the command
/// that makes it again.
pub const GENERATED_TAGS: &[(&str, &str)] = &[(GENERATED, "text.fill"), (POSE_FILL, "posefill.fill")];
pub const TRACKED: &[&str] = &["root", "wrist.L", "wrist.R", "foot.L", "foot.R"];
const SPINE: &[&str] = &["spine05", "spine04", "spine03", "spine02", "spine01"];
/// Of a hip bend the lower back takes: people round the back, not only fold at the hip.
pub const SPINE_SHARE: f64 = 0.5;

type R<T> = Result<T, String>;

/// An in-betweener: the frames strictly between manual keys `a` and `b`.
pub trait Filler {
    fn fill(&self, clip: &Clip, a: i64, b: i64) -> R<(Vec<Vec<Quat>>, Vec<Vec3>, String)>;
    /// Fills gaps longer than [`MAX_FILL`].
    fn long(&self) -> bool {
        false
    }
    /// The model tags of the anchor keys a long fill placed inside `a..b` (`(frame, tag)`,
    /// `crate::anchors::tag`), set by [`refill`] once the frames are written, so a key move or
    /// copy that refills a long span keeps its anchors marked as the first fill did (the default:
    /// none).
    fn anchor_tags(&self, _a: i64, _b: i64) -> Vec<(i64, String)> {
        Vec::new()
    }

    /// The hash of the model's weights, kept on every key it makes ([`Key::model_hash`]) so a
    /// saved animation says which weights made its motion; empty when the filler has none.
    fn hash(&self) -> String {
        String::new()
    }
    /// Several of your keys at once (a model trained with several keys in a window): the frames
    /// `keys[0]+1 .. keys[last]-1`, the keys inside given back as they are. The default refuses.
    fn fill_keys(&self, _clip: &Clip, keys: &[i64]) -> R<(Vec<Vec<Quat>>, Vec<Vec3>, String)> {
        Err(format!(
            "filling the gaps between {} keys in one pass is not supported by this filler",
            keys.len()
        ))
    }
}

/// Frames either side of a foot's contact over which a lock eases in and out.
pub const LOCK_FADE: usize = 4;
/// The most times [`Document::hold_contacts`] goes over a fill.
const HOLD_PASSES: usize = 3;

/// How far `feet.lock` moves a foot on each frame, across the floor only (a heel may rise as the foot
/// rolls). `balls`: where the ball of the foot is, which stays put as the foot rolls over it. Through
/// a contact (a run of `flags`), to the
/// run's mean place; on the [`LOCK_FADE`] frames either side, that move fades out. A run of one frame
/// is not a contact.
fn lock_moves(balls: &[Vec3], flags: &[bool]) -> Vec<Vec3> {
    let n = balls.len().min(flags.len());
    let mut held = vec![false; n];
    let mut by = vec![[0.0; 3]; n];
    let mut ends: Vec<(usize, usize)> = Vec::new();
    let mut f = 0;
    while f < n {
        let start = f;
        while f < n && flags[f] {
            f += 1;
        }
        if f - start >= 2 {
            let sum = balls[start..f].iter().fold([0.0; 3], |s, a| quat::add3(s, *a));
            let at = quat::scale3(sum, 1.0 / (f - start) as f64);
            for i in start..f {
                held[i] = true;
                by[i] = [at[0] - balls[i][0], at[1] - balls[i][1], 0.0];
            }
            ends.push((start, f - 1));
        }
        f = f.max(start + 1);
    }
    faded(&by, &held, &ends)
}

/// `by` with each contact's move (`ends`) fading out over the [`LOCK_FADE`] frames either side.
fn faded(by: &[Vec3], held: &[bool], ends: &[(usize, usize)]) -> Vec<Vec3> {
    let n = by.len();
    let reach = LOCK_FADE + 1;
    let smooth = |d: usize| {
        let u = 1.0 - d as f64 / reach as f64;
        u * u * (3.0 - 2.0 * u)
    };
    let mut fades = vec![[0.0; 3]; n];
    for &(a, b) in ends {
        for d in 1..reach {
            if let Some(i) = a.checked_sub(d).filter(|&i| !held[i]) {
                fades[i] = quat::add3(fades[i], quat::scale3(by[a], smooth(d)));
            }
            if let Some(i) = Some(b + d).filter(|&i| i < n && !held[i]) {
                fades[i] = quat::add3(fades[i], quat::scale3(by[b], smooth(d)));
            }
        }
    }
    by.iter().zip(&fades).map(|(a, b)| quat::add3(*a, *b)).collect()
}

/// How far to move a foot on each frame so the point it stands on stays put through each contact.
/// `drift`: how far that point has travelled across the floor by each frame; `down`: the foot is in
/// contact; `fixed`: a frame that can't change (your key, an anchor). A contact holds at its mean
/// place, or at the place on its fixed frames, going straight from one to the next when it has
/// several; the moves fade out either side as [`lock_moves`]'s do, and a fixed frame never moves.
fn hold_moves(drift: &[Vec3], down: &[bool], fixed: &[bool]) -> Vec<Vec3> {
    let n = drift.len().min(down.len()).min(fixed.len());
    let mut held = vec![false; n];
    let mut by = vec![[0.0; 3]; n];
    let mut ends: Vec<(usize, usize)> = Vec::new();
    let mut f = 0;
    while f < n {
        let start = f;
        while f < n && down[f] {
            f += 1;
        }
        if f - start >= 2 {
            let pins: Vec<usize> = (start..f).filter(|&i| fixed[i]).collect();
            let mean = quat::scale3(
                drift[start..f].iter().fold([0.0; 3], |s, d| quat::add3(s, *d)),
                1.0 / (f - start) as f64,
            );
            for i in start..f {
                let at = match (pins.iter().rfind(|&&p| p <= i), pins.iter().find(|&&p| p >= i)) {
                    (Some(&p), Some(&q)) if q > p => {
                        let w = (i - p) as f64 / (q - p) as f64;
                        quat::add3(drift[p], quat::scale3(quat::sub3(drift[q], drift[p]), w))
                    }
                    (Some(&p), _) | (_, Some(&p)) => drift[p],
                    (None, None) => mean,
                };
                held[i] = true;
                by[i] = [at[0] - drift[i][0], at[1] - drift[i][1], 0.0];
            }
            ends.push((start, f - 1));
        }
        f = f.max(start + 1);
    }
    let mut out = faded(&by, &held, &ends);
    for (m, _) in out.iter_mut().zip(fixed).filter(|(_, fix)| **fix) {
        *m = [0.0; 3];
    }
    out
}

fn keys_mut(clip: &mut Clip) -> R<&mut Vec<Key>> {
    let e = clip.edit;
    clip.layers
        .get_mut(e)
        .map(|l| &mut l.keys)
        .ok_or_else(|| "no layer to edit".into())
}

pub fn manual_frames(clip: &Clip) -> Vec<i64> {
    clip.keys()
        .iter()
        .filter(|k| k.source == KeySource::Manual)
        .map(|k| k.frame)
        .collect()
}

/// The manual keys `a <= f < b` around a frame.
pub fn span_around(clip: &Clip, f: i64) -> Option<(i64, i64)> {
    let m = manual_frames(clip);
    let a = m.iter().copied().filter(|&x| x <= f).max()?;
    let b = m.iter().copied().filter(|&x| x > f).min()?;
    Some((a, b))
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Band {
    pub from: i64,
    pub to: i64,
    pub model: String,
}

/// Runs of consecutive AI keys (inclusive frames).
pub fn ai_bands(clip: &Clip) -> Vec<Band> {
    let mut bands: Vec<Band> = Vec::new();
    let mut open = false;
    for k in clip.keys() {
        if k.source != KeySource::Ai {
            open = false;
            continue;
        }
        match bands.last_mut() {
            Some(b) if open && k.frame == b.to + 1 && k.model == b.model => b.to = k.frame,
            _ => {
                bands.push(Band {
                    from: k.frame,
                    to: k.frame,
                    model: k.model.clone(),
                });
                open = true;
            }
        }
    }
    bands
}

/// The command that made an AI key with this model tag, when a helper did ([`GENERATED_TAGS`]).
fn generated_by(model: &str) -> Option<&'static str> {
    GENERATED_TAGS.iter().find_map(|(tag, command)| {
        model
            .strip_prefix(tag)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
            .then_some(*command)
    })
}

/// Whether an AI key's model tag says a helper made it ([`GENERATED`], [`POSE_FILL`]).
pub fn is_generated(model: &str) -> bool {
    generated_by(model).is_some()
}

/// Whether the span between manual keys `a` and `b` holds motion a helper made.
pub fn generated_in(clip: &Clip, a: i64, b: i64) -> bool {
    clip.keys()
        .iter()
        .any(|k| k.source == KeySource::Ai && a < k.frame && k.frame < b && is_generated(&k.model))
}

/// What an edit says when it leaves a span a helper made as it was.
pub fn generated_note(clip: &Clip, a: i64, b: i64) -> String {
    let (what, again) = clip
        .keys()
        .iter()
        .find(|k| k.source == KeySource::Ai && a < k.frame && k.frame < b && is_generated(&k.model))
        .and_then(|k| generated_by(&k.model).map(|c| (k.model.starts_with(POSE_FILL), c)))
        .map_or(("from text", "text.fill"), |(pose, c)| {
            (if pose { "from poses" } else { "from text" }, c)
        });
    format!(
        "{a}-{b}: the motion made {what} was left as it was; make it again ({again}) to meet the changed key"
    )
}

/// Drop the AI keys strictly between `a` and `b`; returns how many.
pub fn clear_ai(clip: &mut Clip, a: i64, b: i64) -> R<usize> {
    let keys = keys_mut(clip)?;
    let before = keys.len();
    keys.retain(|k| !(k.source == KeySource::Ai && a < k.frame && k.frame < b));
    Ok(before - keys.len())
}

/// Make the AI keys in `a..=b` manual.
pub fn keep_ai(clip: &mut Clip, a: i64, b: i64) -> R<usize> {
    let mut n = 0;
    for k in keys_mut(clip)?
        .iter_mut()
        .filter(|k| k.source == KeySource::Ai && a <= k.frame && k.frame <= b)
    {
        k.source = KeySource::Manual;
        k.model.clear();
        k.model_hash.clear();
        n += 1;
    }
    Ok(n)
}

/// Replace the fill between manual keys `a` and `b`; returns a note ("" when filled).
pub fn refill(clip: &mut Clip, a: i64, b: i64, filler: &dyn Filler, links: &dyn Links) -> R<String> {
    clear_ai(clip, a, b)?;
    if b - a - 1 < 1 {
        return Ok(String::new());
    }
    if b - a - 1 > MAX_FILL && !filler.long() {
        return Ok(format!("{a}-{b}: gap over {MAX_FILL} frames, left to the spline"));
    }
    let (mut rots, mut roots, tag) = filler.fill(clip, a, b)?;
    let hash = filler.hash();
    on_top(clip, a, b, &mut rots, &mut roots, links)?;
    for (i, (r, o)) in rots.into_iter().zip(roots).enumerate() {
        let mut k = Key::new(a + 1 + i as i64, r, o, Interp::Smooth);
        k.source = KeySource::Ai;
        k.model = tag.clone();
        k.model_hash = hash.clone();
        clip.set_key(k)?;
    }
    // the anchor poses of a long fill keep their own tag (the timeline's band, the ease through them)
    for (f, tag) in filler.anchor_tags(a, b) {
        if let Some(k) = keys_mut(clip)?
            .iter_mut()
            .find(|k| k.frame == f && k.source == KeySource::Ai)
        {
            k.model = tag;
        }
    }
    Ok(String::new())
}

/// On an additive layer a fill is a whole pose: keep its turn on top of the layers below.
/// `rots`, `roots`: the frames `a+1 .. b-1`.
fn on_top(
    clip: &Clip,
    a: i64,
    b: i64,
    rots: &mut [Vec<Quat>],
    roots: &mut [Vec3],
    links: &dyn Links,
) -> R<()> {
    let additive = clip
        .layers
        .get(clip.edit)
        .is_some_and(|l| l.mode == LayerMode::Additive);
    if !additive {
        return Ok(());
    }
    let frames: Vec<f64> = ((a + 1)..b).map(|f| f as f64).collect();
    let (br, bo) = clip.sample_below(&frames, clip.edit, links)?;
    for (i, fr) in rots.iter_mut().enumerate() {
        for (q, base) in fr.iter_mut().zip(br.get(i).into_iter().flatten()) {
            *q = quat::normalize(quat::mul(quat::conj(*base), *q));
        }
        if let (Some(o), Some(base)) = (roots.get_mut(i), bo.get(i)) {
            *o = quat::sub3(*o, *base);
        }
    }
    Ok(())
}

/// Replace the fills between neighbouring manual keys `keys` (in frame order) in one pass
/// ([`Filler::fill_keys`]): the keys themselves stay, every frame between them becomes an AI
/// key. Returns a note ("" when filled).
pub fn refill_keys(clip: &mut Clip, keys: &[i64], filler: &dyn Filler, links: &dyn Links) -> R<String> {
    let (Some(&a), Some(&z)) = (keys.first(), keys.last()) else {
        return Err("a fill over several keys needs two or more".into());
    };
    for w in keys.windows(2) {
        let &[p, q] = w else { continue };
        clear_ai(clip, p, q)?;
    }
    if z - a - 1 < 1 {
        return Ok(String::new());
    }
    let (mut rots, mut roots, tag) = filler.fill_keys(clip, keys)?;
    let hash = filler.hash();
    if rots.len() != (z - a - 1) as usize || roots.len() != rots.len() {
        return Err(format!(
            "the fill over {a}–{z} gave {} frames for {}",
            rots.len(),
            z - a - 1
        ));
    }
    on_top(clip, a, z, &mut rots, &mut roots, links)?;
    for (i, (r, o)) in rots.into_iter().zip(roots).enumerate() {
        let f = a + 1 + i as i64;
        if keys.contains(&f) {
            continue; // a key of yours inside the stretch stays as it is
        }
        let mut k = Key::new(f, r, o, Interp::Smooth);
        k.source = KeySource::Ai;
        k.model = tag.clone();
        k.model_hash = hash.clone();
        clip.set_key(k)?;
    }
    Ok(String::new())
}

fn filled_spans(clip: &Clip) -> Vec<(i64, i64)> {
    let m = manual_frames(clip);
    let ai: BTreeSet<i64> = clip
        .keys()
        .iter()
        .filter(|k| k.source == KeySource::Ai)
        .map(|k| k.frame)
        .collect();
    m.windows(2)
        .filter_map(|w| match w {
            [a, b] if ai.iter().any(|&f| *a < f && f < *b) => Some((*a, *b)),
            _ => None,
        })
        .collect()
}

/// Drop the in-betweener's AI keys in the spans on both sides of `f` (keys made from text stay).
fn clear_ai_around(clip: &mut Clip, f: i64) -> R<()> {
    let m = manual_frames(clip);
    let a = m.iter().copied().filter(|&x| x < f).max().unwrap_or(f);
    let b = m.iter().copied().filter(|&x| x > f).min().unwrap_or(f);
    keys_mut(clip)?.retain(|k| {
        !(k.source == KeySource::Ai && a < k.frame && k.frame < b && k.frame != f && !is_generated(&k.model))
    });
    Ok(())
}

fn sample1(clip: &Clip, f: f64, links: &dyn Links) -> R<Pose> {
    let (r, o) = clip.sample_with(&[f], links)?;
    Ok(Pose {
        rotations: r.into_iter().next().unwrap_or_default(),
        root: o.first().copied().unwrap_or_default(),
    })
}

impl Document {
    /// Fill again the spans between your keys that hold one of `frames` and were filled (`was`),
    /// each finished as `ai.fill` finishes it ([`Document::refill_finished`]). A span made by a
    /// helper stays as it is, and a note says so.
    fn refill_touching(&mut self, frames: &[i64], was: &[(i64, i64)]) -> R<Vec<String>> {
        let m = manual_frames(&self.clip);
        let spans: BTreeSet<(i64, i64)> = m
            .windows(2)
            .filter_map(|w| match w {
                [a, b] if frames.iter().any(|&f| *a <= f && f <= *b) => Some((*a, *b)),
                _ => None,
            })
            .collect();
        let (made, spans): (Vec<_>, Vec<_>) = spans
            .into_iter()
            .partition(|&(a, b)| generated_in(&self.clip, a, b));
        let mut notes: Vec<String> = made
            .into_iter()
            .map(|(a, b)| generated_note(&self.clip, a, b))
            .collect();
        let Some(filler) = self.ai_filler() else {
            return Ok(notes);
        };
        for (a, b) in spans {
            if was.iter().any(|&(a2, b2)| !(b2 <= a || a2 >= b)) {
                notes.extend(self.refill_finished(&filler, a, b)?);
            }
        }
        Ok(notes)
    }

    /// The pose on frame `src` (keyed or not) as a manual key on `dst`.
    pub fn copy_key(&mut self, src: i64, dst: i64) -> R<Vec<String>> {
        let dst = dst.max(0);
        let was = filled_spans(&self.clip);
        let p = sample1(&self.clip, src as f64, &self.links)?;
        keys_mut(&mut self.clip)?.retain(|k| k.frame != dst);
        self.clip
            .set_key(Key::new(dst, p.rotations, p.root, Interp::Smooth))?;
        clear_ai_around(&mut self.clip, dst)?;
        let notes = self.refill_touching(&[dst], &was)?;
        self.go_to(dst as f64)?;
        Ok(notes)
    }

    /// Move the key on `src` to `dst`; the spans it leaves and joins are refilled.
    pub fn move_key(&mut self, src: i64, dst: i64) -> R<Vec<String>> {
        let dst = dst.max(0);
        let mut key = self
            .clip
            .keys()
            .iter()
            .find(|k| k.frame == src)
            .cloned()
            .ok_or_else(|| {
                crate::error::hinted(
                    format!("no key on frame {src}"),
                    "editor.state lists the keys of the edited layer",
                )
            })?;
        let was = filled_spans(&self.clip);
        if dst != src {
            keys_mut(&mut self.clip)?.retain(|k| k.frame != src && k.frame != dst);
            key.frame = dst;
            self.clip.set_key(key.clone())?;
        }
        let mut notes = Vec::new();
        if key.source == KeySource::Manual {
            clear_ai_around(&mut self.clip, dst)?;
            notes = self.refill_touching(&[dst, src], &was)?;
        }
        self.go_to(dst as f64)?;
        Ok(notes)
    }

    /// Several keys at once; an animation keeps at least one.
    pub fn delete_keys(&mut self, frames: &[i64]) -> R<()> {
        let drop: BTreeSet<i64> = frames.iter().copied().collect();
        let keys = keys_mut(&mut self.clip)?;
        if keys.iter().all(|k| drop.contains(&k.frame)) && self.clip.edit == 0 {
            return Err("an animation keeps at least one key".into());
        }
        keys_mut(&mut self.clip)?.retain(|k| !drop.contains(&k.frame));
        self.go_to(self.frame)
    }

    /// `flip`: the whole pose mirrored left <-> right; `l2r` / `r2l`: one side copied onto the other.
    pub fn mirror(&mut self, mode: &str) -> R<()> {
        if mode == "flip" {
            self.pose = mirrored(&self.skeleton, &self.pose)?;
            return Ok(());
        }
        let side = match mode {
            "l2r" => ".R",
            "r2l" => ".L",
            other => return Err(format!("unknown mirror mode {other:?}")),
        };
        let m = mirrored(&self.skeleton, &self.pose)?;
        for (i, n) in self.skeleton.names.iter().enumerate() {
            if n.ends_with(side)
                && let (Some(slot), Some(q)) = (self.pose.rotations.get_mut(i), m.rotations.get(i))
            {
                *slot = *q;
            }
        }
        Ok(())
    }

    /// Put `only`'s bones of a pose on the pose shown (all of it for `None`); the root's height
    /// comes along when the root is in. The body stays where it stands (x, y kept).
    pub fn paste_part(&mut self, pose: &Pose, only: Option<&[usize]>) {
        let Some(only) = only else {
            let mut r = pose.root;
            r[0] = self.pose.root[0];
            r[1] = self.pose.root[1];
            self.pose = Pose {
                rotations: pose.rotations.clone(),
                root: r,
            };
            return;
        };
        for &b in only {
            if let (Some(slot), Some(q)) = (self.pose.rotations.get_mut(b), pose.rotations.get(b)) {
                *slot = *q;
            }
        }
        if only.contains(&0) {
            self.pose.root[2] = pose.root[2];
        }
    }

    /// Hold `only`'s bones still from frame `a` to `b`.
    pub fn hold_part(&mut self, a: i64, b: i64, only: Option<Vec<usize>>) -> R<()> {
        if b <= a {
            return Err("hold needs a later end frame".into());
        }
        let bones = only.unwrap_or_else(|| (0..self.skeleton.len()).collect());
        self.go_to(a as f64)?;
        let new = !self.clip.keys().iter().any(|k| k.frame == a);
        // a new start key holds (step): a smooth curve between two equal keys still swings a little
        self.set_key(if new { Some(Interp::Step) } else { None }, Some(bones.clone()))?;
        let held = self.pose.clone();
        let inside: Vec<i64> = self
            .clip
            .keys()
            .iter()
            .map(|k| k.frame)
            .filter(|&f| a < f && f < b)
            .collect();
        for f in inside {
            self.remove_parts(f, Some(bones.clone()))?;
        }
        self.go_to(b as f64)?;
        self.paste_part(&held, Some(&bones));
        self.set_key(None, Some(bones))?;
        self.go_to(a as f64)
    }

    /// Turn `bone` (from the pose `base`) by `angle` radians about a world `axis`. The hips spread
    /// the turn into the lower back and keep the feet.
    pub fn rotate(&mut self, base: &[Quat], bone: &str, axis: Vec3, angle: f64) -> R<()> {
        let sk = self.skeleton.clone();
        let root = self.pose.root;
        let i = self.bone(bone)?;
        self.pose.rotations = if sk.parents.get(i).copied().flatten().is_none() {
            bend_hips(&sk, base, root, axis, angle, SPINE_SHARE)?
        } else {
            turn(&sk, base, root, i, axis, angle)?
        };
        Ok(())
    }

    /// Re-pose a bent-over pose made with a straight back; returns degrees moved.
    pub fn spread_bend(&mut self, share: f64) -> R<f64> {
        let sk = self.skeleton.clone();
        let (rots, root) = (self.pose.rotations.clone(), self.pose.root);
        let (_, p) = sk.fk(&rots, root).map_err(|e| e.to_string())?;
        let s3 = sk.bone("spine03").ok_or("no spine03")?;
        let v = quat::sub3(*p.get(s3).ok_or("joint")?, *p.first().ok_or("joint")?);
        let axis = cross(v, [0.0, 0.0, 1.0]);
        if quat::len3(axis) < 1e-9 {
            return Ok(0.0);
        }
        let lean = (v[2] / quat::len3(v)).clamp(-1.0, 1.0).acos();
        let mut out = turn(&sk, &rots, root, 0, axis, lean * share)?;
        for b in SPINE {
            let i = sk.bone(b).ok_or_else(|| format!("no bone {b}"))?;
            out = turn(&sk, &out, root, i, axis, -lean * share / SPINE.len() as f64)?;
        }
        self.pose.rotations = keep_feet(&sk, out, root, &rots)?;
        Ok((lean * share).to_degrees())
    }

    /// Joint positions on each frame.
    pub fn positions(&self, frames: &[f64]) -> R<Vec<Vec<Vec3>>> {
        let (rots, roots) = self.clip.sample_with(frames, &self.links)?;
        rots.iter()
            .zip(&roots)
            .map(|(r, o)| self.skeleton.fk(r, *o).map(|x| x.1).map_err(|e| e.to_string()))
            .collect()
    }

    /// Frames where each foot is planted: its lowest point near the floor and nearly still.
    pub fn contacts(&self, height: f64, speed: f64) -> R<BTreeMap<&'static str, Vec<bool>>> {
        let mut out = BTreeMap::new();
        if self.clip.base().keys.is_empty() {
            out.insert("L", Vec::new());
            out.insert("R", Vec::new());
            return Ok(out);
        }
        let frames: Vec<f64> = (0..self.clip.length()).map(|f| f as f64).collect();
        let p = self.positions(&frames)?;
        let idx = |n: &str| self.skeleton.bone(n).ok_or_else(|| format!("no bone {n}"));
        let mut floor = f64::INFINITY;
        for s in ["L", "R"] {
            for b in ["foot", "toe3-1"] {
                let i = idx(&format!("{b}.{s}"))?;
                floor = p
                    .iter()
                    .filter_map(|fr| fr.get(i))
                    .fold(floor, |m, j| m.min(j[2]));
            }
        }
        for s in ["L", "R"] {
            let (fi, ti) = (idx(&format!("foot.{s}"))?, idx(&format!("toe3-1.{s}"))?);
            let mut flags = Vec::with_capacity(p.len());
            for (n, fr) in p.iter().enumerate() {
                let (Some(foot), Some(toe)) = (fr.get(fi), fr.get(ti)) else {
                    continue;
                };
                let low = foot[2].min(toe[2]);
                let v = match n
                    .checked_sub(1)
                    .and_then(|m| p.get(m))
                    .and_then(|prev| prev.get(ti))
                {
                    Some(prev) => ((toe[0] - prev[0]).hypot(toe[1] - prev[1])) * self.clip.fps,
                    None => 0.0,
                };
                flags.push(low - floor < height && v < speed);
            }
            out.insert(if s == "L" { "L" } else { "R" }, flags);
        }
        Ok(out)
    }

    /// Hold a foot in place through frames `a..=b` when it stands at the same spot in both keys.
    pub fn plant_feet(&mut self, a: i64, b: i64, still: f64, ground: f64) -> R<usize> {
        let additive = self
            .clip
            .layers
            .get(self.clip.edit)
            .is_some_and(|l| l.mode == LayerMode::Additive);
        if b - a < 2 || additive {
            return Ok(0);
        }
        let sk = self.skeleton.clone();
        let frames: Vec<f64> = (a..=b).map(|f| f as f64).collect();
        let (mut rots, roots) = self.clip.sample_with(&frames, &self.links)?;
        let p: Vec<Vec<Vec3>> = rots
            .iter()
            .zip(&roots)
            .map(|(r, o)| sk.fk(r, *o).map(|x| x.1))
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        let idx = |n: &str| sk.bone(n).ok_or_else(|| format!("no bone {n}"));
        let (first, last) = (p.first().ok_or("no frames")?, p.last().ok_or("no frames")?);
        let mut floor = f64::INFINITY;
        for s in ["L", "R"] {
            for n in ["foot", "toe3-1"] {
                let i = idx(&format!("{n}.{s}"))?;
                for fr in [first, last] {
                    floor = floor.min(fr.get(i).map_or(f64::INFINITY, |j| j[2]));
                }
            }
        }
        let mut changed = BTreeSet::new();
        for s in ["L", "R"] {
            let (i, t) = (idx(&format!("foot.{s}"))?, idx(&format!("toe3-1.{s}"))?);
            let (pa, pb) = (*first.get(i).ok_or("joint")?, *last.get(i).ok_or("joint")?);
            let low = |fr: &Vec<Vec3>| {
                fr.get(t)
                    .map_or(0.0, |x| x[2])
                    .min(fr.get(i).map_or(0.0, |x| x[2]))
            };
            let on_floor = low(first) - floor < ground && low(last) - floor < ground;
            if quat::len3(quat::sub3(pa, pb)) > still || !on_floor {
                continue;
            }
            let n = frames.len();
            for k in 1..n.saturating_sub(1) {
                let w = k as f64 / (n - 1) as f64;
                let at = quat::add3(pa, quat::scale3(quat::sub3(pb, pa), w));
                let (Some(r), Some(o)) = (rots.get(k), roots.get(k)) else {
                    continue;
                };
                let leg = format!("leg.{s}");
                let pole = ik::own_pole(&sk, r, *o, &leg).map_err(|e| e.to_string())?;
                let solved = ik::two_bone(&sk, r, *o, &leg, at, pole, None).map_err(|e| e.to_string())?;
                if let Some(slot) = rots.get_mut(k) {
                    *slot = solved;
                }
                changed.insert(k);
            }
        }
        self.write_solved(&changed, a, &rots, &roots, "foot-plant")?;
        self.go_to(self.frame)?;
        Ok(changed.len())
    }

    /// Plant the steps a model says happen inside a filled span: `contacts` holds, per frame
    /// `a+1 .. b-1`, whether the left and the right foot is down. On each run of two or more frames
    /// a foot is down, its ankle stays where it is on the run's middle frame (leg IK), so a step
    /// inside the gap doesn't slide. A run whose foot is more than `ground` (m) above the keys'
    /// lowest foot is left alone (the model was wrong about it being down). Returns the frames
    /// changed.
    pub fn plant_steps(&mut self, a: i64, b: i64, contacts: &[[bool; 2]], ground: f64) -> R<usize> {
        let additive = self
            .clip
            .layers
            .get(self.clip.edit)
            .is_some_and(|l| l.mode == LayerMode::Additive);
        if b - a < 2 || additive {
            return Ok(0);
        }
        if contacts.len() as i64 != b - a - 1 {
            return Err(format!(
                "foot contacts for {} frames, the span {a}-{b} has {}",
                contacts.len(),
                b - a - 1
            ));
        }
        let sk = self.skeleton.clone();
        let frames: Vec<f64> = (a..=b).map(|f| f as f64).collect();
        let (mut rots, roots) = self.clip.sample_with(&frames, &self.links)?;
        let p: Vec<Vec<Vec3>> = rots
            .iter()
            .zip(&roots)
            .map(|(r, o)| sk.fk(r, *o).map(|x| x.1))
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        let idx = |n: &str| sk.bone(n).ok_or_else(|| format!("no bone {n}"));
        let (first, last) = (p.first().ok_or("no frames")?, p.last().ok_or("no frames")?);
        let mut floor = f64::INFINITY;
        for s in ["L", "R"] {
            for n in ["foot", "toe3-1"] {
                let i = idx(&format!("{n}.{s}"))?;
                for fr in [first, last] {
                    floor = floor.min(fr.get(i).map_or(f64::INFINITY, |j| j[2]));
                }
            }
        }
        let mut changed = BTreeSet::new();
        for (side, s) in ["L", "R"].into_iter().enumerate() {
            let (i, t) = (idx(&format!("foot.{s}"))?, idx(&format!("toe3-1.{s}"))?);
            let down: Vec<bool> = contacts.iter().map(|c| c[side]).collect();
            let mut k = 0;
            while k < down.len() {
                if !down[k] {
                    k += 1;
                    continue;
                }
                let start = k;
                while k < down.len() && down[k] {
                    k += 1;
                }
                if k - start < 2 {
                    continue;
                }
                // frame `n` of the span is p[n + 1] (p[0] is key a)
                let mid = p.get((start + k - 1) / 2 + 1).ok_or("frame")?;
                let at = *mid.get(i).ok_or("joint")?;
                let low = mid.get(t).map_or(at[2], |x| x[2]).min(at[2]);
                if low - floor > ground {
                    continue;
                }
                for n in start + 1..k + 1 {
                    let (Some(r), Some(o)) = (rots.get(n), roots.get(n)) else {
                        continue;
                    };
                    let leg = format!("leg.{s}");
                    let pole = ik::own_pole(&sk, r, *o, &leg).map_err(|e| e.to_string())?;
                    let solved = ik::two_bone(&sk, r, *o, &leg, at, pole, None).map_err(|e| e.to_string())?;
                    if let Some(slot) = rots.get_mut(n) {
                        *slot = solved;
                    }
                    changed.insert(n);
                }
            }
        }
        self.write_solved(&changed, a, &rots, &roots, "foot-plant")?;
        self.go_to(self.frame)?;
        Ok(changed.len())
    }

    /// Hold each foot in contact on the frames `a..=b` (the test `clip.metrics` measures slide
    /// with, [`crate::contact`]) so the point it stands on stays put: the heel as it lands, the
    /// ball as it rolls off. [`Document::plant_steps`] and [`Document::plant_feet`] hold the ankle,
    /// and a foot rolling over a held ankle slides its toes. Your keys and the anchors stay as they
    /// are, and only the legs of the fill's own frames change. A held foot stays slow for longer and
    /// counts as in contact for longer, so this runs again until nothing moves
    /// ([`HOLD_PASSES`] at most). Returns the frames changed.
    pub(crate) fn hold_contacts(&mut self, a: i64, b: i64) -> R<usize> {
        let additive = self
            .clip
            .layers
            .get(self.clip.edit)
            .is_some_and(|l| l.mode == LayerMode::Additive);
        if b - a < 2 || additive {
            return Ok(0);
        }
        let mut all = BTreeSet::new();
        for _ in 0..HOLD_PASSES {
            let changed = self.hold_pass(a, b)?;
            if changed.is_empty() {
                break;
            }
            all.extend(changed);
        }
        self.go_to(self.frame)?;
        Ok(all.len())
    }

    /// One pass of [`Document::hold_contacts`]: the frames it changed (from `a`).
    fn hold_pass(&mut self, a: i64, b: i64) -> R<BTreeSet<usize>> {
        let sk = self.skeleton.clone();
        let frames: Vec<f64> = (a..=b).map(|f| f as f64).collect();
        let (mut rots, roots) = self.clip.sample_with(&frames, &self.links)?;
        let body = crate::physics::Body::new(&sk)?;
        let phases =
            crate::contact::clip_phases(&body, &sk, &rots, &roots, self.clip.fps, &self.clip.surfaces)?;
        let mut posed = Vec::with_capacity(rots.len());
        for (r, o) in rots.iter().zip(&roots) {
            let (w, p) = sk.fk(r, *o).map_err(|e| e.to_string())?;
            posed.push((body.points(&w, &p), p));
        }
        let fixed: Vec<bool> = (a..=b)
            .map(|f| {
                !self
                    .clip
                    .keys()
                    .iter()
                    .find(|k| k.frame == f)
                    .is_some_and(crate::ease::eased)
            })
            .collect();
        let mut changed = BTreeSet::new();
        for (foot, s) in ["L", "R"].into_iter().enumerate() {
            let ankle = sk.bone(&format!("foot.{s}")).ok_or("no foot")?;
            let flags = phases.get(foot).ok_or("no phases")?;
            let down: Vec<bool> = flags.iter().map(|p| p.down()).collect();
            let mut drift = vec![[0.0; 3]; posed.len()];
            for i in 1..posed.len() {
                let (Some(now), Some(before)) = (posed.get(i), posed.get(i - 1)) else {
                    continue;
                };
                // the foot's lowest point now, and how far it came across the floor
                let low = now
                    .0
                    .iter()
                    .enumerate()
                    .filter(|(k, _)| body.foot_of(*k) == foot)
                    .map(|(k, p)| (k, p[2] - crate::physics::ground(&self.clip.surfaces, p[0], p[1])))
                    .min_by(|x, y| x.1.total_cmp(&y.1))
                    .map(|(k, _)| k);
                let step = low
                    .and_then(|k| Some(quat::sub3(*now.0.get(k)?, *before.0.get(k)?)))
                    .unwrap_or([0.0; 3]);
                drift[i] = quat::add3(drift[i - 1], [step[0], step[1], 0.0]);
            }
            let leg = format!("leg.{s}");
            for (i, by) in hold_moves(&drift, &down, &fixed).into_iter().enumerate() {
                if by.iter().all(|x| x.abs() < 1e-6) {
                    continue;
                }
                let (Some(r), Some(o), Some(at)) = (
                    rots.get(i),
                    roots.get(i),
                    posed.get(i).and_then(|x| x.1.get(ankle)),
                ) else {
                    continue;
                };
                let pole = ik::own_pole(&sk, r, *o, &leg).map_err(|e| e.to_string())?;
                let solved = ik::two_bone(&sk, r, *o, &leg, quat::add3(*at, by), pole, None)
                    .map_err(|e| e.to_string())?;
                if let Some(slot) = rots.get_mut(i) {
                    *slot = solved;
                }
                changed.insert(i);
            }
        }
        self.write_solved(&changed, a, &rots, &roots, "foot-plant")?;
        Ok(changed)
    }

    pub(crate) fn write_solved(
        &mut self,
        changed: &BTreeSet<usize>,
        first: i64,
        rots: &[Vec<Quat>],
        roots: &[Vec3],
        tag: &str,
    ) -> R<()> {
        for &n in changed {
            let f = first + n as i64;
            let (Some(r), Some(o)) = (rots.get(n), roots.get(n)) else {
                continue;
            };
            let r: Vec<Quat> = r.iter().map(|q| quat::normalize(*q)).collect();
            if let Some(k) = keys_mut(&mut self.clip)?.iter_mut().find(|k| k.frame == f) {
                k.rotations = r;
            } else {
                let mut k = Key::new(f, r, *o, Interp::Linear);
                k.source = KeySource::Ai;
                k.model = tag.to_string();
                self.clip.set_key(k)?;
            }
        }
        Ok(())
    }

    /// As [`Document::write_solved`], where the root moved too.
    pub(crate) fn write_moved(
        &mut self,
        changed: &BTreeSet<usize>,
        rots: &[Vec<Quat>],
        roots: &[Vec3],
        tag: &str,
    ) -> R<()> {
        self.write_solved(changed, 0, rots, roots, tag)?;
        for k in keys_mut(&mut self.clip)?.iter_mut() {
            let n = k.frame.max(0) as usize;
            if changed.contains(&n)
                && let Some(o) = roots.get(n)
            {
                k.root = *o;
            }
        }
        Ok(())
    }

    /// Pin each planted foot to one spot for its whole contact, easing in and out of it over
    /// [`LOCK_FADE`] frames either side; returns frames changed.
    pub fn lock_feet(&mut self) -> R<usize> {
        if self.clip.base().keys.is_empty() {
            return Ok(0);
        }
        let sk = self.skeleton.clone();
        let frames: Vec<f64> = (0..self.clip.length()).map(|f| f as f64).collect();
        let (mut rots, roots) = self.clip.sample_with(&frames, &self.links)?;
        // landing and release ease, as the frames either side do
        let body = crate::physics::Body::new(&sk)?;
        let [left, right] =
            crate::contact::clip_phases(&body, &sk, &rots, &roots, self.clip.fps, &self.clip.surfaces)?;
        let flag = |v: Vec<crate::contact::Phase>| {
            v.into_iter()
                .map(|p| p == crate::contact::Phase::Planted)
                .collect::<Vec<bool>>()
        };
        let c = [("L", flag(left)), ("R", flag(right))];
        let mut changed = BTreeSet::new();
        for (s, flags) in &c {
            let fi = sk.bone(&format!("foot.{s}")).ok_or("no foot")?;
            let ti = sk.bone(&format!("toe3-1.{s}")).ok_or("no toe")?;
            let (mut ankles, mut balls) = (Vec::with_capacity(flags.len()), Vec::with_capacity(flags.len()));
            for (r, o) in rots.iter().zip(&roots).take(flags.len()) {
                let (_, p) = sk.fk(r, *o).map_err(|e| e.to_string())?;
                ankles.push(*p.get(fi).ok_or("joint")?);
                balls.push(*p.get(ti).ok_or("joint")?);
            }
            let leg = format!("leg.{s}");
            for (i, by) in lock_moves(&balls, flags).into_iter().enumerate() {
                let (Some(r), Some(o), Some(a)) = (rots.get(i), roots.get(i), ankles.get(i)) else {
                    continue;
                };
                if by.iter().all(|x| x.abs() < 1e-6) {
                    continue;
                }
                let pole = ik::own_pole(&sk, r, *o, &leg).map_err(|e| e.to_string())?;
                let solved = ik::two_bone(&sk, r, *o, &leg, quat::add3(*a, by), pole, None)
                    .map_err(|e| e.to_string())?;
                if let Some(slot) = rots.get_mut(i) {
                    *slot = solved;
                }
                changed.insert(i);
            }
        }
        self.write_solved(&changed, 0, &rots, &roots, "foot-lock")?;
        self.go_to(self.frame)?;
        let m = manual_frames(&self.clip);
        let mut planted = 0;
        for w in m.windows(2) {
            if let [a, b] = w {
                planted += self.plant_feet(*a, *b, 0.25, 0.15)?;
            }
        }
        Ok(changed.len() + planted)
    }

    /// Change one channel (0-2: angle about X/Y/Z in degrees; 3-5: root position) of `bone` in the
    /// edited layer's key on `frame`, and move that joint's key to `to`.
    pub fn set_channel(&mut self, bone: usize, frame: i64, to: i64, axis: usize, value: f64) -> R<()> {
        let key = self
            .clip
            .keys()
            .iter()
            .find(|k| k.frame == frame && k.source == KeySource::Manual && k.sets(bone))
            .cloned()
            .ok_or_else(|| format!("no key of yours on frame {frame} sets that joint"))?;
        let mut rot = *key.rotations.get(bone).ok_or("no such bone")?;
        let mut root = key.root;
        if axis < 3 {
            let mut e = quat::to_euler(rot);
            e[axis] = value;
            rot = quat::normalize(quat::from_euler(e));
        } else if bone == 0 && axis < 6 {
            root[axis - 3] = value;
        } else {
            return Err("only the hips have a position".into());
        }
        let to = to.max(0);
        if to == frame {
            let k = keys_mut(&mut self.clip)?
                .iter_mut()
                .find(|k| k.frame == frame)
                .ok_or("key vanished")?;
            if let Some(slot) = k.rotations.get_mut(bone) {
                *slot = rot;
            }
            if bone == 0 {
                k.root = root;
            }
        } else {
            self.remove_parts(frame, Some(vec![bone]))?;
            let dst = keys_mut(&mut self.clip)?
                .iter_mut()
                .find(|k| k.frame == to && k.source == KeySource::Manual);
            if let Some(dst) = dst {
                if let Some(slot) = dst.rotations.get_mut(bone) {
                    *slot = rot;
                }
                if bone == 0 {
                    dst.root = root;
                }
                if let Some(o) = &mut dst.only
                    && !o.contains(&bone)
                {
                    o.push(bone);
                    o.sort_unstable();
                }
            } else {
                let mut p = sample1(&self.clip, to as f64, &self.links)?;
                if let Some(slot) = p.rotations.get_mut(bone) {
                    *slot = rot;
                }
                let r0 = if bone == 0 { root } else { p.root };
                self.clip
                    .set_key(Key::new(to, p.rotations, r0, Interp::Smooth).with_only(Some(vec![bone])))?;
            }
        }
        self.go_to(self.frame)
    }
}

/// Flatten layers `1..=top` into the base keys; returns the keys written.
pub fn bake_down(clip: &mut Clip, top: usize, every: i64, links: &dyn Links) -> R<usize> {
    if !(1..clip.layers.len()).contains(&top) {
        return Err("bake a layer above the base".into());
    }
    let n = clip.bones.len();
    let mut bones = BTreeSet::new();
    for lay in clip.layers.iter().skip(1).take(top) {
        if !lay.muted {
            bones.extend(lay.only.clone().unwrap_or_else(|| (0..n).collect()));
        }
    }
    let end = clip.length();
    let mut frames: BTreeSet<i64> = (0..end).step_by(every.max(1) as usize).collect();
    frames.insert(end - 1);
    let fl: Vec<f64> = frames.iter().map(|&f| f as f64).collect();
    let (rots, roots) = clip.sample_below(&fl, top + 1, links)?;
    let only = if bones.len() == n {
        None
    } else {
        Some(bones.iter().copied().collect::<Vec<_>>())
    };
    let mut base = Clip::new(clip.bones.clone(), clip.fps);
    base.loop_ = clip.loop_;
    if let (Some(b), Some(src)) = (base.layers.first_mut(), clip.layers.first()) {
        b.keys = src.keys.clone();
    }
    for ((f, r), o) in frames.iter().zip(rots).zip(roots) {
        let old = base.keys().iter().find(|k| k.frame == *f).cloned();
        match (old, &only) {
            (Some(old), Some(only)) => {
                let mut r2 = old.rotations.clone();
                for &b in only {
                    if let (Some(slot), Some(q)) = (r2.get_mut(b), r.get(b)) {
                        *slot = *q;
                    }
                }
                let o2 = if only.contains(&0) { o } else { old.root };
                let keep = old
                    .only
                    .map(|x| x.into_iter().chain(only.iter().copied()).collect());
                base.set_key(Key::new(*f, r2, o2, old.interp).with_only(keep))?;
            }
            _ if !bones.is_empty() => {
                base.set_key(Key::new(*f, r, o, Interp::Smooth).with_only(only.clone()))?
            }
            _ => {}
        }
    }
    let new_base = base.layers.into_iter().next().map(|l| l.keys).unwrap_or_default();
    if let Some(b) = clip.layers.first_mut() {
        b.keys = new_base;
    }
    clip.layers.drain(1..=top);
    clip.edit = clip.edit.saturating_sub(top);
    Ok(frames.len())
}

/// A pose mirrored left <-> right: rotations relative to bones resting along the world axes, so a
/// mirror across X maps (x, y, z, w) to (x, -y, -z, w) on the partner bone.
pub fn mirrored(sk: &Skeleton, pose: &Pose) -> R<Pose> {
    let mut out = Vec::with_capacity(sk.len());
    for n in &sk.names {
        let p = sk
            .bone(&mirror_name(n))
            .ok_or_else(|| format!("{n} has no partner bone"))?;
        let q = pose.rotations.get(p).ok_or("pose too short")?;
        out.push([q[0], -q[1], -q[2], q[3]]);
    }
    Ok(Pose {
        rotations: out,
        root: [-pose.root[0], pose.root[1], pose.root[2]],
    })
}

/// The partner bones of a bone set.
pub fn mirror_bones(sk: &Skeleton, only: Option<&[usize]>) -> Option<Vec<usize>> {
    let only = only?;
    let mut out: Vec<usize> = only
        .iter()
        .filter_map(|&i| sk.names.get(i))
        .filter_map(|n| sk.bone(&mirror_name(n)))
        .collect();
    out.sort_unstable();
    Some(out)
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn transpose(m: &Mat3) -> Mat3 {
    [
        [m[0][0], m[1][0], m[2][0]],
        [m[0][1], m[1][1], m[2][1]],
        [m[0][2], m[1][2], m[2][2]],
    ]
}

/// `rots` with bone `i` turned by `angle` about a world `axis` (its children follow).
fn turn(sk: &Skeleton, rots: &[Quat], root: Vec3, i: usize, axis: Vec3, angle: f64) -> R<Vec<Quat>> {
    let (r, _) = sk.fk(rots, root).map_err(|e| e.to_string())?;
    let rp = match sk.parents.get(i).copied().flatten() {
        Some(p) => *r.get(p).ok_or("bone")?,
        None => quat::MAT3_IDENTITY,
    };
    let ri = *r.get(i).ok_or("bone")?;
    let local = quat::mat3_mul(
        &quat::mat3_mul(&transpose(&rp), &ik::axis_angle(axis, angle)),
        &ri,
    );
    let mut out = rots.to_vec();
    if let Some(slot) = out.get_mut(i) {
        *slot = quat::from_mat3(&local);
    }
    Ok(out)
}

/// `rots` with both feet back where (and as) they were in the pose `before`.
fn keep_feet(sk: &Skeleton, mut rots: Vec<Quat>, root: Vec3, before: &[Quat]) -> R<Vec<Quat>> {
    let (r0, p0) = sk.fk(before, root).map_err(|e| e.to_string())?;
    for s in ["L", "R"] {
        let i = sk.bone(&format!("foot.{s}")).ok_or("no foot")?;
        rots = ik::two_bone(
            sk,
            &rots,
            root,
            &format!("leg.{s}"),
            *p0.get(i).ok_or("joint")?,
            None,
            None,
        )
        .map_err(|e| e.to_string())?;
        let (r, _) = sk.fk(&rots, root).map_err(|e| e.to_string())?;
        let p = sk.parents.get(i).copied().flatten().ok_or("foot has no parent")?;
        if let (Some(slot), Some(rp), Some(ri)) = (rots.get_mut(i), r.get(p), r0.get(i)) {
            *slot = quat::from_mat3(&quat::mat3_mul(&transpose(rp), ri));
        }
    }
    Ok(rots)
}

fn bend_hips(sk: &Skeleton, rots: &[Quat], root: Vec3, axis: Vec3, angle: f64, share: f64) -> R<Vec<Quat>> {
    let mut out = turn(sk, rots, root, 0, axis, angle * (1.0 - share))?;
    for b in SPINE {
        let i = sk.bone(b).ok_or_else(|| format!("no bone {b}"))?;
        out = turn(sk, &out, root, i, axis, angle * share / SPINE.len() as f64)?;
    }
    keep_feet(sk, out, root, rots)
}

/// One joint's channels for the graph: per frame its rotation as unwrapped Euler degrees (the root
/// also its position), and the manual keys of the edited layer that set it.
pub fn curves(clip: &Clip, bone: usize, links: &dyn Links) -> R<serde_json::Value> {
    if clip.base().keys.is_empty() {
        return Ok(serde_json::json!({ "frames": [], "keys": [] }));
    }
    let frames: Vec<f64> = (0..clip.length()).map(|f| f as f64).collect();
    let (rots, roots) = clip.sample_with(&frames, links)?;
    let mut e: Vec<Vec3> = rots
        .iter()
        .map(|fr| quat::to_euler(fr.get(bone).copied().unwrap_or(quat::IDENTITY)))
        .collect();
    // unwrap: no jumps at +-180
    let mut prev: Option<Vec3> = None;
    for cur in &mut e {
        if let Some(p) = prev {
            for (c, pc) in cur.iter_mut().zip(p) {
                *c += 360.0 * ((pc - *c) / 360.0).round();
            }
        }
        prev = Some(*cur);
    }
    let vals: Vec<Vec<f64>> = e
        .iter()
        .zip(&roots)
        .map(|(a, o)| {
            if bone == 0 {
                a.iter().chain(o).copied().collect()
            } else {
                a.to_vec()
            }
        })
        .collect();
    let mut keys = Vec::new();
    for k in clip.keys() {
        if k.source != KeySource::Manual || !k.sets(bone) {
            continue;
        }
        let mut ke = quat::to_euler(k.rotations.get(bone).copied().unwrap_or(quat::IDENTITY));
        let f = (k.frame.max(0) as usize).min(e.len().saturating_sub(1));
        if let Some(ef) = e.get(f) {
            for c in 0..3 {
                ke[c] += 360.0 * ((ef[c] - ke[c]) / 360.0).round();
            }
        }
        let values: Vec<f64> = if bone == 0 {
            ke.iter().chain(&k.root).copied().collect()
        } else {
            ke.to_vec()
        };
        keys.push(serde_json::json!({ "frame": k.frame, "values": values }));
    }
    Ok(serde_json::json!({ "frames": vals, "keys": keys }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The point drifts 1 cm a frame along x on 7 frames; the foot is down on 1..=3.
    fn drift() -> Vec<Vec3> {
        (0..7).map(|i| [0.01 * i as f64, 0.0, 0.0]).collect()
    }

    fn down() -> Vec<bool> {
        (0..7).map(|i| (1..=3).contains(&i)).collect()
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-12
    }

    /// Mean of 0.01, 0.02, 0.03 is 0.02: moves +1, 0, -1 cm. One frame beyond the contact fades
    /// by smoothstep(1 - 1/5) = 0.896.
    #[test]
    fn a_free_contact_holds_at_its_mean_place() {
        let by = hold_moves(&drift(), &down(), &[false; 7]);
        assert!(near(by[1][0], 0.01) && near(by[2][0], 0.0) && near(by[3][0], -0.01));
        assert!(near(by[0][0], 0.01 * 0.896) && near(by[4][0], -0.01 * 0.896));
    }

    /// A key on frame 1 stays: the contact holds where the key has it, 0, -1, -2 cm.
    #[test]
    fn a_contact_over_a_key_holds_at_the_key() {
        let fixed: Vec<bool> = (0..7).map(|i| i == 1).collect();
        let by = hold_moves(&drift(), &down(), &fixed);
        assert!(near(by[1][0], 0.0) && near(by[2][0], -0.01) && near(by[3][0], -0.02));
    }

    /// Keys on 1 and 3 can't both be left: the foot goes straight from one to the other, and a
    /// fixed frame beside the contact doesn't take its fade.
    #[test]
    fn a_contact_between_two_keys_goes_straight_and_fixed_frames_stay() {
        let fixed: Vec<bool> = (0..7).map(|i| i == 1 || i == 3 || i == 4).collect();
        let by = hold_moves(&drift(), &down(), &fixed);
        assert!(by.iter().all(|m| near(m[0], 0.0)), "{by:?}");
    }

    /// One frame down is no contact, and the move never leaves the floor's plane.
    #[test]
    fn one_frame_is_not_a_contact_and_height_stays() {
        let one: Vec<bool> = (0..7).map(|i| i == 2).collect();
        assert!(
            hold_moves(&drift(), &one, &[false; 7])
                .iter()
                .all(|m| *m == [0.0; 3])
        );
        let high: Vec<Vec3> = (0..7).map(|i| [0.0, 0.0, 0.01 * i as f64]).collect();
        assert!(
            hold_moves(&high, &down(), &[false; 7])
                .iter()
                .all(|m| m[2] == 0.0)
        );
    }
}
