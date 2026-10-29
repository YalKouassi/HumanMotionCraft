//! Headless pictures of the document's pose (for `ac render`, MCP and fixture views).

use std::path::Path;

use ac_core::Document;
use ac_render::Mesh;
use ac_render::glam::Vec3;
pub use ac_render::offscreen::Framing;
use ac_render::offscreen::{self, Offscreen};

pub struct Pictures {
    off: Offscreen,
}

impl Pictures {
    pub fn new(assets: &Path, bones: &[String]) -> Result<Pictures, String> {
        Pictures::with_adapter(assets, bones, false)
    }

    /// `software`: on the CPU adapter (WARP), the same on every machine (visual goldens).
    pub fn with_adapter(assets: &Path, bones: &[String], software: bool) -> Result<Pictures, String> {
        let glb = std::fs::read(assets.join("rig/mannequin.glb"))
            .map_err(|e| format!("can't read the mannequin: {e}"))?;
        Ok(Pictures {
            off: Offscreen::with_adapter(&Mesh::from_glb(&glb)?, bones, software)?,
        })
    }

    /// The pose shown, from a named view that follows the body, as PNG bytes.
    pub fn png(&mut self, doc: &Document, view: &str, w: u32, h: u32) -> Result<Vec<u8>, String> {
        let rgba = self.rgba(doc, view, w, h, None)?;
        offscreen::png(w, h, &rgba)
    }

    /// The pose shown as RGBA rows. `framing`: a fixed camera (`None`: the named view's usual
    /// distance, looking at the body, so the view follows it).
    pub fn rgba(
        &mut self,
        doc: &Document,
        view: &str,
        w: u32,
        h: u32,
        framing: Option<Framing>,
    ) -> Result<Vec<u8>, String> {
        let sk = &doc.skeleton;
        let (rots, pos) = sk
            .fk(&doc.pose.rotations, doc.pose.root)
            .map_err(|e| e.to_string())?;
        let root = pos.first().copied().unwrap_or_default();
        let mut camera = match framing {
            Some(f) => offscreen::view(
                view,
                Vec3::new(f.target[0] as f32, f.target[1] as f32, f.target[2] as f32),
            )?,
            None => offscreen::view(view, Vec3::new(root[0] as f32, root[1] as f32, 0.9))?,
        };
        if let Some(f) = framing {
            camera.distance = f.distance as f32;
        }
        self.off.render(w, h, &camera, (&rots, &pos), &sk.heads)
    }

    /// The motion shown at `times` (physics included where it plays) as RGBA rows, one camera
    /// for all when `fixed`; the document is not changed.
    pub fn frames(
        &mut self,
        doc: &Document,
        times: &[f64],
        view: &str,
        size: (u32, u32),
        fixed: bool,
    ) -> Result<Vec<Vec<u8>>, String> {
        let posed = doc
            .shown_poses(times)?
            .iter()
            .map(|p| doc.skeleton.fk(&p.rotations, p.root).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, String>>()?;
        self.off.frames(view, size, &posed, &doc.skeleton.heads, fixed)
    }

    /// The adapter the pictures come from.
    pub fn adapter(&self) -> &str {
        self.off.adapter()
    }
}

/// The fit for the body over these frames of the document, as shown (with the physics pass if it
/// plays); the document is not changed.
pub fn framing_of(doc: &Document, frames: &[f64], aspect: f64) -> Result<Framing, String> {
    let mut points = Vec::new();
    for pose in doc.shown_poses(frames)? {
        points.extend(
            doc.skeleton
                .fk(&pose.rotations, pose.root)
                .map_err(|e| e.to_string())?
                .1,
        );
    }
    Ok(Framing::fit(&points, aspect))
}

/// A contact sheet of the document's frames (the motion shown), one camera for all.
pub fn sheet(
    pictures: &mut Pictures,
    doc: &Document,
    view: &str,
    cell: (u32, u32, u32),
    frames: &[f64],
) -> Result<Vec<u8>, String> {
    let posed = doc
        .shown_poses(frames)?
        .iter()
        .map(|p| doc.skeleton.fk(&p.rotations, p.root).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, String>>()?;
    pictures.off.sheet(view, cell, &posed, &doc.skeleton.heads, true)
}

/// Pictures of equal size side by side (RGBA rows), with a 2-pixel gap.
pub fn strip(w: u32, h: u32, frames: &[Vec<u8>]) -> (u32, Vec<u8>) {
    let (tw, _, rgba) = offscreen::grid(w, h, frames.len() as u32, frames);
    (tw, rgba)
}

/// ffmpeg: `$AC_FFMPEG`, else `ffmpeg` on the PATH, else the workspace's usual install.
pub use ac_media::ffmpeg;

pub use ac_render::offscreen::base64;

/// Standard base64 back to bytes (padding optional).
pub fn unbase64(text: &str) -> Result<Vec<u8>, String> {
    let val = |c: u8| -> Result<u32, String> {
        Ok(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err(format!("not base64: {:?}", c as char)),
        }
        .into())
    };
    let bytes: Vec<u8> = text
        .bytes()
        .filter(|c| !c.is_ascii_whitespace() && *c != b'=')
        .collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for c in bytes.chunks(4) {
        let mut n = 0u32;
        for (i, &b) in c.iter().enumerate() {
            n |= val(b)? << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if c.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if c.len() > 3 {
            out.push(n as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #[test]
    fn base64_matches_the_rfc_examples() {
        for (i, o) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(super::base64(i.as_bytes()), o);
            assert_eq!(super::unbase64(o).unwrap(), i.as_bytes());
        }
    }
}
