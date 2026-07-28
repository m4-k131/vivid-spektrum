use crate::dev::SpectrogramDevConfig;
use iced::mouse::{Cursor, Interaction};
use iced::widget::shader;
use iced::wgpu;
use iced::Rectangle;
use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Instant;

const WGSL: &str = r#"
struct Uniforms {
    scroll: f32,
    tex_w: f32,
    tex_h: f32,
    mode: u32,
    contrast: f32,
    saturation: f32,
    overlay_count: u32,
    overlay_thickness: f32,
    overlay_color: vec4<f32>,
    overlay_a: vec4<f32>,
    overlay_b: vec4<f32>,
    overlay_c: vec4<f32>,
    opacity: f32,
    shared_bg: u32,
    is_first: u32,
    bg_r: f32,
    bg_g: f32,
    bg_b: f32,
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
fn get_overlay_line(i: u32) -> f32 {
    if (i < 4u) {
        return u.overlay_a[i];
    } else if (i < 8u) {
        return u.overlay_b[i - 4u];
    } else {
        return u.overlay_c[i - 8u];
    }
}
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var tx: f32;
    var ty: f32;
    var freq_axis: f32;
    if (u.mode == 0u) {
        tx = 1.0 - in.uv.y;
        ty = fract(in.uv.x + u.scroll);
        freq_axis = in.uv.y;
    } else {
        tx = 1.0 - in.uv.x;
        ty = fract(in.uv.y + u.scroll);
        freq_axis = in.uv.x;
    }
    var mag = textureSample(tex, samp, vec2(tx, ty)).r;
    mag = clamp((mag - 0.5) * u.contrast + 0.5, 0.0, 1.0);
    var c = textureSample(cmap_tex, cmap_samp, vec2(mag, 0.5)).rgb;
    let lum = dot(c, vec3(0.2126, 0.7152, 0.0722));
    c = mix(vec3(lum), c, u.saturation);
    var overlay_alpha = 0.0;
    for (var i = 0u; i < u.overlay_count; i = i + 1u) {
        let line_pos = get_overlay_line(i);
        let dist = abs(freq_axis - line_pos);
        if (dist < u.overlay_thickness) {
            overlay_alpha = max(overlay_alpha, 1.0 - dist / u.overlay_thickness);
        }
    }
    if (u.shared_bg == 1u) {
        let bg = vec3(u.bg_r, u.bg_g, u.bg_b);
        let signal_alpha = smoothstep(0.0, 0.03, mag) * u.opacity;
        if (u.is_first == 1u) {
            let signal_color = mix(bg, c, signal_alpha);
            let final_color = mix(signal_color, u.overlay_color.rgb, overlay_alpha * u.overlay_color.a);
            return vec4(final_color, 1.0);
        } else {
            let layer_color = mix(c, u.overlay_color.rgb, overlay_alpha * u.overlay_color.a);
            return vec4(layer_color, signal_alpha);
        }
    }
    if (overlay_alpha > 0.0) {
        c = mix(c, u.overlay_color.rgb, overlay_alpha * u.overlay_color.a);
    }
    return vec4(c, u.opacity);
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
    overlay_count: u32,
    overlay_thickness: f32,
    overlay_color: [f32; 4],
    overlay_a: [f32; 4],
    overlay_b: [f32; 4],
    overlay_c: [f32; 4],
    opacity: f32,
    shared_bg: u32,
    is_first: u32,
    bg_r: f32,
    bg_g: f32,
    bg_b: f32,
    _pad0: f32,
    _pad1: f32,
}

#[derive(Clone)]
pub struct SpectrogramProgram {
    /// One `Vec<f32>` spectrum per STFT hop; drained in `prepare` (multiple rows per frame possible).
    pub pending_spectra: Arc<Mutex<VecDeque<Vec<f32>>>>,
    pub bins: u32,
    pub min_history: u32,
    pub paused: bool,
    pub dev: SpectrogramDevConfig,
    pub colormap_lut: Arc<Vec<[u8; 4]>>,
    pub contrast: f32,
    pub saturation: f32,
    pub debug_profile: bool,
    pub overlay_lines: Vec<f32>,
    pub overlay_color: [f32; 3],
    pub overlay_opacity: f32,
    pub overlay_thickness: f32,
    pub opacity: f32,
}

pub struct SpectrogramPrimitive {
    pub pending_spectra: Arc<Mutex<VecDeque<Vec<f32>>>>,
    pub bins: u32,
    pub history: u32,
    pub paused: bool,
    pub dev: SpectrogramDevConfig,
    pub colormap_lut: Arc<Vec<[u8; 4]>>,
    pub contrast: f32,
    pub saturation: f32,
    pub debug_profile: bool,
    pub overlay_lines: Vec<f32>,
    pub overlay_color: [f32; 3],
    pub overlay_opacity: f32,
    pub overlay_thickness: f32,
    pub opacity: f32,
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
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    write_row: u32,
    scroll: f32,
    prof_last_report: Instant,
    prof_prepare_us: u64,
    prof_cols_uploaded: u64,
    prof_frames: u64,
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
        label: Some("vividspektrum-bg"),
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
            label: Some("vividspektrum-spectrogram"),
            source: wgpu::ShaderSource::Wgsl(WGSL.into()),
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vividspektrum-uniform"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vividspektrum-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let cmap_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vividspektrum-cmap-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vividspektrum-bgl"),
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
            label: Some("vividspektrum-spectrum"),
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
            label: Some("vividspektrum-cmap"),
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
            label: Some("vividspektrum-pl"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vividspektrum-rp"),
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
            bind_group,
            pipeline,
            write_row: 0,
            scroll: 0.0,
            prof_last_report: Instant::now(),
            prof_prepare_us: 0,
            prof_cols_uploaded: 0,
            prof_frames: 0,
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
        let prepare_start = Instant::now();
        let w = self.bins.max(1);
        let h = self.history.max(1);
        let need = device.limits().max_texture_dimension_2d;
        if w > need || h > need {
            return;
        }
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
        let cur_w = pipeline.texture.size().width;
        let cur_h = pipeline.texture.size().height;
        if cur_w != w || cur_h != h {
            pipeline.texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("vividspektrum-spectrum"),
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
        let prev_write_row = pipeline.write_row;
        let mut last_y: Option<u32> = None;
        if self.paused {
            self.pending_spectra.lock().unwrap().clear();
        } else {
            let mut row_u8 = vec![0u8; w as usize];
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
        }
        if let Some(y) = last_y {
            pipeline.scroll = (y as f32 + 1.0) / (h as f32);
        }
        let mut overlay_a = [0.0f32; 4];
        let mut overlay_b = [0.0f32; 4];
        let mut overlay_c = [0.0f32; 4];
        let count = self.overlay_lines.len().min(12);
        for (i, &v) in self.overlay_lines.iter().take(12).enumerate() {
            if i < 4 { overlay_a[i] = v; }
            else if i < 8 { overlay_b[i - 4] = v; }
            else { overlay_c[i - 8] = v; }
        }
        let u = Uniforms {
            scroll: pipeline.scroll,
            tex_w: w as f32,
            tex_h: h as f32,
            mode: if self.dev.scroll_right_to_left { 0 } else { 1 },
            contrast: self.contrast,
            saturation: self.saturation,
            overlay_count: count as u32,
            overlay_thickness: self.overlay_thickness,
            overlay_color: [self.overlay_color[0], self.overlay_color[1], self.overlay_color[2], self.overlay_opacity],
            overlay_a,
            overlay_b,
            overlay_c,
            opacity: self.opacity,
            shared_bg: 0,
            is_first: 1,
            bg_r: 0.0,
            bg_g: 0.0,
            bg_b: 0.0,
            _pad0: 0.0,
            _pad1: 0.0,
        };
        queue.write_buffer(&pipeline.uniform, 0, bytemuck::bytes_of(&u));
        if self.debug_profile {
            let cols_this_frame = if last_y.is_some() { pipeline.write_row.wrapping_sub(prev_write_row) as u64 } else { 0 };
            pipeline.prof_prepare_us += prepare_start.elapsed().as_micros() as u64;
            pipeline.prof_cols_uploaded += cols_this_frame;
            pipeline.prof_frames += 1;
            let elapsed = pipeline.prof_last_report.elapsed();
            if elapsed >= std::time::Duration::from_secs(1) {
                let secs = elapsed.as_secs_f64();
                let queue_depth = self.pending_spectra.lock().unwrap().len();
                eprintln!(
                    "[profile] GPU: {:.1} fps | prepare: {:.1}ms/frame avg | cols/sec: {:.0} | queue: {} | texture: {}x{}",
                    pipeline.prof_frames as f64 / secs,
                    (pipeline.prof_prepare_us as f64 / pipeline.prof_frames.max(1) as f64) / 1000.0,
                    pipeline.prof_cols_uploaded as f64 / secs,
                    queue_depth,
                    w, h,
                );
                pipeline.prof_last_report = Instant::now();
                pipeline.prof_prepare_us = 0;
                pipeline.prof_cols_uploaded = 0;
                pipeline.prof_frames = 0;
            }
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
            paused: self.paused,
            dev: self.dev,
            colormap_lut: self.colormap_lut.clone(),
            contrast: self.contrast,
            saturation: self.saturation,
            debug_profile: self.debug_profile,
            overlay_lines: self.overlay_lines.clone(),
            overlay_color: self.overlay_color,
            overlay_opacity: self.overlay_opacity,
            overlay_thickness: self.overlay_thickness,
            opacity: self.opacity,
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

#[derive(Clone)]
pub struct MultiSpectrogramProgram {
    pub sources: Vec<SpectrogramProgram>,
    pub dev: SpectrogramDevConfig,
    pub debug_profile: bool,
    pub shared_bg: bool,
}

pub struct SourceLayer {
    pending_spectra: Arc<Mutex<VecDeque<Vec<f32>>>>,
    bins: u32,
    history: u32,
    paused: bool,
    colormap_lut: Arc<Vec<[u8; 4]>>,
    contrast: f32,
    saturation: f32,
    overlay_lines: Vec<f32>,
    overlay_color: [f32; 3],
    overlay_opacity: f32,
    overlay_thickness: f32,
    opacity: f32,
}

pub struct MultiSpectrogramPrimitive {
    sources: Vec<SourceLayer>,
    dev: SpectrogramDevConfig,
    debug_profile: bool,
    shared_bg: bool,
}

impl fmt::Debug for MultiSpectrogramPrimitive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MultiSpectrogramPrimitive")
            .field("sources", &self.sources.len())
            .field("dev", &self.dev)
            .finish()
    }
}

struct SourceGpuState {
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    uniform: wgpu::Buffer,
    cmap_texture: wgpu::Texture,
    cmap_view: wgpu::TextureView,
    cmap_sampler: wgpu::Sampler,
    bind_group: wgpu::BindGroup,
    write_row: u32,
    scroll: f32,
    spectra_ptr: usize,
    prof_last_report: Instant,
    prof_prepare_us: u64,
    prof_cols_uploaded: u64,
    prof_frames: u64,
}

pub struct MultiSpectrogramGpu {
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    pipeline: wgpu::RenderPipeline,
    sources: Vec<SourceGpuState>,
}

fn create_source_gpu(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
) -> SourceGpuState {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vividspektrum-spectrum-multi"),
        size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("vividspektrum-uniform-multi"),
        size: std::mem::size_of::<Uniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let cmap_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vividspektrum-cmap-multi"),
        size: wgpu::Extent3d { width: 256, height: 1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let cmap_view = cmap_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let cmap_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("vividspektrum-cmap-sampler-multi"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let bind_group = make_bind_group(
        device,
        bind_group_layout,
        &uniform,
        &texture_view,
        sampler,
        &cmap_view,
        &cmap_sampler,
    );
    SourceGpuState {
        texture,
        texture_view,
        uniform,
        cmap_texture,
        cmap_view,
        cmap_sampler,
        bind_group,
        write_row: 0,
        scroll: 0.0,
        spectra_ptr: 0,
        prof_last_report: Instant::now(),
        prof_prepare_us: 0,
        prof_cols_uploaded: 0,
        prof_frames: 0,
    }
}

impl shader::Pipeline for MultiSpectrogramGpu {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vividspektrum-spectrogram-multi"),
            source: wgpu::ShaderSource::Wgsl(WGSL.into()),
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vividspektrum-sampler-multi"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vividspektrum-bgl-multi"),
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
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
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
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vividspektrum-pll-multi"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vividspektrum-rp-multi"),
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
            sampler,
            pipeline,
            sources: Vec::new(),
        }
    }
}

impl shader::Primitive for MultiSpectrogramPrimitive {
    type Pipeline = MultiSpectrogramGpu;
    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _bounds: &Rectangle,
        _viewport: &shader::Viewport,
    ) {
        while pipeline.sources.len() < self.sources.len() {
            pipeline.sources.push(create_source_gpu(
                device,
                &pipeline.bind_group_layout,
                &pipeline.sampler,
            ));
        }
        while pipeline.sources.len() > self.sources.len() {
            pipeline.sources.pop();
        }

        let mode = if self.dev.scroll_right_to_left { 0u32 } else { 1 };
        let prepare_start = Instant::now();

        let (bg_r, bg_g, bg_b) = if self.shared_bg {
            let mut darkest_lum = f32::MAX;
            let mut dr = 0.0f32;
            let mut dg = 0.0f32;
            let mut db = 0.0f32;
            for src in &self.sources {
                let lut = &*src.colormap_lut;
                if let Some(&c0) = lut.first() {
                    let r = c0[0] as f32 / 255.0;
                    let g = c0[1] as f32 / 255.0;
                    let b = c0[2] as f32 / 255.0;
                    let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                    if lum < darkest_lum {
                        darkest_lum = lum;
                        dr = r;
                        dg = g;
                        db = b;
                    }
                }
            }
            (dr, dg, db)
        } else {
            (0.0, 0.0, 0.0)
        };

        for (i, src) in self.sources.iter().enumerate() {
            let gpu = &mut pipeline.sources[i];
            let prev_write_row = gpu.write_row;
            let w = src.bins.max(1);
            let h = src.history.max(1);
            let need = device.limits().max_texture_dimension_2d;
            if w > need || h > need {
                continue;
            }

            let lut = &*src.colormap_lut;
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &gpu.cmap_texture,
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

            let cur_w = gpu.texture.size().width;
            let cur_h = gpu.texture.size().height;
            let new_ptr = Arc::as_ptr(&src.pending_spectra) as usize;
            let spectra_changed = gpu.spectra_ptr != new_ptr;
            if spectra_changed {
                gpu.spectra_ptr = new_ptr;
                gpu.write_row = 0;
                gpu.scroll = 0.0;
            }
            if cur_w != w || cur_h != h || spectra_changed {
                gpu.texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("vividspektrum-spectrum-multi"),
                    size: wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::R8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                gpu.texture_view =
                    gpu.texture.create_view(&wgpu::TextureViewDescriptor::default());
                gpu.bind_group = make_bind_group(
                    device,
                    &pipeline.bind_group_layout,
                    &gpu.uniform,
                    &gpu.texture_view,
                    &pipeline.sampler,
                    &gpu.cmap_view,
                    &gpu.cmap_sampler,
                );
                gpu.write_row = 0;
            }

            let mut last_y: Option<u32> = None;
            if src.paused {
                src.pending_spectra.lock().unwrap().clear();
            } else {
                let mut row_u8 = vec![0u8; w as usize];
                loop {
                    let col = { src.pending_spectra.lock().unwrap().pop_front() };
                    let Some(col) = col else { break };
                    let n = col.len().min(row_u8.len());
                    for (dst, &s) in row_u8[..n].iter_mut().zip(&col[..n]) {
                        *dst = (s.clamp(0.0, 1.0) * 255.0) as u8;
                    }
                    for dst in row_u8[n..].iter_mut() {
                        *dst = 0;
                    }
                    let y = gpu.write_row % h;
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &gpu.texture,
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
                    gpu.write_row = gpu.write_row.wrapping_add(1);
                    last_y = Some(y);
                }
            }
            if let Some(y) = last_y {
                gpu.scroll = (y as f32 + 1.0) / (h as f32);
            }

            let mut overlay_a = [0.0f32; 4];
            let mut overlay_b = [0.0f32; 4];
            let mut overlay_c = [0.0f32; 4];
            let count = src.overlay_lines.len().min(12);
            for (j, &v) in src.overlay_lines.iter().take(12).enumerate() {
                if j < 4 {
                    overlay_a[j] = v;
                } else if j < 8 {
                    overlay_b[j - 4] = v;
                } else {
                    overlay_c[j - 8] = v;
                }
            }
            let u = Uniforms {
                scroll: gpu.scroll,
                tex_w: w as f32,
                tex_h: h as f32,
                mode,
                contrast: src.contrast,
                saturation: src.saturation,
                overlay_count: count as u32,
                overlay_thickness: src.overlay_thickness,
                overlay_color: [
                    src.overlay_color[0],
                    src.overlay_color[1],
                    src.overlay_color[2],
                    src.overlay_opacity,
                ],
                overlay_a,
                overlay_b,
                overlay_c,
                opacity: src.opacity,
                shared_bg: if self.shared_bg { 1 } else { 0 },
                is_first: if i == 0 { 1 } else { 0 },
                bg_r,
                bg_g,
                bg_b,
                _pad0: 0.0,
                _pad1: 0.0,
            };
            queue.write_buffer(&gpu.uniform, 0, bytemuck::bytes_of(&u));

            if self.debug_profile {
                let cols_this_frame = if last_y.is_some() { gpu.write_row.wrapping_sub(prev_write_row) as u64 } else { 0 };
                gpu.prof_prepare_us += prepare_start.elapsed().as_micros() as u64;
                gpu.prof_cols_uploaded += cols_this_frame;
                gpu.prof_frames += 1;
                let elapsed = gpu.prof_last_report.elapsed();
                if elapsed >= std::time::Duration::from_secs(1) {
                    let secs = elapsed.as_secs_f64();
                    let queue_depth = src.pending_spectra.lock().unwrap().len();
                    eprintln!(
                        "[profile src {}] {:.1} fps | prepare: {:.1}ms/frame | cols/sec: {:.0} | queue: {} | texture: {}x{}",
                        i,
                        gpu.prof_frames as f64 / secs,
                        (gpu.prof_prepare_us as f64 / gpu.prof_frames.max(1) as f64) / 1000.0,
                        gpu.prof_cols_uploaded as f64 / secs,
                        queue_depth,
                        w, h,
                    );
                    gpu.prof_last_report = Instant::now();
                    gpu.prof_prepare_us = 0;
                    gpu.prof_cols_uploaded = 0;
                    gpu.prof_frames = 0;
                }
            }
        }

        if pipeline.sources.len() > 1 {
            let shared_scroll = pipeline.sources.iter()
                .map(|gpu| gpu.scroll)
                .fold(0.0f32, f32::max);
            for gpu in &mut pipeline.sources {
                gpu.scroll = shared_scroll;
                let scroll_bytes = bytemuck::bytes_of(&shared_scroll);
                queue.write_buffer(&gpu.uniform, 0, scroll_bytes);
            }
        }
    }
    fn draw(
        &self,
        pipeline: &Self::Pipeline,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        pass.set_pipeline(&pipeline.pipeline);
        for gpu in &pipeline.sources {
            pass.set_bind_group(0, &gpu.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        !pipeline.sources.is_empty()
    }
}

impl<Message: 'static> shader::Program<Message> for MultiSpectrogramProgram {
    type State = ();
    type Primitive = MultiSpectrogramPrimitive;
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
        let sources = self
            .sources
            .iter()
            .map(|prog| {
                let history = time_px.max(prog.min_history).max(1);
                SourceLayer {
                    pending_spectra: prog.pending_spectra.clone(),
                    bins: prog.bins,
                    history,
                    paused: prog.paused,
                    colormap_lut: prog.colormap_lut.clone(),
                    contrast: prog.contrast,
                    saturation: prog.saturation,
                    overlay_lines: prog.overlay_lines.clone(),
                    overlay_color: prog.overlay_color,
                    overlay_opacity: prog.overlay_opacity,
                    overlay_thickness: prog.overlay_thickness,
                    opacity: prog.opacity,
                }
            })
            .collect();
        MultiSpectrogramPrimitive {
            sources,
            dev: self.dev,
            debug_profile: self.debug_profile,
            shared_bg: self.shared_bg,
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
