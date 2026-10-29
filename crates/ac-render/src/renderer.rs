//! Offscreen rendering of the floor and up to [`MAX_BODIES`] skinned bodies (the pose shown and
//! its ghosts). The target is a texture the UI shows as an image and screenshots read back.

use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

use crate::mesh::{Mesh, Vertex};

/// The pose shown plus its ghosts (keyed, AI preview, and up to five onion-skin ghosts a side).
pub const MAX_BODIES: usize = 16;
pub const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// How far behind a ghost is drawn (a few centimetres at the usual distance).
const GHOST_BIAS: wgpu::DepthBiasState = wgpu::DepthBiasState {
    constant: 1500,
    slope_scale: 2.0,
    clamp: 0.0,
};
/// Largest render target side (a hostile window size is clamped, not allocated).
pub const MAX_SIDE: u32 = 8192;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FrameUniform {
    view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    tint: [f32; 4],
    _pad: [f32; 40], // to 256 bytes: one dynamic-offset slot per body
}

/// One body to draw: skinning matrices (one per skin joint) and its colour (alpha < 1: a ghost).
pub struct Body<'a> {
    pub joints: &'a [Mat4],
    pub tint: [f32; 4],
}

/// An orbit camera around a target, Z up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub target: Vec3,
    /// Radians about Z; 0 looks along +Y (at the rig's front, which faces -Y).
    pub yaw: f32,
    /// Radians above the floor.
    pub pitch: f32,
    pub distance: f32,
    pub fov_y: f32,
    /// Orthographic: no perspective; the picture is as tall as a perspective one is at `target`.
    pub ortho: bool,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            target: Vec3::new(0.0, 0.0, 0.9),
            yaw: 0.35,
            pitch: 0.18,
            distance: 3.6,
            fov_y: 0.7,
            ortho: false,
        }
    }
}

impl Camera {
    /// Unit vector from the eye toward the target.
    pub fn forward(&self) -> Vec3 {
        (self.target - self.eye()).normalize_or_zero()
    }

    /// Unit vector to the picture's right (horizontal) and up.
    pub fn right_up(&self) -> (Vec3, Vec3) {
        let (sy, cy) = self.yaw.sin_cos();
        let right = Vec3::new(cy, sy, 0.0);
        (right, right.cross(self.forward()).normalize_or_zero())
    }

    pub fn eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.target + Vec3::new(sy * cp, -cy * cp, sp) * self.distance
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye(), self.target, Vec3::Z);
        let aspect = aspect.max(1e-3);
        let proj = if self.ortho {
            // as tall as the perspective view is at the target; depth reaches behind the eye too,
            // so a close orthographic camera never cuts the body
            let half = self.distance * (self.fov_y / 2.0).tan();
            Mat4::orthographic_rh(-half * aspect, half * aspect, -half, half, -100.0, 200.0)
        } else {
            Mat4::perspective_rh(self.fov_y, aspect, 0.03, 200.0)
        };
        proj * view
    }

    /// A world point to pixels in a `w` x `h` view (`None` behind the camera).
    pub fn project(&self, p: Vec3, w: f32, h: f32) -> Option<[f32; 2]> {
        let c = self.view_proj(w / h) * p.extend(1.0);
        if c.w <= 1e-4 {
            return None;
        }
        Some([(c.x / c.w * 0.5 + 0.5) * w, (0.5 - c.y / c.w * 0.5) * h])
    }

    /// The ray through pixel (x, y): origin and unit direction.
    pub fn ray(&self, x: f32, y: f32, w: f32, h: f32) -> (Vec3, Vec3) {
        let inv = self.view_proj(w / h).inverse();
        let ndc = [x / w * 2.0 - 1.0, 1.0 - y / h * 2.0];
        let near = inv.project_point3(Vec3::new(ndc[0], ndc[1], 0.0));
        let far = inv.project_point3(Vec3::new(ndc[0], ndc[1], 1.0));
        (near, (far - near).normalize_or_zero())
    }
}

struct Target {
    size: (u32, u32),
    color: wgpu::Texture,
    color_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
}

pub struct Renderer {
    format: wgpu::TextureFormat,
    body: wgpu::RenderPipeline,
    ghost: wgpu::RenderPipeline,
    floor: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    joint_count: usize,
    uniforms: wgpu::Buffer,
    joint_buffers: Vec<wgpu::Buffer>,
    bind_groups: Vec<wgpu::BindGroup>,
    target: Option<Target>,
}

impl Renderer {
    /// `format` is the colour format of the target (the one the UI's textures use).
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, mesh: &Mesh) -> Renderer {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mannequin"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mannequin"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Uint32x4, 3 => Float32x4];
        let vlayout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &attrs,
        };
        let pipeline = |label: &str,
                        vs: &str,
                        fs: &str,
                        buffers: &[Option<wgpu::VertexBufferLayout>],
                        blend,
                        write_depth,
                        cull| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: cull,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(write_depth),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    // ghosts (the only bodies that don't write depth) sit a little behind, so
                    // where one lies on the body shown it doesn't speckle it
                    bias: if write_depth {
                        Default::default()
                    } else {
                        GHOST_BIAS
                    },
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let body = pipeline(
            "body",
            "vs_body",
            "fs_body",
            &[Some(vlayout.clone())],
            None,
            true,
            Some(wgpu::Face::Back),
        );
        let ghost = pipeline(
            "ghost",
            "vs_body",
            "fs_body",
            &[Some(vlayout)],
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
            Some(wgpu::Face::Back),
        );
        let floor = pipeline("floor", "vs_floor", "fs_floor", &[], None, true, None);
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mannequin vertices"),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mannequin indices"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let slot = std::mem::size_of::<FrameUniform>() as u64;
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame uniforms"),
            size: slot * MAX_BODIES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let joint_count = mesh.joints.len().max(1);
        let mut joint_buffers = Vec::new();
        let mut bind_groups = Vec::new();
        for i in 0..MAX_BODIES {
            let jb = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("joints"),
                size: (joint_count * 64) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            bind_groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("frame"),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &uniforms,
                            offset: slot * i as u64,
                            size: wgpu::BufferSize::new(slot),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: jb.as_entire_binding(),
                    },
                ],
            }));
            joint_buffers.push(jb);
        }
        Renderer {
            format,
            body,
            ghost,
            floor,
            vertices,
            indices,
            index_count: mesh.indices.len() as u32,
            joint_count,
            uniforms,
            joint_buffers,
            bind_groups,
            target: None,
        }
    }

    fn ensure_target(&mut self, device: &wgpu::Device, w: u32, h: u32) {
        let size = (w.clamp(1, MAX_SIDE), h.clamp(1, MAX_SIDE));
        if self.target.as_ref().is_none_or(|t| t.size != size) {
            let extent = wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            };
            let color = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("viewport"),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let depth = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("viewport depth"),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let color_view = color.create_view(&Default::default());
            let depth_view = depth.create_view(&Default::default());
            self.target = Some(Target {
                size,
                color,
                color_view,
                depth_view,
            });
        }
    }

    /// Draw the floor and the bodies (the first is the pose shown; later ones are ghosts).
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        w: u32,
        h: u32,
        camera: &Camera,
        bodies: &[Body],
    ) -> Result<&wgpu::TextureView, String> {
        let bodies = &bodies[..bodies.len().min(MAX_BODIES)];
        let aspect = w.max(1) as f32 / h.max(1) as f32;
        let vp = camera.view_proj(aspect).to_cols_array_2d();
        let eye = camera.eye();
        let slot = std::mem::size_of::<FrameUniform>() as u64;
        // slot 0 also serves the floor
        let count = bodies.len().max(1);
        for i in 0..count {
            let tint = bodies.get(i).map_or([0.7, 0.7, 0.72, 1.0], |b| b.tint);
            let u = FrameUniform {
                view_proj: vp,
                eye: [eye.x, eye.y, eye.z, 1.0],
                tint,
                _pad: [0.0; 40],
            };
            queue.write_buffer(&self.uniforms, slot * i as u64, bytemuck::bytes_of(&u));
            if let (Some(b), Some(jb)) = (bodies.get(i), self.joint_buffers.get(i)) {
                let mut mats: Vec<[[f32; 4]; 4]> = b.joints.iter().map(|m| m.to_cols_array_2d()).collect();
                mats.resize(self.joint_count, Mat4::IDENTITY.to_cols_array_2d());
                queue.write_buffer(jb, 0, bytemuck::cast_slice(&mats));
            }
        }
        self.ensure_target(device, w, h);
        let Some(t) = &self.target else {
            return Err("no render target".into());
        };
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("viewport"),
        });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("viewport"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.11,
                            g: 0.115,
                            b: 0.125,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(bg) = self.bind_groups.first() {
                pass.set_pipeline(&self.floor);
                pass.set_bind_group(0, bg, &[]);
                pass.draw(0..6, 0..1);
            }
            pass.set_vertex_buffer(0, self.vertices.slice(..));
            pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
            // the shown pose first (it writes depth), then the ghosts blended over
            for (i, (b, bg)) in bodies.iter().zip(&self.bind_groups).enumerate() {
                pass.set_pipeline(if i == 0 || b.tint[3] >= 1.0 {
                    &self.body
                } else {
                    &self.ghost
                });
                pass.set_bind_group(0, bg, &[]);
                pass.draw_indexed(0..self.index_count, 0, 0..1);
            }
        }
        queue.submit([enc.finish()]);
        Ok(&t.color_view)
    }

    pub fn target_size(&self) -> Option<(u32, u32)> {
        self.target.as_ref().map(|t| t.size)
    }

    /// The last rendered frame as tightly packed RGBA8 rows (blocks until the GPU is done).
    pub fn read_rgba(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<(u32, u32, Vec<u8>), String> {
        let t = self.target.as_ref().ok_or("nothing rendered yet")?;
        let (w, h) = t.size;
        let row = (w * 4).div_ceil(256) * 256;
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(row) * u64::from(h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            t.color.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([enc.finish()]);
        let slice = buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _sent = tx.send(r);
        });
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| format!("GPU wait failed: {e}"))?;
        rx.recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("readback failed: {e}"))?;
        let data = slice
            .get_mapped_range()
            .map_err(|e| format!("readback map failed: {e}"))?;
        let bgra = matches!(
            self.format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        );
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h as usize {
            let start = y * row as usize;
            let line = data
                .get(start..start + (w * 4) as usize)
                .ok_or("short readback")?;
            if bgra {
                for px in line.as_chunks::<4>().0 {
                    out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
                }
            } else {
                out.extend_from_slice(line);
            }
        }
        drop(data);
        buf.unmap();
        Ok((w, h, out))
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    pub fn color_texture(&self) -> Option<&wgpu::Texture> {
        self.target.as_ref().map(|t| &t.color)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod camera_tests {
    use super::{Camera, Vec3};

    /// Orthographic: a point anywhere along the view line lands on the same pixel (the target's,
    /// the picture's centre), rays are parallel, and the picture is as tall as the perspective
    /// one at the target: a point tan(0.35) x 3.6 m above the target is at the top edge.
    #[test]
    fn ortho_keeps_parallel_lines_parallel() {
        let cam = Camera {
            ortho: true,
            ..Camera::default()
        };
        let fwd = cam.forward();
        for depth in [-2.0, 0.0, 5.0] {
            let q = cam.project(cam.target + fwd * depth, 400.0, 300.0).unwrap();
            assert!(
                (q[0] - 200.0).abs() < 1e-3 && (q[1] - 150.0).abs() < 1e-3,
                "{q:?}"
            );
        }
        let (_, up) = cam.right_up();
        let top = cam
            .project(cam.target + up * 3.6 * 0.35f32.tan(), 400.0, 300.0)
            .unwrap();
        assert!(top[1].abs() < 1e-2, "{top:?}");
        let (_, a) = cam.ray(10.0, 10.0, 400.0, 300.0);
        let (_, b) = cam.ray(390.0, 290.0, 400.0, 300.0);
        assert!((a - b).length() < 1e-5 && (a - fwd).length() < 1e-5);
        // perspective rays spread
        let persp = Camera::default();
        let (_, a) = persp.ray(10.0, 10.0, 400.0, 300.0);
        let (_, b) = persp.ray(390.0, 290.0, 400.0, 300.0);
        assert!((a - b).length() > 0.1);
        assert!((persp.right_up().1 - Vec3::Z).length() < 0.2);
    }
}
