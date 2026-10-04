// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The OLS CAD renderer (ADR-0006): lines, points and surfaces drawn with
//! wgpu into an offscreen texture, which the application shows in its
//! views (or saves, [`render_rgba`]).
//!
//! - A [`Scene`] is geometry in project coordinates (`f64`), stored on the
//!   GPU relative to the scene's origin as `f32`: millimetre precision over
//!   tens of kilometres, even with LV95 or UTM coordinates of millions of
//!   metres.
//! - Scenes are uploaded once ([`GpuScene`]) and drawn as layers in any
//!   combination ([`Renderer::render`]).
//! - A [`Camera`] orbits a target point: drag to turn, pan, zoom — and
//!   projects points to the screen for picking ([`Camera::project`]).
//!
//! - Large layers are split into chunks with levels of detail, culled and
//!   chosen by distance every frame ([`lod`]).

pub mod lod;

use nalgebra::{Matrix4, Point3, Vector3};
use wgpu::util::DeviceExt;

/// Colour format of the offscreen texture. It can also be viewed as
/// [`wgpu::TextureFormat::Rgba8Unorm`] to read the sRGB-encoded bytes as
/// they are (as egui expects of its textures).
pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Samples per pixel (anti-aliasing of thin lines).
const SAMPLES: u32 = 4;

/// One vertex: position relative to the scene origin, colour (sRGB, alpha).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    /// Position relative to the scene origin, m.
    pub position: [f32; 3],
    /// Colour, 0–255 sRGB and alpha.
    pub color: [u8; 4],
}

/// One line segment (drawn as a quad `width` pixels wide).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Segment {
    /// Start, relative to the scene origin.
    pub a: [f32; 3],
    /// End, relative to the scene origin.
    pub b: [f32; 3],
    /// Colour, 0–255 sRGB and alpha.
    pub color: [u8; 4],
    /// Width, pixels.
    pub width: f32,
}

/// One point (drawn as a square `size` pixels wide).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Dot {
    /// Position, relative to the scene origin.
    pub position: [f32; 3],
    /// Colour, 0–255 sRGB and alpha.
    pub color: [u8; 4],
    /// Size, pixels.
    pub size: f32,
}

/// Plan and height bounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    /// Lowest x, y, z.
    pub min: [f64; 3],
    /// Highest x, y, z.
    pub max: [f64; 3],
}

impl Bounds {
    /// Centre.
    pub fn centre(&self) -> [f64; 3] {
        [0, 1, 2].map(|k| 0.5 * (self.min[k] + self.max[k]))
    }

    /// Half the diagonal.
    pub fn radius(&self) -> f64 {
        let d = [0, 1, 2].map(|k| self.max[k] - self.min[k]);
        0.5 * (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    }

    fn add(&mut self, p: [f64; 3]) {
        for (k, v) in p.into_iter().enumerate() {
            self.min[k] = self.min[k].min(v);
            self.max[k] = self.max[k].max(v);
        }
    }
}

/// Geometry of one layer, in project coordinates.
#[derive(Debug, Clone)]
pub struct Scene {
    origin: [f64; 3],
    lines: Vec<Segment>,
    points: Vec<Dot>,
    triangles: Vec<Vertex>,
    bounds: Option<Bounds>,
}

impl Scene {
    /// An empty scene whose positions are stored relative to `origin`.
    pub fn new(origin: [f64; 3]) -> Self {
        Self {
            origin,
            lines: Vec::new(),
            points: Vec::new(),
            triangles: Vec::new(),
            bounds: None,
        }
    }

    fn vertex(&mut self, p: [f64; 3], color: [u8; 4]) -> Vertex {
        match &mut self.bounds {
            Some(b) => b.add(p),
            None => self.bounds = Some(Bounds { min: p, max: p }),
        }
        Vertex {
            position: [0, 1, 2].map(|k| (p[k] - self.origin[k]) as f32),
            color,
        }
    }

    /// A polyline, `width` pixels wide.
    pub fn polyline(&mut self, points: &[[f64; 3]], color: [u8; 4], width: f32) {
        for w in points.windows(2) {
            let (a, b) = (self.vertex(w[0], color), self.vertex(w[1], color));
            self.lines.push(Segment {
                a: a.position,
                b: b.position,
                color,
                width,
            });
        }
    }

    /// A point, `size` pixels wide.
    pub fn point(&mut self, p: [f64; 3], color: [u8; 4], size: f32) {
        let v = self.vertex(p, color);
        self.points.push(Dot {
            position: v.position,
            color,
            size,
        });
    }

    /// A triangle (one colour per corner).
    pub fn triangle(&mut self, corners: [[f64; 3]; 3], colors: [[u8; 4]; 3]) {
        for (p, c) in corners.into_iter().zip(colors) {
            let v = self.vertex(p, c);
            self.triangles.push(v);
        }
    }

    /// Bounds of everything added.
    pub fn bounds(&self) -> Option<Bounds> {
        self.bounds
    }

    /// The origin positions are relative to.
    pub fn origin(&self) -> [f64; 3] {
        self.origin
    }

    /// Number of lines, points and triangles.
    pub fn counts(&self) -> (usize, usize, usize) {
        (
            self.lines.len(),
            self.points.len(),
            self.triangles.len() / 3,
        )
    }
}

/// An orbit camera: looks at `target` from `distance` m away, turned by
/// `yaw` (from east, counter-clockwise) and tilted by `pitch` (above the
/// horizon), radians. z is up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// Point looked at, project coordinates.
    pub target: [f64; 3],
    /// Distance from the target, m.
    pub distance: f64,
    /// Direction from the target to the eye in plan, radians from east.
    pub yaw: f64,
    /// Elevation of the eye above the target's horizon, radians.
    pub pitch: f64,
    /// Vertical field of view, radians.
    pub fov_y: f64,
    /// Vertical exaggeration of heights (1 = true scale).
    pub exaggeration: f64,
}

impl Camera {
    /// Looking at `bounds` from the south-west, from above.
    pub fn fit(bounds: &Bounds) -> Self {
        Self {
            target: bounds.centre(),
            distance: (bounds.radius() * 2.2).max(10.0),
            yaw: (-135f64).to_radians(),
            pitch: 35f64.to_radians(),
            fov_y: 45f64.to_radians(),
            exaggeration: 1.0,
        }
    }

    /// Eye position, project coordinates.
    pub fn eye(&self) -> [f64; 3] {
        let (cp, sp) = (self.pitch.cos(), self.pitch.sin());
        [
            self.target[0] + self.distance * cp * self.yaw.cos(),
            self.target[1] + self.distance * cp * self.yaw.sin(),
            self.target[2] + self.distance * sp,
        ]
    }

    /// Turns the camera: `dx`, `dy` in radians.
    pub fn orbit(&mut self, dx: f64, dy: f64) {
        self.yaw -= dx;
        self.pitch = (self.pitch + dy).clamp(-1.5, 1.55);
    }

    /// Moves the target by a drag of (`dx`, `dy`) pixels in a view `height`
    /// pixels high, in the screen plane.
    pub fn pan(&mut self, dx: f64, dy: f64, height: f64) {
        let m = 2.0 * self.distance * (0.5 * self.fov_y).tan() / height.max(1.0);
        let right = [-self.yaw.sin(), self.yaw.cos(), 0.0];
        let (cp, sp) = (self.pitch.cos(), self.pitch.sin());
        // Screen up: perpendicular to the view direction and to right.
        let up = [-sp * self.yaw.cos(), -sp * self.yaw.sin(), cp];
        for k in 0..3 {
            self.target[k] += m * (-dx * right[k] + dy * up[k]);
        }
    }

    /// Turns the view around the eye (first person): `dx`, `dy` in
    /// radians; the eye stays where it is, the target moves.
    pub fn look(&mut self, dx: f64, dy: f64) {
        let eye = self.eye();
        self.orbit(dx, -dy);
        let (cp, sp) = (self.pitch.cos(), self.pitch.sin());
        self.target = [
            eye[0] - self.distance * cp * self.yaw.cos(),
            eye[1] - self.distance * cp * self.yaw.sin(),
            eye[2] - self.distance * sp,
        ];
    }

    /// Moves the eye and the target together: `forward` and `right` along
    /// the ground in the direction of view, `up` vertically, metres.
    pub fn fly(&mut self, forward: f64, right: f64, up: f64) {
        let (c, s) = (self.yaw.cos(), self.yaw.sin());
        // Looking from the eye to the target: (−cos, −sin); right of it:
        // (−sin, cos).
        self.target[0] += -c * forward - s * right;
        self.target[1] += -s * forward + c * right;
        self.target[2] += up;
    }

    /// Zooms by `factor` (< 1 closer).
    pub fn zoom(&mut self, factor: f64) {
        self.distance = (self.distance * factor).clamp(0.5, 1e7);
    }

    fn view_proj_f64(&self, origin: [f64; 3], aspect: f64) -> Matrix4<f64> {
        let ex = self.exaggeration;
        // Heights scaled about the target, everything relative to origin.
        let rel = |p: [f64; 3]| {
            Point3::new(
                p[0] - origin[0],
                p[1] - origin[1],
                (p[2] - self.target[2]) * ex + self.target[2] - origin[2],
            )
        };
        let eye = {
            let e = self.eye();
            Point3::new(e[0] - origin[0], e[1] - origin[1], e[2] - origin[2])
        };
        let view = Matrix4::look_at_rh(&eye, &rel(self.target), &Vector3::z());
        let near = (self.distance * 1e-3).max(0.05);
        let far = self.distance * 50.0 + 1e4;
        // wgpu clip space: depth 0–1.
        let f = 1.0 / (0.5 * self.fov_y).tan();
        let proj = Matrix4::new(
            f / aspect,
            0.0,
            0.0,
            0.0,
            0.0,
            f,
            0.0,
            0.0,
            0.0,
            0.0,
            far / (near - far),
            near * far / (near - far),
            0.0,
            0.0,
            -1.0,
            0.0,
        );
        // Vertical exaggeration about the target height.
        let t = self.target[2] - origin[2];
        let scale = Matrix4::new(
            1.0,
            0.0,
            0.0,
            0.0, //
            0.0,
            1.0,
            0.0,
            0.0, //
            0.0,
            0.0,
            ex,
            t * (1.0 - ex), //
            0.0,
            0.0,
            0.0,
            1.0,
        );
        proj * view * scale
    }

    /// Screen position (pixels from the top left of a `size` view) and
    /// depth of a point, `None` behind the camera.
    pub fn project(&self, p: [f64; 3], size: [f64; 2]) -> Option<([f64; 2], f64)> {
        self.projector(size)(p)
    }

    /// The ray through screen position `at` (pixels from the top left of
    /// a `size` view): its start on the near plane and its unit direction,
    /// project coordinates (heights true, not exaggerated).
    pub fn ray(&self, at: [f64; 2], size: [f64; 2]) -> Option<([f64; 3], [f64; 3])> {
        let m = self.view_proj_f64(self.target, size[0] / size[1].max(1.0));
        let inv = m.try_inverse()?;
        let (x, y) = (at[0] / size[0] * 2.0 - 1.0, 1.0 - at[1] / size[1] * 2.0);
        let t = self.target;
        let point = |depth: f64| {
            let v = inv * nalgebra::Vector4::new(x, y, depth, 1.0);
            (v.w.abs() > 1e-12).then(|| [v.x / v.w + t[0], v.y / v.w + t[1], v.z / v.w + t[2]])
        };
        let (a, b) = (point(0.0)?, point(0.5)?);
        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let n = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        (n > 1e-12).then(|| (a, d.map(|v| v / n)))
    }

    /// [`Camera::project`] for many points: the matrix is made once.
    pub fn projector(&self, size: [f64; 2]) -> impl Fn([f64; 3]) -> Option<([f64; 2], f64)> {
        let m = self.view_proj_f64(self.target, size[0] / size[1].max(1.0));
        let t = self.target;
        move |p| {
            let v = m * nalgebra::Vector4::new(p[0] - t[0], p[1] - t[1], p[2] - t[2], 1.0);
            if v.w <= 1e-9 {
                return None;
            }
            let (x, y) = (v.x / v.w, v.y / v.w);
            Some(([(x + 1.0) * 0.5 * size[0], (1.0 - y) * 0.5 * size[1]], v.w))
        }
    }
}

/// A scene on the GPU.
#[derive(Debug)]
pub struct GpuScene {
    origin: [f64; 3],
    /// Each part in buffers no larger than the device allows.
    lines: Vec<(wgpu::Buffer, u32)>,
    points: Vec<(wgpu::Buffer, u32)>,
    triangles: Vec<(wgpu::Buffer, u32)>,
}

impl GpuScene {
    /// Segments, points and triangles on the GPU.
    pub fn counts(&self) -> (u32, u32, u32) {
        let n = |b: &Vec<(wgpu::Buffer, u32)>| b.iter().map(|b| b.1).sum::<u32>();
        (n(&self.lines), n(&self.points), n(&self.triangles) / 3)
    }

    /// Uploads a scene, each part split into buffers the device accepts
    /// (whole triangles in each).
    pub fn new(device: &wgpu::Device, scene: &Scene) -> Self {
        let max = device.limits().max_buffer_size.min(1 << 30) as usize;
        Self {
            origin: scene.origin,
            lines: buffer_pieces(device, &scene.lines, "lines", 1, max),
            points: buffer_pieces(device, &scene.points, "points", 1, max),
            triangles: buffer_pieces(device, &scene.triangles, "triangles", 3, max),
        }
    }
}

struct Targets {
    size: [u32; 2],
    msaa: wgpu::TextureView,
    depth: wgpu::TextureView,
    color: wgpu::Texture,
    view: wgpu::TextureView,
}

/// `v` in buffers of at most `max` bytes, each a multiple of `unit`
/// elements.
fn buffer_pieces<T: bytemuck::Pod>(
    device: &wgpu::Device,
    v: &[T],
    label: &str,
    unit: usize,
    max: usize,
) -> Vec<(wgpu::Buffer, u32)> {
    let per = ((max / std::mem::size_of::<T>().max(1)) / unit * unit).max(unit);
    v.chunks(per)
        .map(|c| {
            (
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: bytemuck::cast_slice(c),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                c.len() as u32,
            )
        })
        .collect()
}

/// Draws scenes into an offscreen colour texture.
pub struct Renderer {
    pipelines: [wgpu::RenderPipeline; 3],
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    targets: Option<Targets>,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer").finish_non_exhaustive()
    }
}

/// Background colour (sRGB).
pub const BACKGROUND: [u8; 3] = [24, 28, 34];

impl Renderer {
    /// Creates the pipelines.
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ols-render"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: 80,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("uniforms"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ols-render"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry: &str, layout: wgpu::VertexBufferLayout<'_>, bias: i32| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("ols-render"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    buffers: &[Some(layout)],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    // Surfaces pushed back a little: lines on them stay visible.
                    bias: wgpu::DepthBiasState {
                        constant: bias,
                        slope_scale: if bias != 0 { 1.0 } else { 0.0 },
                        clamp: 0.0,
                    },
                }),
                multisample: wgpu::MultisampleState {
                    count: SAMPLES,
                    ..Default::default()
                },
                multiview_mask: None,
                cache: None,
            })
        };
        let surface = pipeline(
            "vs_surface",
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Unorm8x4],
            },
            2,
        );
        let points = pipeline(
            "vs_point",
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Dot>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Unorm8x4, 2 => Float32],
            },
            0,
        );
        let lines = pipeline(
            "vs_line",
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Segment>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x3, 1 => Float32x3, 2 => Unorm8x4, 3 => Float32
                ],
            },
            0,
        );
        Self {
            pipelines: [surface, points, lines],
            uniform,
            bind_group,
            targets: None,
        }
    }

    fn targets(&mut self, device: &wgpu::Device, size: [u32; 2]) -> &Targets {
        if self.targets.as_ref().is_none_or(|t| t.size != size) {
            let texture = |format: wgpu::TextureFormat, samples, usage| {
                device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("ols-render"),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    // The colour texture can also be read as plain bytes
                    // (sRGB-encoded values; egui takes those).
                    view_formats: if format == COLOR_FORMAT {
                        &[wgpu::TextureFormat::Rgba8Unorm]
                    } else {
                        &[]
                    },
                })
            };
            let color = texture(
                COLOR_FORMAT,
                1,
                wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
            );
            let view = |t: &wgpu::Texture| t.create_view(&Default::default());
            self.targets = Some(Targets {
                size,
                msaa: view(&texture(
                    COLOR_FORMAT,
                    SAMPLES,
                    wgpu::TextureUsages::RENDER_ATTACHMENT,
                )),
                depth: view(&texture(
                    DEPTH_FORMAT,
                    SAMPLES,
                    wgpu::TextureUsages::RENDER_ATTACHMENT,
                )),
                view: view(&color),
                color,
            });
        }
        self.targets.as_ref().expect("made above")
    }

    /// Draws `layers` (surfaces first, then points, then lines) seen by
    /// `camera` into a `size` texture and returns it.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layers: &[&GpuScene],
        camera: &Camera,
        size: [u32; 2],
    ) -> &wgpu::Texture {
        let size = [size[0].max(1), size[1].max(1)];
        let aspect = f64::from(size[0]) / f64::from(size[1]);
        let mut encoder = device.create_command_encoder(&Default::default());
        // One origin per frame: the first layer's (all layers of a model
        // share it).
        let origin = layers.first().map_or(camera.target, |l| l.origin);
        let m = camera.view_proj_f64(origin, aspect);
        let mut data = [0f32; 20];
        for (k, v) in m.iter().enumerate() {
            data[k] = *v as f32;
        }
        data[16] = size[0] as f32;
        data[17] = size[1] as f32;
        queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&data));
        self.targets(device, size);
        let (pipelines, bind_group) = (&self.pipelines, &self.bind_group);
        let targets = self.targets.as_ref().expect("made above");
        {
            let bg = BACKGROUND.map(|c| {
                let c = f64::from(c) / 255.0;
                // sRGB → linear for the clear colour.
                if c <= 0.04045 {
                    c / 12.92
                } else {
                    ((c + 0.055) / 1.055).powf(2.4)
                }
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ols-render"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.msaa,
                    resolve_target: Some(&targets.view),
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: bg[0],
                            g: bg[1],
                            b: bg[2],
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, bind_group, &[]);
            for (k, pipeline) in pipelines.iter().enumerate() {
                pass.set_pipeline(pipeline);
                for layer in layers {
                    let part = match k {
                        0 => &layer.triangles,
                        1 => &layer.points,
                        _ => &layer.lines,
                    };
                    for (buffer, n) in part {
                        pass.set_vertex_buffer(0, buffer.slice(..));
                        if k == 0 {
                            pass.draw(0..*n, 0..1);
                        } else {
                            // A quad per point or segment instance.
                            pass.draw(0..6, 0..*n);
                        }
                    }
                }
            }
        }
        queue.submit([encoder.finish()]);
        &self.targets.as_ref().expect("made above").color
    }
}

/// A device without a window (tests, image export), `Err` when no GPU
/// adapter is available.
pub fn headless_device() -> Result<(wgpu::Device, wgpu::Queue), String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        ..Default::default()
    }))
    .map_err(|e| format!("no GPU adapter: {e}"))?;
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        .map_err(|e| format!("no GPU device: {e}"))
}

/// Renders layers headless and returns the image as RGBA rows (sRGB),
/// `width × height`.
pub fn render_rgba(
    scenes: &[&Scene],
    camera: &Camera,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    let (device, queue) = headless_device()?;
    let gpu: Vec<GpuScene> = scenes.iter().map(|s| GpuScene::new(&device, s)).collect();
    let refs: Vec<&GpuScene> = gpu.iter().collect();
    let mut renderer = Renderer::new(&device);
    let texture = renderer.render(&device, &queue, &refs, camera, [width, height]);
    // Rows padded to 256 bytes for the copy.
    let row = (4 * width).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: u64::from(row * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(height),
            },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| format!("GPU: {e}"))?;
    rx.recv()
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("readback: {e}"))?;
    let data = slice
        .get_mapped_range()
        .map_err(|e| format!("readback: {e}"))?;
    let mut out = Vec::with_capacity((4 * width * height) as usize);
    for y in 0..height {
        let start = (y * row) as usize;
        out.extend_from_slice(&data[start..start + 4 * width as usize]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
