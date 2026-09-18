//! GPU state: surface + device, the shared [`SceneRenderer`] pass chain,
//! particles, bloom/tonemap post, UI overlay, and optional RGBA frame capture.

use std::sync::Arc;

use anyhow::{anyhow, Context as _, Result};
use kerabit_color::Color;
use winit::window::Window;

use crate::camera::Camera;
use crate::environment::EquirectImage;
use crate::light::Light;
use crate::mesh::Mesh;
use crate::mesh_gpu::MeshId;
use crate::overlay::{
    bake_atlas_rgba, quad_to_vertices, OverlayCommands, OverlayVertex, ATLAS_HEIGHT, ATLAS_WIDTH,
    MAX_OVERLAY_VERTICES,
};
use crate::particles::{ParticleBurst, ParticleSystem};
use crate::post::{PostStack, HDR_FORMAT};
use crate::scene_renderer::SceneRenderer;
use crate::texture::TextureId;
use crate::uniforms::{DrawItem, RenderSettings};

const SHADER_BLIT: &str = include_str!("../shaders/blit.wgsl");
const SHADER_OVERLAY: &str = include_str!("../shaders/overlay.wgsl");
/// Capture color format (RGBA for easy PNG encode).
const CAPTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// Present / acquire failure (maps from wgpu without exposing it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceError {
    Lost,
    Outdated,
    Timeout,
    OutOfMemory,
    Other,
}

impl From<wgpu::SurfaceError> for SurfaceError {
    fn from(err: wgpu::SurfaceError) -> Self {
        match err {
            wgpu::SurfaceError::Lost => Self::Lost,
            wgpu::SurfaceError::Outdated => Self::Outdated,
            wgpu::SurfaceError::Timeout => Self::Timeout,
            wgpu::SurfaceError::OutOfMemory => Self::OutOfMemory,
            wgpu::SurfaceError::Other => Self::Other,
        }
    }
}

/// Offscreen RGBA target + staging buffer for marketing / trailer frame dumps.
struct FrameCapture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    staging: wgpu::Buffer,
    /// Tonemap/bloom into [`CAPTURE_FORMAT`] (separate from swapchain post).
    post: PostStack,
    blit_pipeline: wgpu::RenderPipeline,
    blit_bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
    bytes_per_row: u32,
    last_rgba: Option<Vec<u8>>,
}

/// Screen-space UI pass resources (after post, on the swapchain).
struct Overlay {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    _atlas_texture: wgpu::Texture,
    _atlas_sampler: wgpu::Sampler,
    scratch: Vec<OverlayVertex>,
}

/// Owns wgpu resources and the scene pass chain for multi-mesh lit draws.
pub struct GpuState {
    pub clear_color: Color,
    surface: Option<wgpu::Surface<'static>>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: SceneRenderer,
    post: PostStack,
    particles: ParticleSystem,
    overlay: Overlay,
    /// When set, each frame is tonemapped to RGBA and staged for [`Self::take_captured_rgba`].
    capture: Option<FrameCapture>,
}

impl GpuState {
    pub fn new(window: Arc<Window>, clear_color: Color) -> Result<Self> {
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let surface = instance
            .create_surface(window.clone())
            .context("failed to create wgpu surface")?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .ok_or_else(|| anyhow!("no suitable GPU adapter (Metal/Vulkan/DX12 required)"))?;
        let (device, queue) = request_device(&adapter, "kerabit-device")?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width,
            height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        Ok(Self::build(device, queue, config, Some(surface), clear_color))
    }

    /// Headless GPU (no window). Always captures RGBA frames — call
    /// [`Self::render_lights`] then [`Self::take_captured_rgba`].
    pub fn new_headless(width: u32, height: u32, clear_color: Color) -> Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .ok_or_else(|| anyhow!("no suitable GPU adapter (Metal/Vulkan/DX12 required)"))?;
        let (device, queue) = request_device(&adapter, "kerabit-device-headless")?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        let mut gpu = Self::build(device, queue, config, None, clear_color);
        gpu.enable_frame_capture();
        Ok(gpu)
    }

    fn build(
        device: wgpu::Device,
        queue: wgpu::Queue,
        config: wgpu::SurfaceConfiguration,
        surface: Option<wgpu::Surface<'static>>,
        clear_color: Color,
    ) -> Self {
        let (width, height) = (config.width, config.height);
        let renderer = SceneRenderer::new(&device, &queue, width, height);
        let mut post = PostStack::new(&device, config.format, width, height);
        post.set_source(&device, renderer.output_view());
        let particles = ParticleSystem::new(&device, HDR_FORMAT);
        let overlay = Overlay::new(&device, &queue, config.format);
        Self {
            clear_color,
            surface,
            device,
            queue,
            config,
            renderer,
            post,
            particles,
            overlay,
            capture: None,
        }
    }

    /// Enable RGBA frame capture (for trailers / offline encode). Idempotent.
    pub fn enable_frame_capture(&mut self) {
        if self.capture.is_some() {
            return;
        }
        let mut cap = FrameCapture::new(&self.device, self.config.format, self.config.width, self.config.height);
        cap.post.set_source(&self.device, self.renderer.output_view());
        self.capture = Some(cap);
    }

    /// Pop the last captured RGBA8 frame, if any (`width`, `height`, tightly packed rows).
    pub fn take_captured_rgba(&mut self) -> Option<(u32, u32, Vec<u8>)> {
        let cap = self.capture.as_mut()?;
        let rgba = cap.last_rgba.take()?;
        Some((cap.width, cap.height, rgba))
    }

    /// Upload a CPU mesh; returns a [`MeshId`] for draw lists.
    pub fn upload_mesh(&mut self, mesh: &Mesh) -> MeshId {
        self.renderer.mesh_cache.upload(&self.device, mesh)
    }

    /// Upload an RGBA8 sRGB albedo texture.
    pub fn upload_texture_rgba8(&mut self, width: u32, height: u32, rgba: &[u8]) -> TextureId {
        self.renderer
            .texture_cache
            .upload_rgba8(&self.device, &self.queue, width, height, rgba)
    }

    /// Upload an RGBA8 linear texture (normal maps).
    pub fn upload_texture_rgba8_linear(&mut self, width: u32, height: u32, rgba: &[u8]) -> TextureId {
        self.renderer
            .texture_cache
            .upload_rgba8_linear(&self.device, &self.queue, width, height, rgba)
    }

    pub fn white_texture(&self) -> TextureId {
        self.renderer.texture_cache.white()
    }

    pub fn flat_normal_texture(&self) -> TextureId {
        self.renderer.texture_cache.flat_normal()
    }

    /// Image-based lighting from an equirectangular HDR; `None` restores the procedural sky.
    pub fn set_environment(&mut self, image: Option<&EquirectImage<'_>>, intensity: f32) {
        self.renderer.set_environment(&self.device, &self.queue, image, intensity);
        self.rebind_post();
    }

    /// Toggle SSAO / IBL / SSR / TAA.
    pub fn settings_mut(&mut self) -> &mut RenderSettings {
        &mut self.renderer.settings
    }

    /// Emit a particle burst (billboards).
    pub fn spawn_particles(&mut self, burst: ParticleBurst) {
        self.particles.emit(&burst);
    }

    pub fn clear_particles(&mut self) {
        self.particles.clear();
    }

    /// Advance particle simulation (call once per frame before render).
    pub fn update_particles(&mut self, dt: f32) {
        self.particles.update(dt);
    }

    pub fn aspect(&self) -> f32 {
        self.config.width as f32 / self.config.height.max(1) as f32
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        if let Some(surface) = self.surface.as_ref() {
            surface.configure(&self.device, &self.config);
        }
        self.renderer.resize(&self.device, new_size.width, new_size.height);
        self.post.resize(&self.device, new_size.width, new_size.height);
        if self.capture.is_some() {
            self.capture = Some(FrameCapture::new(
                &self.device,
                self.config.format,
                new_size.width,
                new_size.height,
            ));
        }
        self.rebind_post();
    }

    fn rebind_post(&mut self) {
        self.post.set_source(&self.device, self.renderer.output_view());
        if let Some(cap) = self.capture.as_mut() {
            cap.post.set_source(&self.device, self.renderer.output_view());
        }
    }

    /// Draw with a single light (legacy). Prefer [`Self::render_lights`].
    pub fn render(
        &mut self,
        camera: &mut Camera,
        light: &Light,
        ambient: Color,
        draws: &[DrawItem],
        overlay: &OverlayCommands,
    ) -> Result<(), SurfaceError> {
        self.render_lights(camera, std::slice::from_ref(light), ambient, draws, overlay)
    }

    /// Scene pass chain → particles → TAA → tonemap/bloom → overlay.
    ///
    /// When frame capture is enabled, the frame is tonemapped into an RGBA
    /// offscreen target (read back via [`Self::take_captured_rgba`]) and
    /// blitted to the swapchain.
    pub fn render_lights(
        &mut self,
        camera: &mut Camera,
        lights: &[Light],
        ambient: Color,
        draws: &[DrawItem],
        overlay: &OverlayCommands,
    ) -> Result<(), SurfaceError> {
        let output = match self.surface.as_ref() {
            Some(surface) => Some(surface.get_current_texture().map_err(SurfaceError::from)?),
            None => None,
        };
        let surface_view = output
            .as_ref()
            .map(|o| o.texture.create_view(&wgpu::TextureViewDescriptor::default()));
        // Headless always captures; windowed captures when enabled.
        if self.surface.is_none() && self.capture.is_none() {
            return Err(SurfaceError::Other);
        }

        let overlay_vert_count = self.overlay.upload(&self.queue, overlay);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame-encoder"),
            });

        self.renderer.encode_scene(
            &self.device,
            &self.queue,
            &mut encoder,
            camera,
            lights,
            ambient,
            self.clear_color,
            draws,
        );
        self.particles.encode(
            &self.queue,
            &mut encoder,
            camera,
            self.renderer.lit_view(),
            self.renderer.depth_view(),
        );
        self.renderer.encode_finish(&mut encoder);

        let mut capture = self.capture.take();
        if let Some(cap) = capture.as_mut() {
            cap.post.encode(&self.queue, &mut encoder, &cap.view);
            if let Some(surface_view) = surface_view.as_ref() {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("capture-blit"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: surface_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                });
                pass.set_pipeline(&cap.blit_pipeline);
                pass.set_bind_group(0, &cap.blit_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &cap.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &cap.staging,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(cap.bytes_per_row),
                        rows_per_image: Some(cap.height),
                    },
                },
                wgpu::Extent3d {
                    width: cap.width,
                    height: cap.height,
                    depth_or_array_layers: 1,
                },
            );
        } else if let Some(surface_view) = surface_view.as_ref() {
            self.post.encode(&self.queue, &mut encoder, surface_view);
        }

        if overlay_vert_count > 0 {
            if let Some(surface_view) = surface_view.as_ref() {
                self.overlay.encode(&mut encoder, surface_view, overlay_vert_count);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        if let Some(output) = output {
            output.present();
        }

        if let Some(cap) = capture.as_mut() {
            cap.read_back(&self.device);
        }
        self.capture = capture;
        Ok(())
    }
}

fn request_device(adapter: &wgpu::Adapter, label: &str) -> Result<(wgpu::Device, wgpu::Queue)> {
    pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some(label),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
        },
        None,
    ))
    .context("failed to request wgpu device")
}

impl Overlay {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, surface_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("overlay"),
            source: wgpu::ShaderSource::Wgsl(SHADER_OVERLAY.into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("overlay-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("overlay-pipeline-layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("overlay-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[OverlayVertex::layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let atlas_rgba = bake_atlas_rgba();
        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ui-font-atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_WIDTH,
                height: ATLAS_HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atlas_rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ATLAS_WIDTH * 4),
                rows_per_image: Some(ATLAS_HEIGHT),
            },
            wgpu::Extent3d {
                width: ATLAS_WIDTH,
                height: ATLAS_HEIGHT,
                depth_or_array_layers: 1,
            },
        );
        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ui-font-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("overlay-bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&atlas_sampler),
                },
            ],
        });
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("overlay-vertices"),
            size: (std::mem::size_of::<OverlayVertex>() * MAX_OVERLAY_VERTICES) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            vertex_buffer,
            bind_group,
            _atlas_texture: atlas_texture,
            _atlas_sampler: atlas_sampler,
            scratch: Vec::with_capacity(256),
        }
    }

    /// Expand quads into the vertex buffer; returns the vertex count.
    fn upload(&mut self, queue: &wgpu::Queue, overlay: &OverlayCommands) -> u32 {
        self.scratch.clear();
        for q in overlay.quads() {
            if self.scratch.len() + 6 > MAX_OVERLAY_VERTICES {
                break;
            }
            quad_to_vertices(q, &mut self.scratch);
        }
        let count = self.scratch.len() as u32;
        if count > 0 {
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&self.scratch));
        }
        count
    }

    fn encode(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, vert_count: u32) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("overlay-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..vert_count, 0..1);
    }
}

impl FrameCapture {
    fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let bytes_per_row = align_bytes_per_row(width * 4);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("frame-capture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: CAPTURE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame-capture-staging"),
            size: (bytes_per_row * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let post = PostStack::new(device, CAPTURE_FORMAT, width, height);

        let blit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("capture-blit"),
            source: wgpu::ShaderSource::Wgsl(SHADER_BLIT.into()),
        });
        let blit_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("capture-blit-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let blit_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("capture-blit-layout"),
            bind_group_layouts: &[&blit_bgl],
            push_constant_ranges: &[],
        });
        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("capture-blit-pipeline"),
            layout: Some(&blit_layout),
            vertex: wgpu::VertexState {
                module: &blit_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &blit_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let blit_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("capture-blit-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let blit_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("capture-blit-bg"),
            layout: &blit_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&blit_sampler),
                },
            ],
        });

        Self {
            texture,
            view,
            staging,
            post,
            blit_pipeline,
            blit_bind_group,
            width,
            height,
            bytes_per_row,
            last_rgba: None,
        }
    }

    /// Block until the staged frame is mapped and unpack it into `last_rgba`.
    fn read_back(&mut self, device: &wgpu::Device) {
        let buffer_slice = self.staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        device.poll(wgpu::Maintain::Wait);
        if rx.recv().ok().and_then(|r| r.ok()).is_some() {
            let data = buffer_slice.get_mapped_range();
            let mut rgba = Vec::with_capacity((self.width * self.height * 4) as usize);
            let row_bytes = (self.width * 4) as usize;
            for y in 0..self.height as usize {
                let start = y * self.bytes_per_row as usize;
                rgba.extend_from_slice(&data[start..start + row_bytes]);
            }
            drop(data);
            self.staging.unmap();
            self.last_rgba = Some(rgba);
        }
    }
}

fn align_bytes_per_row(bytes: u32) -> u32 {
    (bytes + 255) & !255
}
