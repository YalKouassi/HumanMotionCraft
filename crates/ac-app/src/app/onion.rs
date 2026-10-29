//! Onion skin: ghosts of the keyed motion a few frames (or keys) before and after the playhead,
//! warm before and cool after, fading with distance. Which frames comes from `onion.frames`; the
//! poses are sampled once per clip state and settings, not every frame.

use ac_core::Pose;
use ac_core::clip::Clip;
use serde_json::json;

use super::App;
use super::view_state::OnionMode;

/// Ghosts before the playhead (rgb) and after it.
const ONION_BEFORE: [f32; 3] = [0.98, 0.56, 0.32];
const ONION_AFTER: [f32; 3] = [0.34, 0.74, 1.0];
/// The nearest ghost's opacity; farther ones fade toward none.
const ONION_ALPHA: f32 = 0.34;

/// The ghosts worked out for one clip state, playhead and setting.
pub struct OnionCache {
    clip: Clip,
    /// (frame, count, step, keys mode) asked.
    asked: (f64, u32, u32, bool),
    /// (frame, after the playhead, poses) nearest first on each side.
    ghosts: Vec<(i64, bool, Pose)>,
}

/// A ghost's tint: `i` (0 nearest) of `count` on its side.
pub fn tint(after: bool, i: usize, count: usize) -> [f32; 4] {
    let rgb = if after { ONION_AFTER } else { ONION_BEFORE };
    let fade = 1.0 - i as f32 / (count.max(1) as f32 + 1.0);
    [rgb[0], rgb[1], rgb[2], ONION_ALPHA * fade]
}

impl App {
    /// The onion ghosts at the playhead: (frame, after it, pose, tint), nearest first on each side.
    /// None while the onion skin is off or the animation has no keys.
    pub(super) fn onion_ghosts(&mut self) -> Vec<(i64, Pose, [f32; 4])> {
        let doc = &self.session.document;
        if !self.view.onion || doc.clip.base().keys.is_empty() {
            return Vec::new();
        }
        let v = &self.view;
        let asked = (
            doc.frame,
            v.onion_count,
            v.onion_step,
            v.onion_mode == OnionMode::Keys,
        );
        let fresh = v
            .onion_cache
            .as_ref()
            .is_some_and(|c| c.asked == asked && c.clip == doc.clip);
        if !fresh {
            let params = json!({
                "frame": asked.0,
                "count": asked.1,
                "step": asked.2,
                "mode": if asked.3 { "keys" } else { "frames" },
            });
            let ghosts = match self.session.run("onion.frames", &params) {
                Ok(r) => {
                    let list = |k: &str| -> Vec<i64> {
                        r[k].as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|f| f.as_i64())
                            .collect()
                    };
                    let mut want: Vec<(i64, bool)> = list("before").into_iter().map(|f| (f, false)).collect();
                    want.extend(list("after").into_iter().map(|f| (f, true)));
                    let doc = &self.session.document;
                    let frames: Vec<f64> = want.iter().map(|w| w.0 as f64).collect();
                    match doc.clip.sample_with(&frames, &doc.links) {
                        Ok((rots, roots)) => want
                            .into_iter()
                            .zip(rots.into_iter().zip(roots))
                            .map(|((f, after), (rotations, root))| (f, after, Pose { rotations, root }))
                            .collect(),
                        Err(e) => {
                            self.status = format!("onion skin: {e}");
                            Vec::new()
                        }
                    }
                }
                Err(e) => {
                    self.status = format!("onion skin: {e}");
                    Vec::new()
                }
            };
            self.view.onion_cache = Some(OnionCache {
                clip: self.session.document.clip.clone(),
                asked,
                ghosts,
            });
        }
        let Some(c) = &self.view.onion_cache else {
            return Vec::new();
        };
        let count = |after: bool| c.ghosts.iter().filter(|g| g.1 == after).count();
        let (nb, na) = (count(false), count(true));
        let (mut ib, mut ia) = (0, 0);
        c.ghosts
            .iter()
            .map(|(f, after, pose)| {
                let t = if *after {
                    ia += 1;
                    tint(true, ia - 1, na)
                } else {
                    ib += 1;
                    tint(false, ib - 1, nb)
                };
                (*f, pose.clone(), t)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::tint;

    /// Two ghosts a side: the nearest at the full onion opacity, the next at two thirds of it.
    #[test]
    fn ghosts_fade_with_distance() {
        let near = tint(false, 0, 2);
        let far = tint(false, 1, 2);
        assert!((near[3] - 0.34).abs() < 1e-6);
        assert!((far[3] - 0.34 * 2.0 / 3.0).abs() < 1e-6);
        assert_ne!(tint(true, 0, 2)[..3], near[..3]);
    }
}
