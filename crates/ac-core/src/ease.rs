//! An AI fill eased through the key between two filled spans.
//!
//! Each span is filled on its own: the model starts a span from its first key and heads for the
//! next one, whatever speed the motion came in with. Two filled spans then meet at your key with a
//! kink (the arms swing one way into the key and snap the other way out). Over [`EASE_FRAMES`]
//! frames each side of such a key, the AI keys are re-curved (a cubic Hermite in each bone's
//! rotation relative to the key, and in the root) so the motion goes through the key with one
//! velocity. Your key and the frames past the window stay as they are, and the curve meets them
//! with their own slope. Spans a helper made are never touched (see [`crate::edits::GENERATED`]
//! and [`crate::edits::POSE_FILL`]).
//! An anchor pose inside a long fill ([`crate::anchors`]) is a key the ease goes through, like
//! yours: the spans on both sides of it were filled on their own and meet there.

use crate::clip::{Clip, Key, KeySource};
use crate::document::Document;
use crate::edits::is_generated;
use crate::quat::{self, Quat, Vec3};

/// Frames each side of the key the ease reaches (the window's last frame keeps its value).
pub const EASE_FRAMES: i64 = 4;

type R<T> = Result<T, String>;

fn eased(k: &Key) -> bool {
    k.source == KeySource::Ai
        && k.only.is_none()
        && !is_generated(&k.model)
        && !crate::anchors::is_anchor(&k.model)
}

/// A key two fills meet at: yours, or an anchor pose placed inside a long fill.
fn through(k: &Key) -> bool {
    k.source == KeySource::Manual || (k.source == KeySource::Ai && crate::anchors::is_anchor(&k.model))
}

/// The key on `frame` in the edited layer.
fn key_at(clip: &Clip, frame: i64) -> Option<&Key> {
    clip.keys().iter().find(|k| k.frame == frame)
}

/// AI keys in a row next to `k` on one side (`dir` -1 or 1), at most `n`.
fn run(clip: &Clip, k: i64, dir: i64, n: i64) -> i64 {
    (1..=n)
        .take_while(|d| key_at(clip, k + dir * d).is_some_and(eased))
        .count() as i64
}

/// Cubic Hermite from (p0, m0) at t = 0 to (p1, m1) at t = 1.
fn hermite(p0: f64, m0: f64, p1: f64, m1: f64, t: f64) -> f64 {
    let (t2, t3) = (t * t, t * t * t);
    (2.0 * t3 - 3.0 * t2 + 1.0) * p0 + (t3 - 2.0 * t2 + t) * m0 + (-2.0 * t3 + 3.0 * t2) * p1 + (t3 - t2) * m1
}

/// Ease the AI keys around your key (or an anchor) on frame `k` when both sides are AI keys;
/// returns the frames changed (0: a side is plain, generated from text or missing, or the key
/// is neither yours nor an anchor).
pub fn ease_through(clip: &mut Clip, k: i64, n: i64) -> R<usize> {
    if !key_at(clip, k).is_some_and(through) {
        return Ok(0);
    }
    let (l, r) = (run(clip, k, -1, n), run(clip, k, 1, n));
    // a window of two frames a side changes one frame there; less has nothing to re-curve
    if l < 2 || r < 2 {
        return Ok(0);
    }
    // the values the curve is fitted to: frames k-l-1 ..= k+r+1 (the outer ones give the slopes)
    let frames: Vec<i64> = (k - l - 1..=k + r + 1).collect();
    let keys: Vec<Key> = frames
        .iter()
        .map(|&f| key_at(clip, f).cloned())
        .collect::<Option<_>>()
        .ok_or("a frame next to the ease window has no key")?;
    let kk = keys.get(l as usize + 1).ok_or("the key")?.clone();
    let nb = kk.rotations.len();
    // per frame: each bone's rotation from the key's (log), then the root from the key's
    let rel = |x: &Key| -> Vec<f64> {
        let mut v = Vec::with_capacity(nb * 3 + 3);
        for (q, qk) in x.rotations.iter().zip(&kk.rotations) {
            v.extend(quat::log(quat::align(
                quat::mul(quat::conj(*qk), *q),
                [0.0, 0.0, 0.0, 1.0],
            )));
        }
        v.extend(quat::sub3(x.root, kk.root));
        v
    };
    let vals: Vec<Vec<f64>> = keys.iter().map(rel).collect();
    let at = |i: i64| vals.get((i + l + 1) as usize);
    let (Some(lo), Some(lo_out), Some(hi), Some(hi_out)) = (at(-l), at(-l - 1), at(r), at(r + 1)) else {
        return Ok(0);
    };
    let dims = lo.len();
    let mut changed = 0;
    for i in (-l + 1..r).filter(|&i| i != 0) {
        let mut v = vec![0.0; dims];
        for (d, out) in v.iter_mut().enumerate() {
            let (a, a_out, b, b_out) = (lo[d], lo_out[d], hi[d], hi_out[d]);
            // through the key: the mean of the window's two chord slopes (a frame each)
            let mk = ((0.0 - a) / l as f64 + (b - 0.0) / r as f64) / 2.0;
            *out = if i < 0 {
                let t = (i + l) as f64 / l as f64;
                hermite(a, (a - a_out) * l as f64, 0.0, mk * l as f64, t)
            } else {
                let t = i as f64 / r as f64;
                hermite(0.0, mk * r as f64, b, (b_out - b) * r as f64, t)
            };
        }
        let rotations: Vec<Quat> = kk
            .rotations
            .iter()
            .enumerate()
            .map(|(j, qk)| {
                let w: Vec3 = [v[j * 3], v[j * 3 + 1], v[j * 3 + 2]];
                quat::normalize(quat::mul(*qk, quat::exp(w)))
            })
            .collect();
        let root = quat::add3(kk.root, [v[nb * 3], v[nb * 3 + 1], v[nb * 3 + 2]]);
        let mut key = key_at(clip, k + i).cloned().ok_or("an eased frame has no key")?;
        key.rotations = rotations;
        key.root = root;
        clip.set_key(key)?;
        changed += 1;
    }
    Ok(changed)
}

impl Document {
    /// After the span `a..b` was filled: ease through `a` and `b` where the span next door is
    /// filled too, and plant that span's feet again (the ease reached into it).
    pub(crate) fn ease_borders(&mut self, a: i64, b: i64) -> R<()> {
        let m = crate::edits::manual_frames(&self.clip);
        let before = m.iter().copied().filter(|&x| x < a).max();
        let after = m.iter().copied().filter(|&x| x > b).min();
        for (k, other) in [(a, before.map(|p| (p, a))), (b, after.map(|n| (b, n)))] {
            if ease_through(&mut self.clip, k, EASE_FRAMES)? > 0
                && let Some((x, y)) = other
            {
                self.plant_feet(x, y, crate::ai::PLANT_STILL, crate::ai::PLANT_GROUND)?;
            }
        }
        Ok(())
    }
}
