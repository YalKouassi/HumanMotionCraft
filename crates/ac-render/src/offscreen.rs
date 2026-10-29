//! Headless pictures of a pose (no window): for the CLI, MCP and fixture views, so a person can
//! confirm a result from an image.

use glam::Vec3;

use crate::renderer::{Body, Camera, Renderer};
use crate::{Mesh, skin};

/// Most pixels on a side of a headless picture.
pub const MAX_SIDE: u32 = 4096;

/// Named camera angles around a target (the rig faces -Y).
pub fn view(name: &str, target: Vec3) -> Result<Camera, String> {
    let (yaw, pitch) = match name {
        "front" => (0.0, 0.1),
        "back" => (std::f32::consts::PI, 0.1),
        "left" => (std::f32::consts::FRAC_PI_2, 0.1),
        "right" | "side" => (-std::f32::consts::FRAC_PI_2, 0.1),
        "top" => (0.0, 1.45),
        "bottom" => (0.0, -1.45),
        "three-quarter" => (0.6, 0.25),
        other => {
            return Err(format!(
                "unknown view {other:?}; one of front, back, left, right, side, top, bottom, three-quarter"
            ));
        }
    };
    Ok(Camera {
        target,
        yaw,
        pitch,
        distance: 3.6,
        fov_y: 0.7,
        ortho: false,
    })
}

/// The named views' vertical field of view.
const FOV_Y: f64 = 0.7;
/// How much of a tight picture's half-size the farthest point reaches.
const FILL: f32 = 0.88;

/// Where a fixed camera looks and how far back it stands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Framing {
    pub target: [f64; 3],
    pub distance: f64,
}

impl Framing {
    /// A camera that sees every point (joint positions over all the frames shown) with room to
    /// spare, and the floor under them; never closer than a view's usual 3.6 m. The fit is by a
    /// bounding sphere, so it holds from every named view.
    pub fn fit(points: &[[f64; 3]], aspect: f64) -> Framing {
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for p in points.iter().filter(|p| p.iter().all(|v| v.is_finite())) {
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
        if lo[0] > hi[0] {
            return Framing {
                target: [0.0, 0.0, 0.9],
                distance: 3.6,
            };
        }
        lo[2] = lo[2].min(0.0); // the floor
        hi[2] += 0.25; // the joints stop below the top of the head
        let target = [
            (lo[0] + hi[0]) / 2.0,
            (lo[1] + hi[1]) / 2.0,
            (lo[2] + hi[2]) / 2.0,
        ];
        let radius =
            ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt() / 2.0 + 0.1;
        let half = (FOV_Y / 2.0).tan() * aspect.clamp(0.1, 1.0); // the narrower half-angle's tangent
        Framing {
            target,
            distance: (radius / half.atan().sin()).max(3.6),
        }
    }

    /// A camera from a named view that sees `points` snugly: the bounding-sphere fit, then the
    /// distance adjusted (a few rounds) until the points' farthest projection is at `FILL` of
    /// the picture's half-size, and never closer than 1.5 m.
    pub fn tight(points: &[[f64; 3]], view_name: &str, aspect: f64) -> Result<Camera, String> {
        let mut cam = Framing::fit(points, aspect).camera(view_name)?;
        let pts: Vec<Vec3> = points
            .iter()
            .filter(|p| p.iter().all(|v| v.is_finite()))
            .map(|p| Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32))
            .collect();
        let (w, h) = (aspect.clamp(0.05, 20.0) as f32, 1.0);
        for _ in 0..6 {
            let mut reach = 0.0f32;
            for p in &pts {
                match cam.project(*p, w, h) {
                    Some(q) => {
                        reach = reach
                            .max((q[0] / w * 2.0 - 1.0).abs())
                            .max((q[1] / h * 2.0 - 1.0).abs())
                    }
                    None => reach = reach.max(2.0), // behind the camera: back off
                }
            }
            if reach <= 0.0 || (reach - FILL).abs() < 0.02 {
                break;
            }
            cam.distance = (cam.distance * reach / FILL).max(1.5);
        }
        Ok(cam)
    }

    /// This framing from a named view.
    pub fn camera(&self, view_name: &str) -> Result<Camera, String> {
        let t = self.target;
        let mut c = view(view_name, Vec3::new(t[0] as f32, t[1] as f32, t[2] as f32))?;
        c.distance = self.distance as f32;
        Ok(c)
    }
}

/// Pictures of equal size (RGBA rows) in a grid of `cols` columns, 2-pixel gaps; returns the
/// sheet's width, height and rows.
pub fn grid(w: u32, h: u32, cols: u32, frames: &[Vec<u8>]) -> (u32, u32, Vec<u8>) {
    let gap = 2u32;
    let n = frames.len() as u32;
    let cols = cols.clamp(1, n.max(1));
    let rows = n.div_ceil(cols).max(1);
    let (tw, th) = (cols * w + (cols - 1) * gap, rows * h + (rows - 1) * gap);
    let mut out = vec![0u8; (tw * th * 4) as usize];
    for (i, f) in frames.iter().enumerate() {
        let (x0, y0) = ((i as u32 % cols) * (w + gap), (i as u32 / cols) * (h + gap));
        for row in 0..h {
            let src = &f[(row * w * 4) as usize..((row + 1) * w * 4) as usize];
            let at = (((y0 + row) * tw + x0) * 4) as usize;
            out[at..at + src.len()].copy_from_slice(src);
        }
    }
    (tw, th, out)
}

/// A posed rig: world rotations and joint positions (as the rig's FK gives them).
pub type Posed = (Vec<[[f64; 3]; 3]>, Vec<[f64; 3]>);

pub struct Offscreen {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    /// Skin joint i is rig bone `skin_to_bone[i]`.
    skin_to_bone: Vec<usize>,
    /// The adapter the pictures come from (pictures differ a little between adapters).
    adapter: String,
}

impl Offscreen {
    /// A GPU device of its own (any adapter, software included) and the mannequin on it.
    pub fn new(mesh: &Mesh, bones: &[String]) -> Result<Offscreen, String> {
        Offscreen::with_adapter(mesh, bones, false)
    }

    /// `software`: the fallback (CPU) adapter only, WARP on Windows, so pictures are the same on
    /// every machine (visual goldens). Refused when there is none.
    pub fn with_adapter(mesh: &Mesh, bones: &[String], software: bool) -> Result<Offscreen, String> {
        let skin_to_bone = mesh
            .joints
            .iter()
            .map(|n| {
                bones
                    .iter()
                    .position(|b| b == n)
                    .ok_or(format!("the mannequin's joint {n:?} is not in the rig"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let options = wgpu::RequestAdapterOptions {
            force_fallback_adapter: software,
            ..Default::default()
        };
        let adapter = pollster::block_on(instance.request_adapter(&options)).map_err(|e| {
            if software {
                format!("no software adapter (WARP) for golden pictures: {e}")
            } else {
                format!("no GPU adapter for headless rendering: {e}")
            }
        })?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .map_err(|e| format!("no GPU device: {e}"))?;
        let renderer = Renderer::new(&device, wgpu::TextureFormat::Rgba8Unorm, mesh);
        let info = adapter.get_info();
        Ok(Offscreen {
            device,
            queue,
            renderer,
            skin_to_bone,
            adapter: format!("{} ({:?}, {:?})", info.name, info.backend, info.device_type),
        })
    }

    /// The adapter the pictures come from: name, backend and kind.
    pub fn adapter(&self) -> &str {
        &self.adapter
    }

    /// RGBA8 rows of the pose given as world rotations (row-major) and joint positions, from
    /// `camera`; `ghosts` are drawn see-through behind it.
    pub fn render(
        &mut self,
        w: u32,
        h: u32,
        camera: &Camera,
        pose: (&[[[f64; 3]; 3]], &[[f64; 3]]),
        rest: &[[f64; 3]],
    ) -> Result<Vec<u8>, String> {
        if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
            return Err(format!(
                "picture size {w}x{h} must be 1..={MAX_SIDE} on each side"
            ));
        }
        let all = skin::skin_matrices(pose.0, pose.1, rest);
        let joints: Vec<_> = self
            .skin_to_bone
            .iter()
            .map(|&b| all.get(b).copied().unwrap_or_default())
            .collect();
        let body = Body {
            joints: &joints,
            tint: [0.72, 0.74, 0.78, 1.0],
        };
        self.renderer
            .render(&self.device, &self.queue, w, h, camera, &[body])?;
        Ok(self.renderer.read_rgba(&self.device, &self.queue)?.2)
    }
}

/// RGBA8 rows to PNG bytes.
impl Offscreen {
    /// A contact sheet, `cols` to a row (cells `w` x `h`), as PNG bytes. `shared`: one camera
    /// that sees every frame (heights and travel compare); else each cell framed on its own body
    /// (thumbnails).
    pub fn sheet(
        &mut self,
        view_name: &str,
        (w, h, cols): (u32, u32, u32),
        frames: &[Posed],
        rest: &[[f64; 3]],
        shared: bool,
    ) -> Result<Vec<u8>, String> {
        if frames.is_empty() {
            return Err("no frames to show".into());
        }
        let aspect = f64::from(w) / f64::from(h.max(1));
        let points: Vec<[f64; 3]> = frames.iter().flat_map(|f| f.1.iter().copied()).collect();
        let all = Framing::tight(&points, view_name, aspect)?;
        let mut cells = Vec::with_capacity(frames.len());
        for (rots, pos) in frames {
            let camera = if shared {
                all
            } else {
                Framing::tight(pos, view_name, aspect)?
            };
            cells.push(self.render(w, h, &camera, (rots, pos), rest)?);
        }
        let (tw, th, rgba) = grid(w, h, cols, &cells);
        png(tw, th, &rgba)
    }
}

/// A PNG's width, height and RGBA rows.
pub fn unpng(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND);
    let mut rd = dec.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; rd.output_buffer_size()];
    let info = rd.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let px = (info.width * info.height) as usize;
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..px * 4].to_vec(),
        png::ColorType::Rgb => buf[..px * 3]
            .chunks(3)
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect(),
        other => return Err(format!("PNG colour type {other:?} (want RGB or RGBA)")),
    };
    Ok((info.width, info.height, rgba))
}

/// How two pictures of one size differ: the pixels with a channel more than `tolerance` apart,
/// and a picture of them (red where they differ, the expected picture dimmed elsewhere).
pub fn diff(expected: &[u8], actual: &[u8], tolerance: u8) -> (usize, Vec<u8>) {
    let mut n = 0;
    let mut out = Vec::with_capacity(expected.len());
    for (e, a) in expected.chunks(4).zip(actual.chunks(4)) {
        if e.iter().zip(a).any(|(x, y)| x.abs_diff(*y) > tolerance) {
            n += 1;
            out.extend([255, 0, 0, 255]);
        } else {
            out.extend([e[0] / 3, e[1] / 3, e[2] / 3, 255]);
        }
    }
    (n, out)
}

pub fn png(w: u32, h: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().map_err(|e| e.to_string())?;
        wr.write_image_data(rgba).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// Standard base64 (for MCP image content).
pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod framing_tests {
    use super::{Framing, grid};

    /// Worked by hand: points from the floor to 3.75 m tops out at 4.0 m with the head room, so
    /// the camera looks at z 2.0; the sphere's radius is 2.0 + 0.1, and a square picture's
    /// half-angle is 0.35 rad, so the camera stands 2.1 / sin 0.35 = 6.12427 m back. A standing
    /// body needs no more than the usual 3.6 m. Points that aren't numbers are skipped; none at
    /// all gives the usual view.
    #[test]
    fn framing_sees_every_point() {
        let f = Framing::fit(&[[0.0, 0.0, 0.0], [0.0, 0.0, 3.75], [f64::NAN, 0.0, 9.0]], 1.0);
        assert_eq!(f.target, [0.0, 0.0, 2.0]);
        assert!((f.distance - 6.12427).abs() < 1e-5, "{}", f.distance);
        // a portrait picture is narrower across, so the camera goes further back
        assert!(Framing::fit(&[[0.0, 0.0, 0.0], [0.0, 0.0, 3.75]], 0.5).distance > f.distance);
        let stand = Framing::fit(&[[0.0, 0.0, 0.05], [0.0, 0.0, 1.6]], 4.0 / 3.0);
        assert_eq!(stand.distance, 3.6);
        assert_eq!(
            Framing::fit(&[], 1.0),
            Framing {
                target: [0.0, 0.0, 0.9],
                distance: 3.6
            }
        );
    }

    /// Two 2x1 pictures, one pixel 30 apart in green (over a tolerance of 24) and one 20 apart
    /// (under it): one pixel differs, drawn red; the other is the expected pixel at a third.
    #[test]
    fn diff_counts_pixels_past_the_tolerance() {
        let e = [90, 90, 90, 255, 90, 90, 90, 255];
        let a = [90, 120, 90, 255, 90, 110, 90, 255];
        let (n, pic) = super::diff(&e, &a, 24);
        assert_eq!(n, 1);
        assert_eq!(pic, vec![255, 0, 0, 255, 30, 30, 30, 255]);
        let bytes = super::png(2, 1, &a).unwrap();
        assert_eq!(super::unpng(&bytes).unwrap(), (2, 1, a.to_vec()));
    }

    /// A standing body (feet to 1.75 m) seen from the front, tightly: its farthest point lands at
    /// 0.88 of the half-size (within the 0.02 the fit stops at), wide or narrow.
    #[test]
    fn tight_fills_the_picture() {
        let body = [
            [-0.2, 0.0, 0.0],
            [0.2, 0.0, 0.0],
            [0.0, 0.1, 1.75],
            [0.0, -0.1, 0.9],
        ];
        for aspect in [0.75, 1.0, 1.6] {
            let cam = Framing::tight(&body, "front", aspect).unwrap();
            let reach = body
                .iter()
                .map(|p| {
                    let q = cam
                        .project(
                            super::Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32),
                            aspect as f32,
                            1.0,
                        )
                        .unwrap();
                    (q[0] / aspect as f32 * 2.0 - 1.0)
                        .abs()
                        .max((q[1] * 2.0 - 1.0).abs())
                })
                .fold(0.0f32, f32::max);
            assert!((reach - 0.88).abs() < 0.03, "aspect {aspect}: reach {reach}");
        }
    }

    /// Five 1x1 cells in 2 columns: 3 rows, 2 + 2 + 1 cells, with 2-pixel gaps (4 x 7 pixels).
    #[test]
    fn grid_lays_cells_in_rows() {
        let cells: Vec<Vec<u8>> = (1..=5u8).map(|i| vec![i; 4]).collect();
        let (w, h, px) = grid(1, 1, 2, &cells);
        assert_eq!((w, h), (4, 7));
        let at = |x: u32, y: u32| px[((y * w + x) * 4) as usize];
        assert_eq!(
            [at(0, 0), at(3, 0), at(0, 3), at(3, 3), at(0, 6), at(3, 6)],
            [1, 2, 3, 4, 5, 0]
        );
    }
}
