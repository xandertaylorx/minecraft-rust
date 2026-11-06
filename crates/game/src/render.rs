//! wgpu renderer: world (opaque + translucent), block outline and HUD.
use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use glam::{DVec3, Mat4, Vec3};
use image::RgbaImage;
use wgpu::util::DeviceExt;

use crate::gui::{Batch, GuiAtlas, Kind, ATLAS_SIZE};
use crate::mesh::{MeshData, Vertex};

pub const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub const SAMPLES: u32 = 4;
const MIP_LEVELS: u32 = 5;

fn msaa_state() -> wgpu::MultisampleState { wgpu::MultisampleState { count: SAMPLES, mask: !0, alpha_to_coverage_enabled: false } }
const CHUNK_SLOTS: u64 = 4096;
const ALIGN: u64 = 256;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    fog_color: [f32; 4],
    params: [f32; 4],
    light_vp: [[f32; 4]; 4],
    sun: [f32; 4],
    ldir: [f32; 4],
    cam: [f32; 4],
    zenith: [f32; 4],
}
const SHADOW_SIZE: u32 = 2048;
const SHADOW_EXTENT: f64 = 64.0;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct HudVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

pub struct Atlas {
    pub image: RgbaImage,
    pub names: HashMap<String, u32>,
}

struct GpuMesh {
    vb: wgpu::Buffer,
    ib: wgpu::Buffer,
    n: u32,
}

#[derive(Default)]
struct ChunkGpu {
    opaque: Option<GpuMesh>,
    translucent: Option<GpuMesh>,
}

pub struct Camera {
    pub pos: DVec3,
    pub yaw: f32,
    pub pitch: f32,
    pub fov_deg: f32,
    pub roll: f32,
}

impl Camera {
    pub fn up(&self) -> Vec3 {
        if self.roll == 0.0 { return Vec3::Y; }
        let f = self.forward();
        let right = f.cross(Vec3::Y).normalize_or_zero();
        let (s, c) = self.roll.to_radians().sin_cos();
        (Vec3::Y * c + right * s).normalize()
    }
    pub fn forward(&self) -> Vec3 {
        let (sy, cy) = self.yaw.to_radians().sin_cos();
        let (sp, cp) = self.pitch.to_radians().sin_cos();
        Vec3::new(-sy * cp, -sp, cy * cp)
    }
}

pub struct WorldState {
    pub target: Option<[i32; 3]>,
    /// Block being mined and the atlas tile of its crack stage.
    pub crack: Option<([i32; 3], u32)>,
}

pub struct SkyState {
    pub sun_dir: [f32; 3],
    pub night: f32,
    pub zenith: [f32; 3],
    pub horizon: [f32; 3],
    pub cloud_color: [f32; 3],
    pub cloud_scroll: f32,
    pub cloud_height: f32,
    pub time_angle: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SkyUniform {
    inv_vp: [[f32; 4]; 4],
    view_proj: [[f32; 4]; 4],
    sun: [f32; 4],
    zenith: [f32; 4],
    horizon: [f32; 4],
    r_sun: [f32; 4],
    r_moon: [f32; 4],
    r_clouds: [f32; 4],
    cloud_color: [f32; 4],
    cam: [f32; 4],
    p: [f32; 4],
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub format: wgpu::TextureFormat,
    size: (u32, u32),
    depth: wgpu::TextureView,
    msaa: wgpu::TextureView,
    globals: wgpu::Buffer,
    globals_bg: wgpu::BindGroup,
    chunk_buf: wgpu::Buffer,
    chunk_bg: wgpu::BindGroup,
    opaque_pipe: wgpu::RenderPipeline,
    trans_pipe: wgpu::RenderPipeline,
    line_pipe: wgpu::RenderPipeline,
    hud_pipe: wgpu::RenderPipeline,
    hud_invert_pipe: wgpu::RenderPipeline,
    hud_bg: wgpu::BindGroup,
    hud_bg_linear: wgpu::BindGroup,
    sky_buf: wgpu::Buffer,
    sky_bg: wgpu::BindGroup,
    sky_pipe: wgpu::RenderPipeline,
    cloud_pipe: wgpu::RenderPipeline,
    cloud_vb: wgpu::Buffer,
    sky_rects: [[f32; 4]; 3],
    entity_bg: wgpu::BindGroup,
    hand_globals: wgpu::Buffer,
    hand_bg: wgpu::BindGroup,
    hand_pipe: wgpu::RenderPipeline,
    hand_mesh: Option<(wgpu::Buffer, wgpu::Buffer, u32)>,
    atlas_tex: wgpu::Texture,
    ent_tex: wgpu::Texture,
    entity_mesh: Option<(wgpu::Buffer, wgpu::Buffer, u32)>,
    crack_pipe: wgpu::RenderPipeline,
    shadow_pipe: wgpu::RenderPipeline,
    shadow_view: wgpu::TextureView,
    shadow_buf: wgpu::Buffer,
    shadow_bg: wgpu::BindGroup,
    chunks: HashMap<(i32, i32), ChunkGpu>,
    pub sky: [f32; 3],
    pub day: f32,
    pub fog_end: f32,
    pub gamma: f32,
    /// (start, end) fog distances overriding the render-distance fog (underwater).
    pub fog_override: Option<(f32, f32)>,
}

fn make_texture_handle(device: &wgpu::Device, queue: &wgpu::Queue, img: &RgbaImage, label: &str) -> (wgpu::Texture, wgpu::TextureView) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: img.width(), height: img.height(), depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        img.as_raw(),
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * img.width()), rows_per_image: Some(img.height()) },
        wgpu::Extent3d { width: img.width(), height: img.height(), depth_or_array_layers: 1 },
    );
    let v = tex.create_view(&Default::default());
    (tex, v)
}

fn make_texture(device: &wgpu::Device, queue: &wgpu::Queue, img: &RgbaImage, label: &str) -> wgpu::TextureView {
    let tex = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: img.width(), height: img.height(), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        img,
    );
    tex.create_view(&Default::default())
}

/// Per-tile mip chain (so neighbouring tiles never bleed into each other), 16px tiles in a 16x16 grid.
fn atlas_mips(img: &RgbaImage) -> Vec<RgbaImage> {
    let mut levels = vec![img.clone()];
    for l in 1..MIP_LEVELS {
        let size = img.width() >> l;
        let tile = 16u32 >> l;
        let rows = img.height() / 16;
        let prev = &levels[(l - 1) as usize];
        let mut out = RgbaImage::new(size, img.height() >> l);
        for ty in 0..rows {
            for tx in 0..16 {
                for y in 0..tile {
                    for x in 0..tile {
                        let (px, py) = (tx * (tile * 2) + x * 2, ty * (tile * 2) + y * 2);
                        let (mut c, mut a) = ([0f32; 3], 0f32);
                        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                            let p = prev.get_pixel(px + dx, py + dy);
                            let w = p[3] as f32;
                            for i in 0..3 { c[i] += p[i] as f32 * w; }
                            a += w;
                        }
                        let px_out = if a > 0.0 { [(c[0] / a) as u8, (c[1] / a) as u8, (c[2] / a) as u8, (a / 4.0) as u8] } else { [0, 0, 0, 0] };
                        out.put_pixel(tx * tile + x, ty * tile + y, image::Rgba(px_out));
                    }
                }
            }
        }
        levels.push(out);
    }
    levels
}

fn make_texture_mips(device: &wgpu::Device, queue: &wgpu::Queue, levels: &[RgbaImage], label: &str) -> (wgpu::Texture, wgpu::TextureView) {
    let base = &levels[0];
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: base.width(), height: base.height(), depth_or_array_layers: 1 },
        mip_level_count: levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (i, img) in levels.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: i as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            img.as_raw(),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * img.width()), rows_per_image: Some(img.height()) },
            wgpu::Extent3d { width: img.width(), height: img.height(), depth_or_array_layers: 1 },
        );
    }
    let v = tex.create_view(&Default::default());
    (tex, v)
}

fn msaa_view(device: &wgpu::Device, format: wgpu::TextureFormat, w: u32, h: u32) -> wgpu::TextureView {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("msaa"),
        size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: SAMPLES, dimension: wgpu::TextureDimension::D2, format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[],
    }).create_view(&Default::default())
}

fn depth_view(device: &wgpu::Device, w: u32, h: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: SAMPLES,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

impl Renderer {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, format: wgpu::TextureFormat, size: (u32, u32), atlas: &Atlas, hud: &GuiAtlas, ent_image: &RgbaImage) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Linear,
            lod_max_clamp: (MIP_LEVELS - 1) as f32,
            ..Default::default()
        });
        let (atlas_tex, atlas_view) = make_texture_mips(&device, &queue, &atlas_mips(&atlas.image), "atlas");
        let hud_view = make_texture(&device, &queue, &hud.image, "hud");
        let rect_uv = |n: &str| { let r = hud.sprites[n]; [r.x / ATLAS_SIZE as f32, r.y / ATLAS_SIZE as f32, r.w / ATLAS_SIZE as f32, r.h / ATLAS_SIZE as f32] };
        let sky_rects = [rect_uv("sun"), rect_uv("moon"), rect_uv("clouds")];

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let chunk_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("chunk offsets"),
            size: CHUNK_SLOTS * ALIGN,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let tex_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let samp_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                tex_entry(1),
                samp_entry(2),
                wgpu::BindGroupLayoutEntry {
                    binding: 3, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison), count: None,
                },
            ],
        });
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0, visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None,
                },
                tex_entry(1),
                samp_entry(2),
            ],
        });
        let shadow_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow"), size: wgpu::Extent3d { width: SHADOW_SIZE, height: SHADOW_SIZE, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
        });
        let shadow_view = shadow_tex.create_view(&Default::default());
        let shadow_samp = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual), ..Default::default()
        });
        let shadow_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow u"), size: 64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let chunk_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: None },
                count: None,
            }],
        });
        let hud_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[tex_entry(0), samp_entry(1)],
        });
        let (ent_tex, ent_view) = make_texture_handle(&device, &queue, ent_image, "entities");
        let globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &globals_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&atlas_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&shadow_view) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&shadow_samp) },
            ],
        });
        let chunk_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &chunk_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &chunk_buf, offset: 0, size: wgpu::BufferSize::new(16) }),
            }],
        });
        let entity_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None, layout: &globals_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&ent_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&shadow_view) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&shadow_samp) },
            ],
        });
        let hand_globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hand globals"), size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let hand_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None, layout: &globals_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: hand_globals.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&ent_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&shadow_view) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&shadow_samp) },
            ],
        });
        let sampler_lin = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default()
        });
        let hud_bg_linear = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None, layout: &hud_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&hud_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler_lin) },
            ],
        });
        let hud_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &hud_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&hud_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });

        let world_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("world"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let hud_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hud"),
            source: wgpu::ShaderSource::Wgsl(include_str!("hud.wgsl").into()),
        });
        let world_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&globals_layout, &chunk_layout],
            push_constant_ranges: &[],
        });
        let hud_pl_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&hud_layout],
            push_constant_ranges: &[],
        });
        let vattrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x4, 3 => Float32x2];
        let vbuf = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &vattrs,
        };
        let depth_state = |write: bool| wgpu::DepthStencilState {
            format: DEPTH,
            depth_write_enabled: write,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: Default::default(),
            bias: Default::default(),
        };
        let world_pipe = |label: &str, blend: Option<wgpu::BlendState>, write: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&world_layout),
                vertex: wgpu::VertexState {
                    module: &world_shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[vbuf.clone()],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &world_shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(depth_state(write)),
                multisample: msaa_state(),
                multiview: None,
                cache: None,
            })
        };
        let opaque_pipe = world_pipe("opaque", None, true);
        let trans_pipe = world_pipe("translucent", Some(wgpu::BlendState::ALPHA_BLENDING), false);
        let line_attrs = wgpu::vertex_attr_array![0 => Float32x3];
        let line_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lines"),
            layout: Some(&world_layout),
            vertex: wgpu::VertexState {
                module: &world_shader,
                entry_point: Some("vs_line"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout { array_stride: 12, step_mode: wgpu::VertexStepMode::Vertex, attributes: &line_attrs }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &world_shader,
                entry_point: Some("fs_line"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
            }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::LineList, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { bias: wgpu::DepthBiasState { constant: -2, slope_scale: -1.0, clamp: 0.0 }, ..depth_state(false) }),
            multisample: msaa_state(),
            multiview: None,
            cache: None,
        });
        let hud_attrs = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];
        let hud_pipe_for = |blend: wgpu::BlendState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("hud"),
                layout: Some(&hud_pl_layout),
                vertex: wgpu::VertexState {
                    module: &hud_shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<HudVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &hud_attrs,
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &hud_shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend: Some(blend), write_mask: wgpu::ColorWrites::ALL })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview: None,
                cache: None,
            })
        };
        let hud_pipe = hud_pipe_for(wgpu::BlendState::ALPHA_BLENDING);
        // Vanilla's crosshair inverts the colour behind it.
        let hud_invert_pipe = hud_pipe_for(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::OneMinusDst,
                dst_factor: wgpu::BlendFactor::OneMinusSrc,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::OVER,
        });

        // ---- sky + clouds
        let sky_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky"), size: std::mem::size_of::<SkyUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let sky_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let sky_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None, layout: &sky_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: sky_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&hud_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("sky"), source: wgpu::ShaderSource::Wgsl(include_str!("sky.wgsl").into()) });
        let sky_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&sky_layout], push_constant_ranges: &[] });
        let no_depth = wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: false, depth_compare: wgpu::CompareFunction::Always, stencil: Default::default(), bias: Default::default() };
        let sky_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky"), layout: Some(&sky_pl),
            vertex: wgpu::VertexState { module: &sky_shader, entry_point: Some("vs_sky"), compilation_options: Default::default(), buffers: &[] },
            fragment: Some(wgpu::FragmentState { module: &sky_shader, entry_point: Some("fs_sky"), compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })] }),
            primitive: Default::default(), depth_stencil: Some(no_depth), multisample: msaa_state(), multiview: None, cache: None,
        });
        let cloud_attrs = wgpu::vertex_attr_array![0 => Float32x3];
        let cloud_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("clouds"), layout: Some(&sky_pl),
            vertex: wgpu::VertexState { module: &sky_shader, entry_point: Some("vs_cloud"), compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout { array_stride: 12, step_mode: wgpu::VertexStepMode::Vertex, attributes: &cloud_attrs }] },
            fragment: Some(wgpu::FragmentState { module: &sky_shader, entry_point: Some("fs_cloud"), compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }),
            primitive: Default::default(), depth_stencil: Some(depth_state(false)), multisample: msaa_state(), multiview: None, cache: None,
        });
        let big = 4000.0f32;
        let cloud_vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&[[-big, 0.0f32, -big], [big, 0.0, -big], [big, 0.0, big], [-big, 0.0, -big], [big, 0.0, big], [-big, 0.0, big]]),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let crack_pipe = world_pipe("crack", Some(wgpu::BlendState::ALPHA_BLENDING), false);

        let hand_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hand"),
            layout: Some(&world_layout),
            vertex: wgpu::VertexState { module: &world_shader, entry_point: Some("vs_hand"), compilation_options: Default::default(), buffers: &[vbuf.clone()] },
            fragment: Some(wgpu::FragmentState { module: &world_shader, entry_point: Some("fs"), compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })] }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(depth_state(true)),
            multisample: msaa_state(), multiview: None, cache: None,
        });

        let shadow_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shadow"), source: wgpu::ShaderSource::Wgsl(include_str!("shadow.wgsl").into()),
        });
        let shadow_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&shadow_layout, &chunk_layout], push_constant_ranges: &[] });
        let shadow_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow"), layout: Some(&shadow_pl),
            vertex: wgpu::VertexState { module: &shadow_shader, entry_point: Some("vs_shadow"), compilation_options: Default::default(), buffers: &[vbuf.clone()] },
            fragment: Some(wgpu::FragmentState { module: &shadow_shader, entry_point: Some("fs_shadow"), compilation_options: Default::default(), targets: &[] }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::LessEqual, stencil: Default::default(),
                bias: wgpu::DepthBiasState { constant: 2, slope_scale: 2.5, clamp: 0.0 },
            }),
            multisample: Default::default(), multiview: None, cache: None,
        });
        let shadow_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None, layout: &shadow_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: shadow_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&atlas_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        let depth = depth_view(&device, size.0, size.1);
        let msaa = msaa_view(&device, format, size.0, size.1);
        Self {
            device, queue, format, size, depth, msaa, globals, globals_bg, chunk_buf, chunk_bg,
            opaque_pipe, trans_pipe, line_pipe, hud_pipe, hud_invert_pipe, hud_bg, hud_bg_linear, entity_bg, hand_globals, hand_bg, hand_pipe, hand_mesh: None, atlas_tex, ent_tex, entity_mesh: None, sky_buf, sky_bg, sky_pipe, cloud_pipe, cloud_vb, sky_rects, crack_pipe, shadow_pipe, shadow_view, shadow_buf, shadow_bg,
            chunks: HashMap::new(),
            sky: [0.47, 0.65, 1.0],
            day: 1.0,
            fog_end: 160.0,
            gamma: 0.5,
            fog_override: None,
        }
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        self.size = (w.max(1), h.max(1));
        self.depth = depth_view(&self.device, self.size.0, self.size.1);
        self.msaa = msaa_view(&self.device, self.format, self.size.0, self.size.1);
    }

    pub fn set_chunk(&mut self, key: (i32, i32), data: MeshData) {
        let mut up = |(v, i): (Vec<Vertex>, Vec<u32>)| -> Option<GpuMesh> {
            if i.is_empty() { return None; }
            Some(GpuMesh {
                vb: self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None, contents: bytemuck::cast_slice(&v), usage: wgpu::BufferUsages::VERTEX,
                }),
                ib: self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None, contents: bytemuck::cast_slice(&i), usage: wgpu::BufferUsages::INDEX,
                }),
                n: i.len() as u32,
            })
        };
        let g = ChunkGpu { opaque: up(data.opaque), translucent: up(data.translucent) };
        self.chunks.insert(key, g);
    }

    /// Replace one 16x16 animation frame in the block atlas (all mip levels) and the entity atlas copy.
    pub fn update_tile(&mut self, tile: u32, img: &RgbaImage) {
        let (tx, ty) = (tile % 16, tile / 16);
        let mut cur = img.clone();
        for l in 0..MIP_LEVELS {
            let size = 16u32 >> l;
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &self.atlas_tex, mip_level: l, origin: wgpu::Origin3d { x: tx * size, y: ty * size, z: 0 }, aspect: wgpu::TextureAspect::All },
                cur.as_raw(),
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * size), rows_per_image: Some(size) },
                wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
            );
            if l == 0 {
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo { texture: &self.ent_tex, mip_level: 0, origin: wgpu::Origin3d { x: tx * 16, y: ty * 16, z: 0 }, aspect: wgpu::TextureAspect::All },
                    cur.as_raw(),
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(64), rows_per_image: Some(16) },
                    wgpu::Extent3d { width: 16, height: 16, depth_or_array_layers: 1 },
                );
            }
            let n = size / 2;
            if n == 0 { break; }
            let mut next = RgbaImage::new(n, n);
            for y in 0..n { for x in 0..n {
                let (mut c3, mut a) = ([0f32; 3], 0f32);
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let p = cur.get_pixel(x * 2 + dx, y * 2 + dy);
                    let w = p[3] as f32;
                    for i in 0..3 { c3[i] += p[i] as f32 * w; }
                    a += w;
                }
                next.put_pixel(x, y, image::Rgba(if a > 0.0 { [(c3[0] / a) as u8, (c3[1] / a) as u8, (c3[2] / a) as u8, (a / 4.0) as u8] } else { [0; 4] }));
            } }
            cur = next;
        }
    }

    pub fn set_hand(&mut self, v: &[Vertex], i: &[u32]) {
        self.hand_mesh = if i.is_empty() { None } else {
            Some((
                self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(v), usage: wgpu::BufferUsages::VERTEX }),
                self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(i), usage: wgpu::BufferUsages::INDEX }),
                i.len() as u32,
            ))
        };
    }

    pub fn set_entities(&mut self, v: &[Vertex], i: &[u32]) {
        self.entity_mesh = if i.is_empty() { None } else {
            Some((
                self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(v), usage: wgpu::BufferUsages::VERTEX }),
                self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(i), usage: wgpu::BufferUsages::INDEX }),
                i.len() as u32,
            ))
        };
    }

    pub fn drop_chunk(&mut self, key: (i32, i32)) { self.chunks.remove(&key); }
    pub fn chunk_count(&self) -> usize { self.chunks.len() }
    pub fn has_chunk(&self, key: (i32, i32)) -> bool { self.chunks.contains_key(&key) }

    pub fn render(
        &mut self, target: &wgpu::TextureView, cam: &Camera, ws: &WorldState, sky: &SkyState,
        gui: &[Batch], visible: &dyn Fn((i32, i32)) -> bool,
    ) {
        let aspect = self.size.0 as f32 / self.size.1 as f32;
        let proj = Mat4::perspective_rh(cam.fov_deg.to_radians(), aspect, 0.05, 4000.0);
        let view = Mat4::look_to_rh(Vec3::ZERO, cam.forward(), cam.up());
        let vp = proj * view;
        // Sun shadow map: an orthographic projection along the sun direction, snapped to the texel grid.
        let s3 = Vec3::from(sky.sun_dir);
        let sd = Vec3::new(s3.x, s3.y.max(0.16), s3.z).normalize();
        let sun_str = ((s3.y - 0.02) / 0.23).clamp(0.0, 1.0);
        let sun_str = sun_str * sun_str * (3.0 - 2.0 * sun_str);
        let dirv = -sd;
        let upv = if sd.y.abs() > 0.95 { Vec3::X } else { Vec3::Y };
        let right = dirv.cross(upv).normalize();
        let up2 = right.cross(dirv).normalize();
        let fwd = cam.forward();
        let hf = DVec3::new(fwd.x as f64, 0.0, fwd.z as f64).normalize_or_zero();
        let center = cam.pos + hf * 28.0;
        let texel = SHADOW_EXTENT * 2.0 / SHADOW_SIZE as f64;
        let (rd, ud) = (DVec3::new(right.x as f64, right.y as f64, right.z as f64), DVec3::new(up2.x as f64, up2.y as f64, up2.z as f64));
        let (a, b) = (center.dot(rd), center.dot(ud));
        let center = center + rd * ((a / texel).round() * texel - a) + ud * ((b / texel).round() * texel - b);
        let crel = (center - cam.pos).as_vec3();
        let lview = Mat4::look_to_rh(crel + sd * 150.0, dirv, upv);
        let lproj = Mat4::orthographic_rh(-SHADOW_EXTENT as f32, SHADOW_EXTENT as f32, -SHADOW_EXTENT as f32, SHADOW_EXTENT as f32, 0.1, 320.0);
        let light_vp = (lproj * lview).to_cols_array_2d();
        self.queue.write_buffer(&self.shadow_buf, 0, bytemuck::bytes_of(&light_vp));
        let underwater = if self.fog_override.is_some() { 1.0 } else { 0.0 };
        self.queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&Globals {
            view_proj: vp.to_cols_array_2d(),
            fog_color: [sky.horizon[0], sky.horizon[1], sky.horizon[2], 1.0],
            params: [self.day, self.fog_override.map(|f| f.0).unwrap_or(self.fog_end * 0.55), self.fog_override.map(|f| f.1).unwrap_or(self.fog_end), self.gamma],
            light_vp,
            sun: [sky.sun_dir[0], sky.sun_dir[1], sky.sun_dir[2], sky.cloud_scroll / 1.2],
            ldir: [sd.x, sd.y, sd.z, sun_str],
            cam: [cam.pos.x as f32, cam.pos.y as f32, cam.pos.z as f32, underwater],
            zenith: [sky.zenith[0], sky.zenith[1], sky.zenith[2], 1.0],
        }));
        let hand_proj = Mat4::perspective_rh(cam.fov_deg.to_radians(), aspect, 0.05, 100.0);
        self.queue.write_buffer(&self.hand_globals, 0, bytemuck::bytes_of(&Globals {
            view_proj: hand_proj.to_cols_array_2d(),
            fog_color: [0.0, 0.0, 0.0, 1.0],
            params: [self.day.max(0.55), 10000.0, 20000.0, self.gamma],
            light_vp, sun: [0.0, 1.0, 0.0, 0.0], ldir: [0.0, 1.0, 0.0, 0.0], cam: [0.0; 4], zenith: [0.0; 4],
        }));
        let su = SkyUniform {
            inv_vp: vp.inverse().to_cols_array_2d(),
            view_proj: vp.to_cols_array_2d(),
            sun: [sky.sun_dir[0], sky.sun_dir[1], sky.sun_dir[2], sky.night],
            zenith: [sky.zenith[0], sky.zenith[1], sky.zenith[2], 1.0],
            horizon: [sky.horizon[0], sky.horizon[1], sky.horizon[2], 1.0],
            r_sun: self.sky_rects[0], r_moon: self.sky_rects[1], r_clouds: self.sky_rects[2],
            cloud_color: [sky.cloud_color[0], sky.cloud_color[1], sky.cloud_color[2], 1.0],
            cam: [cam.pos.x as f32, cam.pos.y as f32, cam.pos.z as f32, sky.cloud_scroll],
            p: [sky.cloud_height, self.fog_end * 0.55, self.fog_end, sky.time_angle],
        };
        self.queue.write_buffer(&self.sky_buf, 0, bytemuck::bytes_of(&su));

        // Per-chunk camera-relative offsets (slot order = draw order).
        let mut keys: Vec<(i32, i32)> = self.chunks.keys().copied().filter(|k| visible(*k)).collect();
        keys.truncate(CHUNK_SLOTS as usize - 3);
        let cam_chunk = ((cam.pos.x / 16.0).floor() as i32, (cam.pos.z / 16.0).floor() as i32);
        keys.sort_by_key(|k| (k.0 - cam_chunk.0).pow(2) + (k.1 - cam_chunk.1).pow(2));
        let extra: Vec<(i32, i32)> = self.chunks.keys().copied().filter(|k| !keys.contains(k) && (k.0 - cam_chunk.0).abs() <= 6 && (k.1 - cam_chunk.1).abs() <= 6).collect();
        let mut data = vec![0u8; (keys.len() + 3 + extra.len()) * ALIGN as usize];
        let mut put = |slot: usize, o: [f32; 4]| {
            data[slot * ALIGN as usize..slot * ALIGN as usize + 16].copy_from_slice(bytemuck::bytes_of(&o));
        };
        for (i, k) in keys.iter().enumerate() {
            let rel = DVec3::new(k.0 as f64 * 16.0 - cam.pos.x, -cam.pos.y, k.1 as f64 * 16.0 - cam.pos.z);
            put(i, [rel.x as f32, rel.y as f32, rel.z as f32, 0.0]);
        }
        for (i, k) in extra.iter().enumerate() {
            let rel = DVec3::new(k.0 as f64 * 16.0 - cam.pos.x, -cam.pos.y, k.1 as f64 * 16.0 - cam.pos.z);
            put(keys.len() + 3 + i, [rel.x as f32, rel.y as f32, rel.z as f32, 0.0]);
        }
        let line_slot = keys.len();
        let crack_slot = keys.len() + 1;
        for (slot, b) in [(line_slot, ws.target), (crack_slot, ws.crack.map(|c| c.0))] {
            if let Some(t) = b {
                let rel = DVec3::new(t[0] as f64, t[1] as f64, t[2] as f64) - cam.pos;
                put(slot, [rel.x as f32, rel.y as f32, rel.z as f32, 0.0]);
            }
        }
        self.queue.write_buffer(&self.chunk_buf, 0, &data);

        let outline = ws.target.map(|_| {
            let e = 0.002f32;
            let (a, b) = (-e, 1.0 + e);
            let c = [[a, a, a], [b, a, a], [b, a, b], [a, a, b], [a, b, a], [b, b, a], [b, b, b], [a, b, b]];
            let edges = [(0, 1), (1, 2), (2, 3), (3, 0), (4, 5), (5, 6), (6, 7), (7, 4), (0, 4), (1, 5), (2, 6), (3, 7)];
            let v: Vec<[f32; 3]> = edges.iter().flat_map(|&(i, j)| [c[i], c[j]]).collect();
            self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&v), usage: wgpu::BufferUsages::VERTEX })
        });
        let crack = ws.crack.map(|(_, tile)| {
            let mut verts: Vec<Vertex> = Vec::new();
            let e = 0.003f32;
            let faces: [([[f32; 3]; 4], f32); 6] = [
                ([[0., 1., 0.], [1., 1., 0.], [1., 1., 1.], [0., 1., 1.]], 1.0), ([[0., 0., 0.], [1., 0., 0.], [1., 0., 1.], [0., 0., 1.]], 1.0),
                ([[1., 1., 1.], [1., 1., 0.], [1., 0., 0.], [1., 0., 1.]], 1.0), ([[0., 1., 0.], [0., 1., 1.], [0., 0., 1.], [0., 0., 0.]], 1.0),
                ([[0., 1., 1.], [1., 1., 1.], [1., 0., 1.], [0., 0., 1.]], 1.0), ([[1., 1., 0.], [0., 1., 0.], [0., 0., 0.], [1., 0., 0.]], 1.0),
            ];
            let uvs = [[0.0f32, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
            let (tx, ty) = ((tile % 16) as f32, (tile / 16) as f32);
            for (corners, _) in faces {
                let q: Vec<Vertex> = corners.iter().zip(uvs).map(|(c, uv)| Vertex {
                    pos: [c[0] + (c[0] * 2.0 - 1.0) * e, c[1] + (c[1] * 2.0 - 1.0) * e, c[2] + (c[2] * 2.0 - 1.0) * e],
                    uv: [(tx + uv[0]) / 16.0, (ty + uv[1]) / crate::ATLAS_ROWS as f32], color: [1.0; 4], light: [15.0, 0.0],
                }).collect();
                for i in [0, 1, 2, 0, 2, 3] { verts.push(q[i]); }
            }
            (self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&verts), usage: wgpu::BufferUsages::VERTEX }), verts.len() as u32)
        });
        let gui_bufs: Vec<(Kind, wgpu::Buffer, u32)> = gui.iter().filter(|b| !b.verts.is_empty()).map(|b| (b.kind, self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&b.verts), usage: wgpu::BufferUsages::VERTEX }), b.verts.len() as u32)).collect();
        let mut enc = self.device.create_command_encoder(&Default::default());
        if sun_str > 0.0 {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow"), color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None, occlusion_query_set: None,
            });
            pass.set_pipeline(&self.shadow_pipe);
            pass.set_bind_group(0, &self.shadow_bg, &[]);
            let all = keys.iter().enumerate().map(|(i, k)| (i, *k)).chain(extra.iter().enumerate().map(|(i, k)| (keys.len() + 3 + i, *k)));
            for (slot, k) in all {
                let Some(m) = self.chunks[&k].opaque.as_ref() else { continue };
                pass.set_bind_group(1, &self.chunk_bg, &[(slot as u64 * ALIGN) as u32]);
                pass.set_vertex_buffer(0, m.vb.slice(..));
                pass.set_index_buffer(m.ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..m.n, 0, 0..1);
            }
        } else {
            // No sun: clear to "far" so every lookup reads as lit.
            let _ = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow clear"), color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None, occlusion_query_set: None,
            });
        }
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("world"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.msaa, resolve_target: Some(target),
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: sky.horizon[0] as f64, g: sky.horizon[1] as f64, b: sky.horizon[2] as f64, a: 1.0 }), store: wgpu::StoreOp::Discard },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None, occlusion_query_set: None,
            });
            pass.set_pipeline(&self.sky_pipe);
            pass.set_bind_group(0, &self.sky_bg, &[]);
            pass.draw(0..3, 0..1);

            pass.set_bind_group(0, &self.globals_bg, &[]);
            pass.set_pipeline(&self.opaque_pipe);
            for (i, k) in keys.iter().enumerate() {
                let Some(m) = self.chunks[k].opaque.as_ref() else { continue };
                pass.set_bind_group(1, &self.chunk_bg, &[(i as u64 * ALIGN) as u32]);
                pass.set_vertex_buffer(0, m.vb.slice(..));
                pass.set_index_buffer(m.ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..m.n, 0, 0..1);
            }
            if let Some((vb, ib, n)) = &self.entity_mesh {
                pass.set_pipeline(&self.opaque_pipe);
                pass.set_bind_group(0, &self.entity_bg, &[]);
                pass.set_bind_group(1, &self.chunk_bg, &[((keys.len() + 2) as u64 * ALIGN) as u32]);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..*n, 0, 0..1);
                pass.set_bind_group(0, &self.globals_bg, &[]);
            }
            // Clouds sit behind translucent water but in front of the sky.
            if sky.cloud_color[0] > 0.0 {
                pass.set_pipeline(&self.cloud_pipe);
                pass.set_bind_group(0, &self.sky_bg, &[]);
                pass.set_vertex_buffer(0, self.cloud_vb.slice(..));
                pass.draw(0..6, 0..1);
                pass.set_bind_group(0, &self.globals_bg, &[]);
            }
            pass.set_pipeline(&self.trans_pipe);
            for (i, k) in keys.iter().enumerate().rev() {
                let Some(m) = self.chunks[k].translucent.as_ref() else { continue };
                pass.set_bind_group(1, &self.chunk_bg, &[(i as u64 * ALIGN) as u32]);
                pass.set_vertex_buffer(0, m.vb.slice(..));
                pass.set_index_buffer(m.ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..m.n, 0, 0..1);
            }
            if let Some((vb, n)) = &crack {
                pass.set_pipeline(&self.crack_pipe);
                pass.set_bind_group(1, &self.chunk_bg, &[(crack_slot as u64 * ALIGN) as u32]);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.draw(0..*n, 0..1);
            }
            if let Some((vb, ib, n)) = &self.hand_mesh {
                pass.set_pipeline(&self.hand_pipe);
                pass.set_bind_group(0, &self.hand_bg, &[]);
                pass.set_bind_group(1, &self.chunk_bg, &[((keys.len() + 2) as u64 * ALIGN) as u32]);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..*n, 0, 0..1);
                pass.set_bind_group(0, &self.globals_bg, &[]);
            }
            if let Some(vb) = &outline {
                pass.set_pipeline(&self.line_pipe);
                pass.set_bind_group(1, &self.chunk_bg, &[(line_slot as u64 * ALIGN) as u32]);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.draw(0..24, 0..1);
            }
        }
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target, resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
            });
            for (kind, buf, n) in &gui_bufs {
                match kind {
                    Kind::Normal => { pass.set_pipeline(&self.hud_pipe); pass.set_bind_group(0, &self.hud_bg, &[]); }
                    Kind::Invert => { pass.set_pipeline(&self.hud_invert_pipe); pass.set_bind_group(0, &self.hud_bg, &[]); }
                    Kind::Icon => { pass.set_pipeline(&self.hud_pipe); pass.set_bind_group(0, &self.hud_bg_linear, &[]); }
                }
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..*n, 0..1);
            }
        }
        self.queue.submit([enc.finish()]);
    }

    /// Render to an offscreen RGBA image (for tests and screenshots).
    pub fn screenshot(&mut self, cam: &Camera, ws: &WorldState, sky: &SkyState, gui: &[Batch]) -> RgbaImage {
        let (w, h) = self.size;
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shot"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        self.render(&view, cam, ws, sky, gui, &|_| true);
        let bpr = (w * 4).div_ceil(256) * 256;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None, size: (bpr * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit([enc.finish()]);
        let slice = buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::Maintain::Wait);
        let data = slice.get_mapped_range();
        let mut img = RgbaImage::new(w, h);
        for y in 0..h {
            let row = &data[(y * bpr) as usize..(y * bpr + w * 4) as usize];
            for x in 0..w {
                let p = &row[x as usize * 4..x as usize * 4 + 4];
                let (r, g, b) = if matches!(self.format, wgpu::TextureFormat::Bgra8UnormSrgb | wgpu::TextureFormat::Bgra8Unorm) { (p[2], p[1], p[0]) } else { (p[0], p[1], p[2]) };
                img.put_pixel(x, y, image::Rgba([r, g, b, 255]));
            }
        }
        img
    }
}
