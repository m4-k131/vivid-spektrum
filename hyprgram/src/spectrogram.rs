use crate::dev::SpectrogramDevConfig;
use iced::mouse::{Cursor, Interaction};
use iced::widget::shader;
use iced::wgpu;
use iced::Rectangle;
use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex};

const WGSL: &str = r#"
struct Uniforms {
    scroll: f32,
    tex_w: f32,
    tex_h: f32,
    mode: u32,
    contrast: f32,
    saturation: f32,
    _pad0: f32,
    _pad1: f32,
}
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;
@group(0) @binding(3) var cmap_tex: texture_2d<f32>;
@group(0) @binding(4) var cmap_samp: sampler;
struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}
@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VsOut {
    var pos = array<vec2<f32>, 3>(
        vec2(-1.0, -1.0),
        vec2(3.0, -1.0),
        vec2(-1.0, 3.0)
    );
    let p = pos[vid];
    var o: VsOut;
    o.clip_pos = vec4(p, 0.0, 1.0);
    o.uv = p * vec2(0.5, -0.5) + vec2(0.5, 0.5);
    return o;
}
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var tx: f32;
    var ty: f32;
    if (u.mode == 0u) {
        tx = 1.0 - in.uv.y;
        ty = fract(in.uv.x + u.scroll);
    } else {
        tx = 1.0 - in.uv.x;
        ty = fract(in.uv.y + u.scroll);
    }
    var mag = textureSample(tex, samp, vec2(tx, ty)).r;
    // Contrast: pivot around 0.5
    mag = clamp((mag - 0.5) * u.contrast + 0.5, 0.0, 1.0);
    var c = textureSample(cmap_tex, cmap_samp, vec2(mag, 0.5)).rgb;
    // Saturation: mix towards luminance
    let lum = dot(c, vec3(0.2126, 0.7152, 0.0722));
    c = mix(vec3(lum), c, u.saturation);
    return vec4(c, 1.0);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    scroll: f32,
    tex_w: f32,
    tex_h: f32,
    mode: u32,
    contrast: f32,
    saturation: f32,
    _pad0: f32,
    _pad1: f32,
}

#[derive(Clone)]
pub struct SpectrogramProgram {
    /// One `Vec<f32>` spectrum per STFT hop; drained in `prepare` (multiple rows per frame possible).
    pub pending_spectra: Arc<Mutex<VecDeque<Vec<f32>>>>,
    pub bins: u32,
    pub min_history: u32,
    pub dev: SpectrogramDevConfig,
    pub colormap_lut: Arc<Vec<[u8; 4]>>,
    pub contrast: f32,
    pub saturation: f32,
}

pub struct SpectrogramPrimitive {
    pub pending_spectra: Arc<Mutex<VecDeque<Vec<f32>>>>,
    pub bins: u32,
    pub history: u32,
    pub dev: SpectrogramDevConfig,
    pub colormap_lut: Arc<Vec<[u8; 4]>>,
    pub contrast: f32,
    pub saturation: f32,
}

impl fmt::Debug for SpectrogramPrimitive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SpectrogramPrimitive")
            .field("bins", &self.bins)
            .field("history", &self.history)
            .field("dev", &self.dev)
            .finish()
    }
}

pub struct SpectrogramGpu {
    bind_group_layout: wgpu::BindGroupLayout,
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    uniform: wgpu::Buffer,
    cmap_texture: wgpu::Texture,
    cmap_view: wgpu::TextureView,
    cmap_sampler: wgpu::Sampler,
    cmap_uploaded: bool,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    write_row: u32,
}

fn make_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform: &wgpu::Buffer,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    cmap_view: &wgpu::TextureView,
    cmap_sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("hyprgram-bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(cmap_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(cmap_sampler),
            },
        ],
    })
}

impl shader::Pipeline for SpectrogramGpu {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hyprgram-spectrogram"),
            source: wgpu::ShaderSource::Wgsl(WGSL.into()),
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hyprgram-uniform"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("hyprgram-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let cmap_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("hyprgram-cmap-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hyprgram-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hyprgram-spectrum"),
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let cmap_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hyprgram-cmap"),
            size: wgpu::Extent3d {
                width: 256,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let cmap_view = cmap_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = make_bind_group(device, &bind_group_layout, &uniform, &texture_view, &sampler, &cmap_view, &cmap_sampler);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hyprgram-pl"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hyprgram-rp"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        Self {
            bind_group_layout,
            texture,
            texture_view,
            sampler,
            uniform,
            cmap_texture,
            cmap_view,
            cmap_sampler,
            cmap_uploaded: false,
            bind_group,
            pipeline,
            write_row: 0,
        }
    }
}

impl shader::Primitive for SpectrogramPrimitive {
    type Pipeline = SpectrogramGpu;
    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _bounds: &Rectangle,
        _viewport: &shader::Viewport,
    ) {
        let w = self.bins.max(1);
        let h = self.history.max(1);
        let need = device.limits().max_texture_dimension_2d;
        if w > need || h > need {
            return;
        }
        if !pipeline.cmap_uploaded {
            let lut = &*self.colormap_lut;
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &pipeline.cmap_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(lut),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256 * 4),
                    rows_per_image: Some(1),
                },
                wgpu::Extent3d {
                    width: 256,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
            pipeline.cmap_uploaded = true;
        }
        let cur_w = pipeline.texture.size().width;
        let cur_h = pipeline.texture.size().height;
        if cur_w != w || cur_h != h {
            pipeline.texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("hyprgram-spectrum"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            pipeline.texture_view = pipeline.texture.create_view(&wgpu::TextureViewDescriptor::default());
            pipeline.bind_group = make_bind_group(
                device,
                &pipeline.bind_group_layout,
                &pipeline.uniform,
                &pipeline.texture_view,
                &pipeline.sampler,
                &pipeline.cmap_view,
                &pipeline.cmap_sampler,
            );
            pipeline.write_row = 0;
        }
        let mut row_u8 = vec![0u8; w as usize];
        let mut last_y: Option<u32> = None;
        loop {
            let col = { self.pending_spectra.lock().unwrap().pop_front() };
            let Some(col) = col else { break };
            let n = col.len().min(row_u8.len());
            for (dst, &src) in row_u8[..n].iter_mut().zip(&col[..n]) {
                *dst = (src.clamp(0.0, 1.0) * 255.0) as u8;
            }
            for dst in row_u8[n..].iter_mut() {
                *dst = 0;
            }
            let y = pipeline.write_row % h;
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &pipeline.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &row_u8,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w),
                    rows_per_image: Some(1),
                },
                wgpu::Extent3d {
                    width: w,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
            pipeline.write_row = pipeline.write_row.wrapping_add(1);
            last_y = Some(y);
        }
        if let Some(y) = last_y {
            let scroll = (y as f32 + 1.0) / (h as f32);
            let u = Uniforms {
                scroll,
                tex_w: w as f32,
                tex_h: h as f32,
                mode: if self.dev.scroll_right_to_left { 0 } else { 1 },
                contrast: self.contrast,
                saturation: self.saturation,
                _pad0: 0.0,
                _pad1: 0.0,
            };
            queue.write_buffer(&pipeline.uniform, 0, bytemuck::bytes_of(&u));
        }
    }
    fn draw(
        &self,
        pipeline: &Self::Pipeline,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        pass.set_pipeline(&pipeline.pipeline);
        pass.set_bind_group(0, &pipeline.bind_group, &[]);
        pass.draw(0..3, 0..1);
        true
    }
}

impl<Message: 'static> shader::Program<Message> for SpectrogramProgram {
    type State = ();
    type Primitive = SpectrogramPrimitive;
    fn draw(
        &self,
        _state: &Self::State,
        _cursor: Cursor,
        bounds: Rectangle,
    ) -> Self::Primitive {
        let time_px = if self.dev.scroll_right_to_left {
            bounds.width as u32
        } else {
            bounds.height as u32
        };
        let history = time_px.max(self.min_history).max(1);
        SpectrogramPrimitive {
            pending_spectra: self.pending_spectra.clone(),
            bins: self.bins,
            history,
            dev: self.dev,
            colormap_lut: self.colormap_lut.clone(),
            contrast: self.contrast,
            saturation: self.saturation,
        }
    }
    fn mouse_interaction(
        &self,
        _state: &Self::State,
        _bounds: Rectangle,
        _cursor: Cursor,
    ) -> Interaction {
        Interaction::None
    }
}
