//! GPU state: surface, lit instanced pipeline, HDR post, particles, mesh cache.

use std::sync::Arc;

use anyhow::{anyhow, Context as _, Result};
use kerabit_color::Color;
use winit::window::Window;

const SHADER_BLIT: &str = include_str!("../shaders/blit.wgsl");
/// Capture color format (RGBA for easy PNG encode).
const CAPTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

use crate::camera::Camera;
use crate::light::{Light, MAX_LIGHTS};
use crate::mesh::Mesh;
use crate::mesh_gpu::{MeshCache, MeshId};
use crate::overlay::{
    bake_atlas_rgba, quad_to_vertices, OverlayCommands, OverlayVertex, ATLAS_HEIGHT, ATLAS_WIDTH,
    MAX_OVERLAY_VERTICES,
};
use crate::particles::{ParticleBurst, ParticleSystem};
use crate::post::{PostStack, HDR_FORMAT};
use crate::shadow::{directional_light_matrix, ShadowMap, SHADOW_HALF_EXTENT};
use crate::sky::SkyPass;
use crate::texture::{TextureCache, TextureId};
use crate::uniforms::{pack_draw_batches, DrawItem, FrameUniforms, InstanceRaw, MAX_INSTANCES};
use crate::vertex::Vertex;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SHADER_LIT: &str = include_str!("../shaders/lit.wgsl");
const SHADER_OVERLAY: &str = include_str!("../shaders/overlay.wgsl");

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
    #[allow(dead_code)]
    blit_bgl: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    blit_sampler: wgpu::Sampler,
    width: u32,
    height: u32,
    bytes_per_row: u32,
    last_rgba: Option<Vec<u8>>,
}

/// Owns wgpu resources and a mesh cache for multi-mesh lit draws.
pub struct GpuState {
    pub clear_color: Color,
    surface: Option<wgpu::Surface<'static>>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    depth_view: wgpu::TextureView,
    pipeline: wgpu::RenderPipeline,
    frame_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    frame_bind_group: wgpu::BindGroup,
    mesh_cache: MeshCache,
    texture_cache: TextureCache,
    shadow: ShadowMap,
    sky: SkyPass,
    post: PostStack,
    particles: ParticleSystem,
    /// Screen-space UI pass (after post, on swapchain).
    overlay_pipeline: wgpu::RenderPipeline,
    overlay_vertex_buffer: wgpu::Buffer,
    overlay_bind_group: wgpu::BindGroup,
    _atlas_texture: wgpu::Texture,
    _atlas_sampler: wgpu::Sampler,
    overlay_scratch: Vec<OverlayVertex>,
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

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("kerabit-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .context("failed to request wgpu device")?;

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

        let depth_view = create_depth_view(&device, width, height);
        let shadow = ShadowMap::new(&device);
        let sky = SkyPass::new(&device, HDR_FORMAT);
        let post = PostStack::new(&device, surface_format, width, height);
        let particles = ParticleSystem::new(&device, HDR_FORMAT);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lit"),
            source: wgpu::ShaderSource::Wgsl(SHADER_LIT.into()),
        });

        let frame_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let texture_cache = TextureCache::new(&device, &queue);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("lit-pipeline-layout"),
            bind_group_layouts: &[
                &frame_bgl,
                texture_cache.bind_group_layout(),
                &shadow.bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lit-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout(), InstanceRaw::layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR_FORMAT,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let frame_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame-uniforms"),
            size: std::mem::size_of::<FrameUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instance-buffer"),
            size: (std::mem::size_of::<InstanceRaw>() * MAX_INSTANCES) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let frame_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame-bg"),
            layout: &frame_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame_buffer.as_entire_binding(),
            }],
        });

        // --- Overlay (screen-space UI on swapchain after post) ---
        let overlay_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("overlay"),
            source: wgpu::ShaderSource::Wgsl(SHADER_OVERLAY.into()),
        });

        let overlay_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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

        let overlay_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("overlay-pipeline-layout"),
                bind_group_layouts: &[&overlay_bgl],
                push_constant_ranges: &[],
            });

        let overlay_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("overlay-pipeline"),
            layout: Some(&overlay_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &overlay_shader,
                entry_point: Some("vs_main"),
                buffers: &[OverlayVertex::layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &overlay_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
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
        let overlay_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("overlay-bg"),
            layout: &overlay_bgl,
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

        let overlay_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("overlay-vertices"),
            size: (std::mem::size_of::<OverlayVertex>() * MAX_OVERLAY_VERTICES) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            clear_color,
            surface: Some(surface),
            device,
            queue,
            config,
            size: winit::dpi::PhysicalSize::new(width, height),
            depth_view,
            pipeline,
            frame_buffer,
            instance_buffer,
            frame_bind_group,
            mesh_cache: MeshCache::new(),
            texture_cache,
            shadow,
            sky,
            post,
            particles,
            overlay_pipeline,
            overlay_vertex_buffer,
            overlay_bind_group,
            _atlas_texture: atlas_texture,
            _atlas_sampler: atlas_sampler,
            overlay_scratch: Vec::with_capacity(256),
            capture: None,
        })
    }

    /// Enable RGBA frame capture (for trailers / offline encode). Idempotent.
    pub fn enable_frame_capture(&mut self) {
        if self.capture.is_some() {
            return;
        }
        let width = self.config.width.max(1);
        let height = self.config.height.max(1);
        self.capture = Some(FrameCapture::new(
            &self.device,
            self.config.format,
            width,
            height,
        ));
    }

    /// Headless GPU (no window). Always captures RGBA frames — call
    /// [`Self::render_lights`] then [`Self::take_captured_rgba`].
    pub fn new_headless(width: u32, height: u32, clear_color: Color) -> Result<Self> {
        let width = width.max(1);
        let height = height.max(1);

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

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("kerabit-device-headless"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .context("failed to request wgpu device")?;

        // Prefer a common swapchain-like format for blit target compatibility.
        let surface_format = wgpu::TextureFormat::Bgra8UnormSrgb;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        let depth_view = create_depth_view(&device, width, height);
        let shadow = ShadowMap::new(&device);
        let sky = SkyPass::new(&device, HDR_FORMAT);
        let post = PostStack::new(&device, surface_format, width, height);
        let particles = ParticleSystem::new(&device, HDR_FORMAT);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lit"),
            source: wgpu::ShaderSource::Wgsl(SHADER_LIT.into()),
        });

        let frame_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let texture_cache = TextureCache::new(&device, &queue);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("lit-pipeline-layout"),
            bind_group_layouts: &[
                &frame_bgl,
                texture_cache.bind_group_layout(),
                &shadow.bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lit-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout(), InstanceRaw::layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR_FORMAT,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let frame_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame-uniforms"),
            size: std::mem::size_of::<FrameUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instance-buffer"),
            size: (std::mem::size_of::<InstanceRaw>() * MAX_INSTANCES) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let frame_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame-bg"),
            layout: &frame_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame_buffer.as_entire_binding(),
            }],
        });

        let overlay_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("overlay"),
            source: wgpu::ShaderSource::Wgsl(SHADER_OVERLAY.into()),
        });

        let overlay_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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

        let overlay_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("overlay-pipeline-layout"),
                bind_group_layouts: &[&overlay_bgl],
                push_constant_ranges: &[],
            });

        let overlay_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("overlay-pipeline"),
            layout: Some(&overlay_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &overlay_shader,
                entry_point: Some("vs_main"),
                buffers: &[OverlayVertex::layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &overlay_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
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
        let overlay_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("overlay-bg"),
            layout: &overlay_bgl,
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

        let overlay_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("overlay-vertices"),
            size: (std::mem::size_of::<OverlayVertex>() * MAX_OVERLAY_VERTICES) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut gpu = Self {
            clear_color,
            surface: None,
            device,
            queue,
            config,
            size: winit::dpi::PhysicalSize::new(width, height),
            depth_view,
            pipeline,
            frame_buffer,
            instance_buffer,
            frame_bind_group,
            mesh_cache: MeshCache::new(),
            texture_cache,
            shadow,
            sky,
            post,
            particles,
            overlay_pipeline,
            overlay_vertex_buffer,
            overlay_bind_group,
            _atlas_texture: atlas_texture,
            _atlas_sampler: atlas_sampler,
            overlay_scratch: Vec::with_capacity(256),
            capture: None,
        };
        gpu.enable_frame_capture();
        Ok(gpu)
    }

    /// Pop the last captured RGBA8 frame, if any (`width`, `height`, tightly packed rows).
    pub fn take_captured_rgba(&mut self) -> Option<(u32, u32, Vec<u8>)> {
        let cap = self.capture.as_mut()?;
        let rgba = cap.last_rgba.take()?;
        Some((cap.width, cap.height, rgba))
    }

    /// Upload a CPU mesh; returns a [`MeshId`] for draw lists.
    pub fn upload_mesh(&mut self, mesh: &Mesh) -> MeshId {
        self.mesh_cache.upload(&self.device, mesh)
    }

    /// Upload an RGBA8 sRGB albedo texture.
    pub fn upload_texture_rgba8(&mut self, width: u32, height: u32, rgba: &[u8]) -> TextureId {
        self.texture_cache
            .upload_rgba8(&self.device, &self.queue, width, height, rgba)
    }

    /// Upload an RGBA8 linear texture (normal maps).
    pub fn upload_texture_rgba8_linear(
        &mut self,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> TextureId {
        self.texture_cache
            .upload_rgba8_linear(&self.device, &self.queue, width, height, rgba)
    }

    pub fn white_texture(&self) -> TextureId {
        self.texture_cache.white()
    }

    pub fn flat_normal_texture(&self) -> TextureId {
        self.texture_cache.flat_normal()
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
        self.size = new_size;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        if let Some(surface) = self.surface.as_ref() {
            surface.configure(&self.device, &self.config);
        }
        self.depth_view = create_depth_view(&self.device, new_size.width, new_size.height);
        self.post
            .resize(&self.device, new_size.width, new_size.height);
        if self.capture.is_some() {
            self.capture = Some(FrameCapture::new(
                &self.device,
                self.config.format,
                new_size.width,
                new_size.height,
            ));
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

    /// Shadow → sky → lit (HDR) → particles → tonemap/bloom → overlay.
    ///
    /// At most [`MAX_LIGHTS`] lights are used; soft shadows follow the first
    /// directional light. When frame capture is enabled, HDR is tonemapped into
    /// an RGBA offscreen target (read back via [`Self::take_captured_rgba`]) and
    /// blitted to the swapchain.
    pub fn render_lights(
        &mut self,
        camera: &mut Camera,
        lights: &[Light],
        ambient: Color,
        draws: &[DrawItem],
        overlay: &OverlayCommands,
    ) -> Result<(), SurfaceError> {
        // Acquire swapchain only when we have a window surface.
        let output = match self.surface.as_ref() {
            Some(surface) => Some(surface.get_current_texture().map_err(SurfaceError::from)?),
            None => None,
        };
        let surface_view = output
            .as_ref()
            .map(|o| o.texture.create_view(&wgpu::TextureViewDescriptor::default()));

        camera.set_aspect(self.aspect());
        let lights = &lights[..lights.len().min(MAX_LIGHTS)];
        let shadow_dir = Light::first_directional(lights)
            .map(|l| l.direction)
            .unwrap_or_else(|| kerabit_math::vec3(-0.35, -1.0, -0.25));
        let light_vp = directional_light_matrix(shadow_dir, camera.target, SHADOW_HALF_EXTENT);
        let frame = FrameUniforms::from_lights(camera, lights, ambient, light_vp);
        self.queue
            .write_buffer(&self.frame_buffer, 0, bytemuck::bytes_of(&frame));

        let white = self.texture_cache.white();
        let flat_n = self.texture_cache.flat_normal();
        let (flat, ranges) = pack_draw_batches(draws, white, flat_n);
        if !flat.is_empty() {
            self.queue
                .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&flat));
        }

        let shadow_ranges: Vec<(MeshId, u32, u32)> = ranges
            .iter()
            .map(|&(mesh, _, _, start, count)| (mesh, start, count))
            .collect();

        for &(_, albedo, normal, _, _) in &ranges {
            let _ = self
                .texture_cache
                .ensure_material_bind_group(&self.device, albedo, normal);
        }

        self.overlay_scratch.clear();
        for q in overlay.quads() {
            if self.overlay_scratch.len() + 6 > MAX_OVERLAY_VERTICES {
                break;
            }
            quad_to_vertices(q, &mut self.overlay_scratch);
        }
        let overlay_vert_count = self.overlay_scratch.len() as u32;
        if overlay_vert_count > 0 {
            self.queue.write_buffer(
                &self.overlay_vertex_buffer,
                0,
                bytemuck::cast_slice(&self.overlay_scratch),
            );
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame-encoder"),
            });

        self.shadow.encode(
            &self.queue,
            &mut encoder,
            light_vp,
            &self.mesh_cache,
            &self.instance_buffer,
            &shadow_ranges,
        );

        // Headless always captures; windowed captures when enabled.
        if self.surface.is_none() && self.capture.is_none() {
            return Err(SurfaceError::Other);
        }

        // When capturing, render HDR into the capture post stack so tonemap can
        // land in RGBA for PNG readback; otherwise use the window post stack.
        let mut capture = self.capture.take();
        {
            let hdr_view = if let Some(cap) = capture.as_ref() {
                &cap.post.hdr_view
            } else {
                &self.post.hdr_view
            };

            self.sky.encode(
                &self.queue,
                &mut encoder,
                hdr_view,
                &self.depth_view,
                self.clear_color,
            );

            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("lit-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: hdr_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    occlusion_query_set: None,
                    timestamp_writes: None,
                });

                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.frame_bind_group, &[]);
                pass.set_bind_group(2, &self.shadow.bind_group, &[]);

                for (mesh_id, albedo, normal, start, count) in ranges {
                    let Some(gpu_mesh) = self.mesh_cache.get(mesh_id) else {
                        continue;
                    };
                    let Some(tex_bg) = self.texture_cache.material_bind_group(albedo, normal) else {
                        continue;
                    };
                    let byte_offset = start as u64 * std::mem::size_of::<InstanceRaw>() as u64;
                    let byte_size = count as u64 * std::mem::size_of::<InstanceRaw>() as u64;
                    pass.set_bind_group(1, tex_bg, &[]);
                    pass.set_vertex_buffer(0, gpu_mesh.vertex_buffer.slice(..));
                    pass.set_vertex_buffer(
                        1,
                        self.instance_buffer.slice(byte_offset..byte_offset + byte_size),
                    );
                    pass.set_index_buffer(gpu_mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                    pass.draw_indexed(0..gpu_mesh.index_count, 0, 0..count);
                }
            }

            self.particles.encode(
                &self.queue,
                &mut encoder,
                camera,
                hdr_view,
                &self.depth_view,
            );
        }

        if let Some(cap) = capture.as_mut() {
            cap.post
                .encode(&self.queue, &mut encoder, &cap.view);
            // Blit RGBA capture → swapchain when presenting to a window.
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
            // Stage for CPU readback.
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
            self.post
                .encode(&self.queue, &mut encoder, surface_view);
        }

        if overlay_vert_count > 0 {
            if let Some(surface_view) = surface_view.as_ref() {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("overlay-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: surface_view,
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
                pass.set_pipeline(&self.overlay_pipeline);
                pass.set_bind_group(0, &self.overlay_bind_group, &[]);
                pass.set_vertex_buffer(0, self.overlay_vertex_buffer.slice(..));
                pass.draw(0..overlay_vert_count, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        if let Some(output) = output {
            output.present();
        }

        if let Some(cap) = capture.as_mut() {
            let buffer_slice = cap.staging.slice(..);
            let (tx, rx) = std::sync::mpsc::channel();
            buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
            self.device.poll(wgpu::Maintain::Wait);
            if rx.recv().ok().and_then(|r| r.ok()).is_some() {
                let data = buffer_slice.get_mapped_range();
                let mut rgba = Vec::with_capacity((cap.width * cap.height * 4) as usize);
                let row_bytes = (cap.width * 4) as usize;
                for y in 0..cap.height as usize {
                    let start = y * cap.bytes_per_row as usize;
                    rgba.extend_from_slice(&data[start..start + row_bytes]);
                }
                drop(data);
                cap.staging.unmap();
                cap.last_rgba = Some(rgba);
            }
        }
        self.capture = capture;
        Ok(())
    }
}

impl FrameCapture {
    fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
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
            blit_bgl,
            blit_sampler,
            width,
            height,
            bytes_per_row,
            last_rgba: None,
        }
    }
}

fn align_bytes_per_row(bytes: u32) -> u32 {
    (bytes + 255) & !255
}

fn create_depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}
