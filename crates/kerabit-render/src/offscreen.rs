//! Offscreen lit pass for embedding kerabit-render scenes in host windows
//! (e.g. egui viewports). Runs the same [`SceneRenderer`] chain as games,
//! tonemaps into an sRGB color target, and blits it into a host render pass.

use kerabit_color::Color;

use crate::camera::Camera;
use crate::light::Light;
use crate::mesh::Mesh;
use crate::mesh_gpu::MeshId;
use crate::post::PostStack;
use crate::scene_renderer::SceneRenderer;
use crate::texture::TextureId;
use crate::uniforms::{DrawItem, RenderSettings};

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const SHADER_BLIT: &str = include_str!("../shaders/blit.wgsl");

/// Lit scene renderer targeting an offscreen color texture.
pub struct OffscreenLitRenderer {
    pub clear_color: Color,
    renderer: SceneRenderer,
    post: PostStack,
    width: u32,
    height: u32,
    _color_texture: wgpu::Texture,
    color_view: wgpu::TextureView,
    /// Blit into a host render pass (egui surface).
    blit_pipeline: wgpu::RenderPipeline,
    blit_bgl: wgpu::BindGroupLayout,
    blit_sampler: wgpu::Sampler,
    blit_bind_group: wgpu::BindGroup,
}

impl OffscreenLitRenderer {
    /// Build pipelines and a starter 1×1 target using the host `device` / `queue`.
    ///
    /// `blit_target_format` must match the egui / swapchain format.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        blit_target_format: wgpu::TextureFormat,
        clear_color: Color,
    ) -> Self {
        let renderer = SceneRenderer::new(device, queue, 1, 1);
        let mut post = PostStack::new(device, COLOR_FORMAT, 1, 1);
        post.set_source(device, renderer.output_view());

        let blit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit"),
            source: wgpu::ShaderSource::Wgsl(SHADER_BLIT.into()),
        });
        let blit_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit-bgl"),
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
            label: Some("blit-layout"),
            bind_group_layouts: &[&blit_bgl],
            push_constant_ranges: &[],
        });
        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit-pipeline"),
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
                    format: blit_target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
            label: Some("blit-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let (color_texture, color_view) = create_color_target(device, 1, 1);
        let blit_bind_group = make_blit_bind_group(device, &blit_bgl, &color_view, &blit_sampler);

        Self {
            clear_color,
            renderer,
            post,
            width: 1,
            height: 1,
            _color_texture: color_texture,
            color_view,
            blit_pipeline,
            blit_bgl,
            blit_sampler,
            blit_bind_group,
        }
    }

    pub fn upload_mesh(&mut self, device: &wgpu::Device, mesh: &Mesh) -> MeshId {
        self.renderer.mesh_cache.upload(device, mesh)
    }

    pub fn upload_texture_rgba8(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> TextureId {
        self.renderer
            .texture_cache
            .upload_rgba8(device, queue, width, height, rgba)
    }

    pub fn white_texture(&self) -> TextureId {
        self.renderer.texture_cache.white()
    }

    /// Toggle SSAO / IBL / SSR / TAA for the viewport.
    pub fn settings_mut(&mut self) -> &mut RenderSettings {
        &mut self.renderer.settings
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;
        self.renderer.resize(device, width, height);
        self.post.resize(device, width, height);
        self.post.set_source(device, self.renderer.output_view());
        let (color_texture, color_view) = create_color_target(device, width, height);
        self._color_texture = color_texture;
        self.color_view = color_view;
        self.blit_bind_group =
            make_blit_bind_group(device, &self.blit_bgl, &self.color_view, &self.blit_sampler);
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Encode the full scene chain + tonemap into the offscreen color target.
    #[allow(clippy::too_many_arguments)]
    pub fn encode_lit(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        camera: &mut Camera,
        light: &Light,
        ambient: Color,
        draws: &[DrawItem],
    ) {
        self.renderer.encode_scene(
            device,
            queue,
            encoder,
            camera,
            std::slice::from_ref(light),
            ambient,
            self.clear_color,
            draws,
        );
        self.renderer.encode_finish(encoder);
        self.post.encode(queue, encoder, &self.color_view);
    }

    /// Draw the offscreen color target into an existing render pass (egui).
    pub fn blit_into(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        render_pass.set_pipeline(&self.blit_pipeline);
        render_pass.set_bind_group(0, &self.blit_bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}

fn create_color_target(device: &wgpu::Device, width: u32, height: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let color_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen-color"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: COLOR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());
    (color_texture, color_view)
}

fn make_blit_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    color_view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("blit-bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(color_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}
