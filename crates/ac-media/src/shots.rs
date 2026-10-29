//! Rendering the animation as it plays (physics included) into frames, headless: what a video,
//! a GIF and a contact sheet are made of.

use std::path::Path;

use ac_core::Document;
use ac_render::Mesh;
use ac_render::glam::Vec3;
use ac_render::offscreen::{self, Framing, Offscreen};

use crate::encode::Encoder;

/// The named camera views.
pub const VIEWS: &[&str] = &["front", "back", "left", "right", "side", "top", "three-quarter"];
/// Most frames one video holds.
pub const MAX_FRAMES: usize = 20_000;

/// A headless renderer on the GPU, or on the software adapter when there is no GPU (said in
/// `note`, so a front end can tell the person why it is slow).
pub struct Shots {
    off: Offscreen,
    pub note: Option<String>,
}

impl Shots {
    pub fn new(assets: &Path, bones: &[String]) -> Result<Shots, String> {
        let glb = std::fs::read(assets.join("rig/mannequin.glb"))
            .map_err(|e| format!("can't read the mannequin: {e}"))?;
        let mesh = Mesh::from_glb(&glb)?;
        match Offscreen::new(&mesh, bones) {
            Ok(off) => Ok(Shots { off, note: None }),
            Err(gpu) => {
                let off = Offscreen::with_adapter(&mesh, bones, true)
                    .map_err(|e| format!("no renderer: {gpu}; and {e}"))?;
                Ok(Shots {
                    off,
                    note: Some(format!("drawn on the software renderer ({gpu})")),
                })
            }
        }
    }

    /// RGBA rows of the document's shown motion at each of `times` (frames), from `view`: a
    /// fixed camera that sees the whole shot (`fixed`), else one that follows the body.
    pub fn frames(
        &mut self,
        doc: &Document,
        times: &[f64],
        view: &str,
        (w, h): (u32, u32),
        fixed: bool,
        mut each: impl FnMut(Vec<u8>) -> Result<(), String>,
    ) -> Result<(), String> {
        let sk = &doc.skeleton;
        let poses = doc.shown_poses(times)?;
        let posed = poses
            .iter()
            .map(|p| sk.fk(&p.rotations, p.root).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, String>>()?;
        let framing = if fixed {
            let points: Vec<[f64; 3]> = posed.iter().flat_map(|p| p.1.iter().copied()).collect();
            Some(Framing::fit(&points, f64::from(w) / f64::from(h.max(1))))
        } else {
            None
        };
        for (rots, pos) in &posed {
            let camera = match framing {
                Some(f) => f.camera(view)?,
                None => {
                    let root = pos.first().copied().unwrap_or_default();
                    offscreen::view(view, Vec3::new(root[0] as f32, root[1] as f32, 0.9))?
                }
            };
            each(self.off.render(w, h, &camera, (rots, pos), &sk.heads)?)?;
        }
        Ok(())
    }

    /// Play `times` into `enc`; returns the frames written.
    pub fn video(
        &mut self,
        doc: &Document,
        times: &[f64],
        view: &str,
        size: (u32, u32),
        fixed: bool,
        mut enc: Encoder,
    ) -> Result<usize, String> {
        self.frames(doc, times, view, size, fixed, |rgba| enc.push(&rgba))?;
        enc.finish()
    }

    /// A contact sheet (PNG bytes) of these frames, one camera for all, `cols` to a row.
    pub fn sheet(
        &mut self,
        doc: &Document,
        frames: &[f64],
        view: &str,
        cell: (u32, u32, u32),
    ) -> Result<Vec<u8>, String> {
        let posed = doc
            .shown_poses(frames)?
            .iter()
            .map(|p| doc.skeleton.fk(&p.rotations, p.root).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, String>>()?;
        self.off.sheet(view, cell, &posed, &doc.skeleton.heads, true)
    }
}

/// The frame times from `from` to `to` (inclusive) played at `fps` (the clip's own rate gives
/// whole frames).
pub fn times(clip_fps: f64, from: f64, to: f64, fps: f64) -> Result<Vec<f64>, String> {
    if !(fps.is_finite() && fps > 0.0 && clip_fps.is_finite() && clip_fps > 0.0) {
        return Err(format!("{fps} frames per second"));
    }
    if !(from.is_finite() && to.is_finite() && 0.0 <= from && from <= to) {
        return Err(format!("frames {from} to {to}: from 0, rising"));
    }
    let step = clip_fps / fps;
    let n = ((to - from) / step + 1e-9).floor() as usize + 1;
    if n > MAX_FRAMES {
        return Err(format!("{n} frames is more than {MAX_FRAMES} in one video"));
    }
    Ok((0..n).map(|i| from + i as f64 * step).collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn times_step_by_the_rate() {
        assert_eq!(
            super::times(30.0, 0.0, 4.0, 30.0),
            Ok(vec![0.0, 1.0, 2.0, 3.0, 4.0])
        );
        assert_eq!(super::times(30.0, 0.0, 4.0, 15.0), Ok(vec![0.0, 2.0, 4.0]));
        assert!(super::times(30.0, 5.0, 4.0, 30.0).is_err());
        assert!(super::times(30.0, 0.0, 1e12, 30.0).is_err());
    }
}
