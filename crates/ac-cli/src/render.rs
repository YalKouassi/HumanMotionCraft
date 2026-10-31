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

    /// How the pictures from here on are drawn.
    pub fn set_look(&mut self, look: offscreen::Look) {
        self.off.set_look(look);
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

/// The picture flags of `ac render` and `ac video`, checked.
pub fn look_flags(
    no_floor: bool,
    background: Option<&str>,
    camera: Option<&str>,
) -> Result<offscreen::Look, String> {
    Ok(offscreen::Look {
        floor: !no_floor,
        background: background.map_or(Ok(offscreen::Background::Usual), offscreen::Background::parse)?,
        camera: camera.map(offscreen::Orbit::parse).transpose()?,
    })
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

/// Clip `text`'s preview strip, as the app's library popover draws it (and sends with a share):
/// [`ac_community::PREVIEW_FRAMES`] frames spread over the clip, each cell framed on its own body,
/// at twice [`ac_community::PREVIEW_CELL`] for sharp high-DPI pictures, packed small to send.
pub fn clip_strip(
    pictures: &mut Pictures,
    sk: &ac_core::rig::Skeleton,
    text: &str,
    lib: &ac_io::library::Library,
) -> Result<Vec<u8>, String> {
    let clip = ac_core::clipfile::load_clip(text)?;
    let links = ac_io::commands::load_links(lib, &clip)?;
    let last = (clip.length_with(&links) - 1).max(0) as f64;
    let n = ac_community::PREVIEW_FRAMES.max(2);
    let frames: Vec<f64> = (0..n)
        .map(|i| (last * i as f64 / (n - 1) as f64).round())
        .collect();
    let (r, o) = clip.sample_with(&frames, &links)?;
    let posed = r
        .iter()
        .zip(o)
        .map(|(r, o)| sk.fk(r, o).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, String>>()?;
    let (w, h) = ac_community::PREVIEW_CELL;
    let png = pictures.off.sheet(
        "three-quarter",
        (w * 2, h * 2, posed.len() as u32),
        &posed,
        &sk.heads,
        false,
    )?;
    offscreen::png_small(&png)
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
