//! Keyframed clips on a named skeleton, sampled per frame (a port of cf-animate's `clip.py`).
//!
//! A key holds one rotation per bone (relative to the parent) and a root offset in hip heights.
//! Interpolation belongs to the segment that starts at a key: `step` holds, `linear` slerps and
//! lerps, `smooth` runs squad on rotations and a cubic on the root with clamped (monotone) tangents,
//! flat where the keys hold or turn back, so a curve never swings past its keys, and the eases
//! reshape the smooth curve's timing. A partial key (`only`) sets just those bones; every bone interpolates
//! between the keys that set it.
//!
//! Layers stack on the base keys, bottom to top: their own keys or a link to another clip,
//! driving some bones (`only`), replacing them (`override`) or turning them further (`additive`),
//! blended by `weight` and an optional ranged fade.

use std::collections::BTreeMap;

use crate::quat::{self, Quat, Vec3};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interp {
    Step,
    Linear,
    Smooth,
    EaseIn,
    EaseOut,
    Ease,
}

impl Interp {
    pub const NAMES: &'static [&'static str] = &["step", "linear", "smooth", "ease_in", "ease_out", "ease"];

    pub fn parse(s: &str) -> Option<Interp> {
        Some(match s {
            "step" => Interp::Step,
            "linear" => Interp::Linear,
            "smooth" => Interp::Smooth,
            "ease_in" => Interp::EaseIn,
            "ease_out" => Interp::EaseOut,
            "ease" => Interp::Ease,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Interp::Step => "step",
            Interp::Linear => "linear",
            Interp::Smooth => "smooth",
            Interp::EaseIn => "ease_in",
            Interp::EaseOut => "ease_out",
            Interp::Ease => "ease",
        }
    }

    fn ease(self, t: f64) -> f64 {
        match self {
            Interp::EaseIn => t * t,
            Interp::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            Interp::Ease => t * t * (3.0 - 2.0 * t),
            _ => t,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    Manual,
    Ai,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Key {
    pub frame: i64,
    pub rotations: Vec<Quat>,
    pub root: Vec3,
    pub interp: Interp,
    pub source: KeySource,
    /// The in-betweener that made an AI key.
    pub model: String,
    /// The bones this key sets (sorted, unique); `None`: all of them.
    pub only: Option<Vec<usize>>,
}

impl Key {
    /// A key with its rotations normalised where they are off (unit ones keep their bits, so a
    /// load -> save round trip is exact) and `only` sorted; `only` naming every bone becomes `None`.
    pub fn new(frame: i64, rotations: Vec<Quat>, root: Vec3, interp: Interp) -> Key {
        let rotations = rotations
            .into_iter()
            .map(|q| {
                if (quat::norm(q) - 1.0).abs() > 1e-12 {
                    quat::normalize(q)
                } else {
                    q
                }
            })
            .collect();
        Key {
            frame,
            rotations,
            root,
            interp,
            source: KeySource::Manual,
            model: String::new(),
            only: None,
        }
    }

    pub fn with_only(mut self, only: Option<Vec<usize>>) -> Key {
        self.only = only.map(|mut o| {
            o.sort_unstable();
            o.dedup();
            o
        });
        if self
            .only
            .as_ref()
            .is_some_and(|o| o.len() == self.rotations.len())
        {
            self.only = None;
        }
        self
    }

    pub fn sets(&self, bone: usize) -> bool {
        self.only.as_ref().is_none_or(|o| o.binary_search(&bone).is_ok())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerSource {
    Keys,
    Link,
    Travel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerMode {
    Override,
    Additive,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub name: String,
    pub source: LayerSource,
    pub keys: Vec<Key>,
    /// The linked clip's id (`<group>/<name>`).
    pub link: String,
    pub only: Option<Vec<usize>>,
    pub parts: Vec<String>,
    pub mode: LayerMode,
    pub weight: f64,
    pub offset: f64,
    pub speed: f64,
    pub loop_: bool,
    pub muted: bool,
    pub start: Option<f64>,
    pub end: Option<f64>,
    pub fade: i64,
    /// Travel layers: the gait, the path, hand-moved footsteps and the planned steps.
    pub gait: String,
    pub waypoints: Vec<[f64; 2]>,
    pub moves: BTreeMap<String, [f64; 2]>,
    pub steps: Vec<serde_json::Value>,
}

impl Layer {
    pub fn keys(name: &str, keys: Vec<Key>) -> Layer {
        Layer {
            name: name.to_string(),
            source: LayerSource::Keys,
            keys,
            link: String::new(),
            only: None,
            parts: Vec::new(),
            mode: LayerMode::Override,
            weight: 1.0,
            offset: 0.0,
            speed: 1.0,
            loop_: true,
            muted: false,
            start: None,
            end: None,
            fade: 0,
            gait: "auto".into(),
            waypoints: Vec::new(),
            moves: BTreeMap::new(),
            steps: Vec::new(),
        }
    }

    /// The layer's weight on a frame (its range and fades).
    pub fn ramp(&self, f: f64) -> f64 {
        let (Some(start), Some(end)) = (self.start, self.end) else {
            return self.weight;
        };
        let inside = f >= start && f <= end;
        if self.fade > 0 {
            let edge = ((f - start).min(end - f) / self.fade as f64).clamp(0.0, 1.0);
            return self.weight * if inside { edge } else { 0.0 };
        }
        if inside { self.weight } else { 0.0 }
    }
}

/// A box in the scene the body stands on, leans on or jumps over (metres, rig world).
#[derive(Debug, Clone, PartialEq)]
pub struct Surface {
    pub name: String,
    /// Its corners across the floor before it turns.
    pub min: [f64; 2],
    pub max: [f64; 2],
    pub top: f64,
    /// How far it is turned about its centre (radians, counter-clockwise seen from above; 0 in
    /// files that don't say).
    pub yaw: f64,
}

impl Surface {
    /// The middle of its footprint.
    pub fn centre(&self) -> [f64; 2] {
        [
            (self.min[0] + self.max[0]) / 2.0,
            (self.min[1] + self.max[1]) / 2.0,
        ]
    }

    /// Whether the floor point (x, y) is under its top (on the edge counts).
    pub fn contains(&self, x: f64, y: f64) -> bool {
        if self.yaw == 0.0 {
            return x >= self.min[0] && x <= self.max[0] && y >= self.min[1] && y <= self.max[1];
        }
        let c = self.centre();
        let (s, k) = self.yaw.sin_cos();
        // into the box's own frame: turn back by its yaw
        let (dx, dy) = (x - c[0], y - c[1]);
        let (lx, ly) = (k * dx + s * dy, -s * dx + k * dy);
        let (hx, hy) = (
            (self.max[0] - self.min[0]) / 2.0,
            (self.max[1] - self.min[1]) / 2.0,
        );
        lx.abs() <= hx && ly.abs() <= hy
    }

    /// Its footprint's four corners on the floor, turned, in order around it.
    pub fn corners(&self) -> [[f64; 2]; 4] {
        let c = self.centre();
        let (s, k) = self.yaw.sin_cos();
        let (hx, hy) = (
            (self.max[0] - self.min[0]) / 2.0,
            (self.max[1] - self.min[1]) / 2.0,
        );
        [(-hx, -hy), (hx, -hy), (hx, hy), (-hx, hy)]
            .map(|(x, y)| [c[0] + k * x - s * y, c[1] + s * x + k * y])
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub bones: Vec<String>,
    pub fps: f64,
    pub loop_: bool,
    pub name: String,
    pub group: String,
    /// Where a built-in came from; empty for the user's own.
    pub origin: String,
    /// `layers[0]` is the base.
    pub layers: Vec<Layer>,
    /// The layer the key tools work on.
    pub edit: usize,
    /// Play and export through the physics pass.
    pub physics: bool,
    pub surfaces: Vec<Surface>,
    /// Named markers on the timeline, by frame (`crate::markers`).
    pub markers: Vec<crate::markers::Marker>,
    /// Hands and feet held in place (`feet.plant`), for the physics pass.
    pub contacts: Vec<Contact>,
}

/// A hand or foot held at one point of the world through frames `from..=to`, at any height: on the
/// floor, or on the side of a box (`crate::physics`).
#[derive(Debug, Clone, PartialEq)]
pub struct Contact {
    /// `foot.L`, `foot.R`, `wrist.L` or `wrist.R`.
    pub bone: String,
    pub from: i64,
    pub to: i64,
    pub at: Vec3,
}

/// Rotations `[frame][bone]` and root offsets `[frame]`.
pub type Samples = (Vec<Vec<Quat>>, Vec<Vec3>);

/// Finds a linked clip by id (the library, for link layers).
pub trait Links {
    fn clip(&self, id: &str) -> Option<&Clip>;
}

/// No library: link layers drive nothing.
pub struct NoLinks;

impl Links for NoLinks {
    fn clip(&self, _: &str) -> Option<&Clip> {
        None
    }
}

pub type Resolve<'a> = &'a dyn Links;

impl Clip {
    pub fn new(bones: Vec<String>, fps: f64) -> Clip {
        Clip {
            bones,
            fps,
            loop_: false,
            name: String::new(),
            group: String::new(),
            origin: String::new(),
            layers: vec![Layer::keys("Base", Vec::new())],
            edit: 0,
            physics: true,
            surfaces: Vec::new(),
            markers: Vec::new(),
            contacts: Vec::new(),
        }
    }

    pub fn base(&self) -> &Layer {
        // a clip always has its base layer: every constructor and loader makes one
        &self.layers[0]
    }

    /// The keys of the layer being edited.
    pub fn keys(&self) -> &[Key] {
        self.layers.get(self.edit).map_or(&[], |l| &l.keys)
    }

    /// Insert a key into the edited layer, replacing any key on the same frame.
    pub fn set_key(&mut self, key: Key) -> Result<(), String> {
        if key.rotations.len() != self.bones.len() {
            return Err(format!(
                "key has {} bones, clip has {}",
                key.rotations.len(),
                self.bones.len()
            ));
        }
        let layer = self.layers.get_mut(self.edit).ok_or("no layer to edit")?;
        layer.keys.retain(|k| k.frame != key.frame);
        layer.keys.push(key);
        layer.keys.sort_by_key(|k| k.frame);
        Ok(())
    }

    pub fn remove_key(&mut self, frame: i64) -> Result<(), String> {
        let layer = self.layers.get_mut(self.edit).ok_or("no layer to edit")?;
        let before = layer.keys.len();
        layer.keys.retain(|k| k.frame != frame);
        if layer.keys.len() == before {
            return Err(format!("no key on frame {frame}"));
        }
        Ok(())
    }

    /// Frames covered: the last key of any layer + 1 (a link counts with one play-through).
    pub fn length_with(&self, resolve: Resolve) -> i64 {
        let mut last: Option<i64> = None;
        for lay in &self.layers {
            if lay.source != LayerSource::Link
                && let Some(k) = lay.keys.last()
            {
                last = Some(last.map_or(k.frame, |l| l.max(k.frame)));
            }
        }
        for lay in self.layers.iter().skip(1) {
            if lay.source == LayerSource::Link
                && !lay.link.is_empty()
                && let Some(src) = resolve.clip(&lay.link)
            {
                let n = src.length_with(&NoLinks);
                if n > 0 {
                    let f = (lay.offset + (n - 1) as f64 / lay.speed).ceil() as i64;
                    last = Some(last.map_or(f, |l| l.max(f)));
                }
            }
        }
        last.map_or(0, |l| l + 1)
    }

    pub fn length(&self) -> i64 {
        self.length_with(&NoLinks)
    }

    /// Every layer, bottom to top, at (possibly fractional) frames.
    pub fn sample(&self, frames: &[f64]) -> Result<Samples, String> {
        self.sample_below(frames, self.layers.len(), &NoLinks)
    }

    pub fn sample_with(&self, frames: &[f64], resolve: Resolve) -> Result<Samples, String> {
        self.sample_below(frames, self.layers.len(), resolve)
    }

    /// The layers under `top` only (`top` = 1: the base keys alone).
    pub fn sample_below(&self, frames: &[f64], top: usize, resolve: Resolve) -> Result<Samples, String> {
        if self.base().keys.is_empty() {
            return Err("clip has no keys".into());
        }
        let nb = self.bones.len();
        let (mut rots, mut roots) = sample_keys(&self.base().keys, nb, self.loop_, frames);
        for i in 1..top.min(self.layers.len()) {
            let lay = &self.layers[i];
            if lay.muted || lay.weight == 0.0 {
                continue;
            }
            let Some((lr, lo)) = self.layer_pose(lay, frames, i, resolve)? else {
                continue;
            };
            let bones: Vec<usize> = lay.only.clone().unwrap_or_else(|| (0..nb).collect());
            let w: Vec<f64> = frames.iter().map(|&f| lay.ramp(f)).collect();
            if w.iter().all(|&x| x == 0.0) {
                continue;
            }
            let full = w.iter().all(|&x| x == 1.0);
            for (fi, wf) in w.iter().enumerate() {
                for &b in &bones {
                    let (Some(below), Some(&layer_q)) = (rots[fi].get(b).copied(), lr[fi].get(b)) else {
                        continue;
                    };
                    let q = match lay.mode {
                        LayerMode::Override if full => layer_q,
                        LayerMode::Override => quat::slerp(below, layer_q, *wf),
                        LayerMode::Additive => {
                            let d = if full {
                                layer_q
                            } else {
                                quat::slerp(quat::IDENTITY, layer_q, *wf)
                            };
                            quat::normalize(quat::mul(below, d))
                        }
                    };
                    rots[fi][b] = q;
                }
                if bones.contains(&0) {
                    let o = lo[fi];
                    roots[fi] = match lay.mode {
                        LayerMode::Override if full => o,
                        LayerMode::Override => {
                            quat::add3(roots[fi], quat::scale3(quat::sub3(o, roots[fi]), *wf))
                        }
                        LayerMode::Additive => quat::add3(roots[fi], quat::scale3(o, *wf)),
                    };
                }
            }
        }
        Ok((rots, roots))
    }

    /// A layer's own pose (an additive layer: its turn); `None` when it has nothing to give.
    fn layer_pose(
        &self,
        lay: &Layer,
        frames: &[f64],
        index: usize,
        resolve: Resolve,
    ) -> Result<Option<Samples>, String> {
        let nb = self.bones.len();
        if lay.source != LayerSource::Link {
            if lay.keys.is_empty() {
                return Ok(None);
            }
            return Ok(Some(sample_keys(&lay.keys, nb, self.loop_, frames)));
        }
        let src = if lay.link.is_empty() {
            None
        } else {
            resolve.clip(&lay.link)
        };
        let Some(src) = src.filter(|s| !s.base().keys.is_empty() && s.bones == self.bones) else {
            return Ok(None); // a missing link drives nothing
        };
        let n = src.length() as f64;
        let period = if src.loop_ { n } else { (n - 1.0).max(1.0) };
        let mut cycles = Vec::with_capacity(frames.len());
        let ts: Vec<f64> = frames
            .iter()
            .map(|&f| {
                let t = (f - lay.offset) * lay.speed;
                if lay.loop_ {
                    let c = (t / period).floor();
                    cycles.push(c);
                    t - c * period
                } else {
                    cycles.push(0.0);
                    t.clamp(0.0, n - 1.0)
                }
            })
            .collect();
        let (r, mut o) = src.sample(&ts)?;
        let (r0, o0) = src.sample(&[0.0])?;
        let (q0, root0) = (
            r0.into_iter().next().unwrap_or_default(),
            o0.first().copied().unwrap_or_default(),
        );
        let end_at = if src.loop_ { period } else { n - 1.0 };
        let end = src.sample(&[end_at])?.1.first().copied().unwrap_or_default();
        let step = quat::sub3(end, root0);
        for (oi, c) in o.iter_mut().zip(&cycles) {
            oi[0] += c * step[0];
            oi[1] += c * step[1];
        }
        if lay.mode == LayerMode::Additive {
            let r = r
                .into_iter()
                .map(|fr| {
                    fr.iter()
                        .zip(&q0)
                        .map(|(q, a)| quat::normalize(quat::mul(quat::conj(*a), *q)))
                        .collect()
                })
                .collect();
            let o = o.into_iter().map(|x| quat::sub3(x, root0)).collect();
            return Ok(Some((r, o)));
        }
        // the link starts where the character stands at the layer's first frame, at its height
        let below = self
            .sample_below(&[lay.offset], index, resolve)?
            .1
            .first()
            .copied()
            .unwrap_or_default();
        for oi in &mut o {
            oi[0] += below[0] - root0[0];
            oi[1] += below[1] - root0[1];
            oi[2] += below[2] - root0[2];
        }
        Ok(Some((r, o)))
    }
}

/// Keys alone (partial keys per bone channel).
pub fn sample_keys(keys: &[Key], bones: usize, looping: bool, frames: &[f64]) -> Samples {
    if keys.iter().all(|k| k.only.is_none()) {
        return sample_plain(keys, bones, looping, frames);
    }
    // bones grouped by the keys that set them; a bone no key sets follows every key
    let mut groups: BTreeMap<Vec<usize>, Vec<usize>> = BTreeMap::new();
    for b in 0..bones {
        let mut ks: Vec<usize> = keys
            .iter()
            .enumerate()
            .filter(|(_, k)| k.sets(b))
            .map(|(i, _)| i)
            .collect();
        if ks.is_empty() {
            ks = (0..keys.len()).collect();
        }
        groups.entry(ks).or_default().push(b);
    }
    let mut rots = vec![vec![quat::IDENTITY; bones]; frames.len()];
    let mut roots = vec![[0.0; 3]; frames.len()];
    for (ks, bs) in groups {
        let sub: Vec<Key> = ks.iter().filter_map(|&i| keys.get(i).cloned()).collect();
        let (r, o) = sample_plain(&sub, bones, looping, frames);
        for (fi, fr) in r.iter().enumerate() {
            for &b in &bs {
                if let Some(q) = fr.get(b) {
                    rots[fi][b] = *q;
                }
            }
        }
        if bs.contains(&0) {
            roots = o;
        }
    }
    (rots, roots)
}

fn sample_plain(keys: &[Key], bones: usize, looping: bool, frames: &[f64]) -> Samples {
    let mut rots = Vec::with_capacity(frames.len());
    let mut roots = Vec::with_capacity(frames.len());
    for &f in frames {
        let (r, o) = sample_one(keys, bones, looping, f);
        rots.push(r);
        roots.push(o);
    }
    (rots, roots)
}

/// Keys k0 <= f < k1 plus their outer neighbours, and the segment parameter t.
fn neighbours(ks: &[Key], looping: bool, f: f64) -> Option<(&Key, &Key, &Key, &Key, f64)> {
    let n = ks.len();
    let first = ks.first()?;
    let last = ks.last()?;
    if n == 1 {
        return Some((first, first, first, first, 0.0));
    }
    let at = |i: usize| ks.get(i);
    let search = |f: f64| ks.partition_point(|k| (k.frame as f64) <= f).saturating_sub(1);
    if looping {
        let period = (last.frame + 1 - first.frame) as f64;
        let f = first.frame as f64 + (f - first.frame as f64).rem_euclid(period);
        let i = search(f);
        let (k0, k1) = (at(i)?, at((i + 1) % n)?);
        let span = (k1.frame - k0.frame) as f64;
        let span = span.rem_euclid(period);
        let span = if span == 0.0 { period } else { span };
        let t = (f - k0.frame as f64) / span;
        return Some((at((i + n - 1) % n)?, k0, k1, at((i + 2) % n)?, t));
    }
    if f <= first.frame as f64 {
        return Some((first, first, first, first, 0.0));
    }
    if f >= last.frame as f64 {
        return Some((last, last, last, last, 0.0));
    }
    let i = search(f);
    let (k0, k1) = (at(i)?, at(i + 1)?);
    let t = (f - k0.frame as f64) / (k1.frame - k0.frame) as f64;
    Some((at(i.saturating_sub(1))?, k0, k1, at((i + 2).min(n - 1))?, t))
}

fn sample_one(keys: &[Key], bones: usize, looping: bool, f: f64) -> (Vec<Quat>, Vec3) {
    let Some((kp, k0, k1, kn, t)) = neighbours(keys, looping, f) else {
        return (vec![quat::IDENTITY; bones], [0.0; 3]);
    };
    let t = k0.interp.ease(t);
    if t == 0.0 || k0.interp == Interp::Step {
        return (k0.rotations.clone(), k0.root);
    }
    if k0.interp == Interp::Linear {
        let r = k0
            .rotations
            .iter()
            .zip(&k1.rotations)
            .map(|(a, b)| quat::slerp(*a, *b, t))
            .collect();
        return (
            r,
            quat::add3(k0.root, quat::scale3(quat::sub3(k1.root, k0.root), t)),
        );
    }
    // frames from each key to the next (0: no key on that side)
    let period = match (keys.first(), keys.last()) {
        (Some(a), Some(b)) => (b.frame + 1 - a.frame) as f64,
        _ => 1.0,
    };
    let span = |a: &Key, b: &Key| {
        let d = (b.frame - a.frame) as f64;
        if looping { d.rem_euclid(period) } else { d }
    };
    let (h_in, h, h_out) = (span(kp, k0), span(k0, k1), span(k1, kn));
    let h = if h > 0.0 { h } else { period };
    let mut rot = Vec::with_capacity(bones);
    for b in 0..bones {
        let (Some(&q0), Some(&q1), Some(&qp), Some(&qn)) = (
            k0.rotations.get(b),
            k1.rotations.get(b),
            kp.rotations.get(b),
            kn.rotations.get(b),
        ) else {
            rot.push(quat::IDENTITY);
            continue;
        };
        let q1 = quat::align(q1, q0);
        let qp = quat::align(qp, q0);
        let qn = quat::align(qn, q1);
        // the turns into, across and out of the segment, as rotation vectors (each seen from the
        // key it starts at), and squad's inner controls for the clamped tangents at both ends
        let into = quat::log(quat::mul(quat::conj(qp), q0));
        let across = quat::log(quat::mul(quat::conj(q0), q1));
        let back = quat::log(quat::mul(quat::conj(q1), q0));
        let out = quat::log(quat::mul(quat::conj(q1), qn));
        let t0: Vec3 = std::array::from_fn(|i| slope(into[i], across[i], h_in, h) * h);
        let t1: Vec3 = std::array::from_fn(|i| slope(-back[i], out[i], h, h_out) * h);
        let s0 = quat::normalize(quat::mul(
            q0,
            quat::exp(quat::scale3(quat::sub3(t0, across), 0.5)),
        ));
        let s1 = quat::normalize(quat::mul(q1, quat::exp(quat::scale3(quat::add3(t1, back), -0.5))));
        rot.push(quat::squad_arc(q0, q1, s0, s1, t));
    }
    // cubic Hermite on the root, with the same clamped tangents
    let (t2, t3) = (t * t, t * t * t);
    let mut root = [0.0; 3];
    for (i, r) in root.iter_mut().enumerate() {
        let (p, a, b, n) = (kp.root[i], k0.root[i], k1.root[i], kn.root[i]);
        let m0 = slope(a - p, b - a, h_in, h) * h;
        let m1 = slope(b - a, n - b, h, h_out) * h;
        *r = (2.0 * t3 - 3.0 * t2 + 1.0) * a
            + (t3 - 2.0 * t2 + t) * m0
            + (-2.0 * t3 + 3.0 * t2) * b
            + (t3 - t2) * m1;
    }
    (rot, root)
}

/// A smooth curve's slope (per frame) at a key, from the change `d_in` over the `h_in` frames
/// before it and `d_out` over the `h_out` frames after. Flat where the keys hold or turn back, and
/// at most three times the gentler side's slope, so the curve never swings past a key's
/// neighbours (monotone cubic, Fritsch–Carlson): a hold before a long move stays put. An end key
/// (`h_in` or `h_out` 0) takes half its one segment's slope.
fn slope(d_in: f64, d_out: f64, h_in: f64, h_out: f64) -> f64 {
    if h_in <= 0.0 {
        return if h_out > 0.0 { d_out / h_out / 2.0 } else { 0.0 };
    }
    if h_out <= 0.0 {
        return d_in / h_in / 2.0;
    }
    let (s_in, s_out) = (d_in / h_in, d_out / h_out);
    if s_in * s_out <= 0.0 {
        return 0.0;
    }
    let limit = 3.0 * s_in.abs().min(s_out.abs());
    ((d_in + d_out) / (h_in + h_out)).clamp(-limit, limit)
}
