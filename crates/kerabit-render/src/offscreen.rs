//! Offscreen lit pass for embedding kerabit-render scenes in host windows
//! (e.g. egui viewports). Owns color+depth targets; no winit surface.

use kerabit_color::Color;

use crate::camera::Camera;
use crate::light::Light;
use crate::mesh::Mesh;
use crate::mesh_gpu::{MeshCache, MeshId};
use crate::shadow::{directional_light_matrix, ShadowMap, SHADOW_HALF_EXTENT};
use crate::sky::SkyPass;
use crate::texture::{TextureCache, TextureId};
use crate::uniforms::{pack_draw_batches, DrawItem, FrameUniforms, InstanceRaw, MAX_INSTANCES};
use crate::vertex::Vertex;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const SHADER_LIT: &str = include_str!("../shaders/lit.wgsl");
const SHADER_BLIT: &str = include_str!("../shaders/blit.wgsl");

/// Lit scene renderer targeting an offscreen color texture (plus depth).
pub struct OffscreenLitRenderer {
    pub clear_color: Color,
    pipeline: wgpu::RenderPipeline,
    frame_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    frame_bind_group: wgpu::BindGroup,
    mesh_cache: MeshCache,
    texture_cache: TextureCache,
    shadow: ShadowMap,
    sky: SkyPass,
    width: u32,
    height: u32,
    color_texture: wgpu::Texture,
    color_view: wgpu::TextureView,
    _depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
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
        let texture_cache = TextureCache::new(device, queue);
        let shadow = ShadowMap::new(device);
        let sky = SkyPass::new(device, COLOR_FORMAT);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("offscreen-lit"),
            source: wgpu::ShaderSource::Wgsl(SHADER_LIT.into()),
        });

        let frame_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("offscreen-frame-bgl"),
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

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("offscreen-lit-layout"),
            bind_group_layouts: &[
                &frame_bgl,
                texture_cache.bind_group_layout(),
                &shadow.bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("offscreen-lit-pipeline"),
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
                    format: COLOR_FORMAT,
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
            label: Some("offscreen-frame-uniforms"),
            size: std::mem::size_of::<FrameUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("offscreen-instance-buffer"),
            size: (std::mem::size_of::<InstanceRaw>() * MAX_INSTANCES) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let frame_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("offscreen-frame-bg"),
            layout: &frame_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame_buffer.as_entire_binding(),
            }],
        });

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

        let (color_texture, color_view, depth_texture, depth_view) = create_targets(device, 1, 1);
        let blit_bind_group = make_blit_bind_group(device, &blit_bgl, &color_view, &blit_sampler);

        Self {
            clear_color,
            pipeline,
            frame_buffer,
            instance_buffer,
            frame_bind_group,
            mesh_cache: MeshCache::new(),
            texture_cache,
            shadow,
            sky,
            width: 1,
            height: 1,
            color_texture,
            color_view,
            _depth_texture: depth_texture,
            depth_view,
            blit_pipeline,
            blit_bgl,
            blit_sampler,
            blit_bind_group,
        }
    }

    pub fn upload_mesh(&mut self, device: &wgpu::Device, mesh: &Mesh) -> MeshId {
        self.mesh_cache.upload(device, mesh)
    }

    pub fn upload_texture_rgba8(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> TextureId {
        self.texture_cache
            .upload_rgba8(device, queue, width, height, rgba)
    }

    pub fn white_texture(&self) -> TextureId {
        self.texture_cache.white()
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;
        let (color_texture, color_view, depth_texture, depth_view) =
            create_targets(device, width, height);
        self.color_texture = color_texture;
        self.color_view = color_view;
        self._depth_texture = depth_texture;
        self.depth_view = depth_view;
        self.blit_bind_group =
            make_blit_bind_group(device, &self.blit_bgl, &self.color_view, &self.blit_sampler);
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Encode shadow → sky → lit into `encoder` (color+depth offscreen).
    pub fn encode_lit(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        camera: &mut Camera,
        light: &Light,
        ambient: Color,
        draws: &[DrawItem],
    ) {
        camera.set_aspect(self.width as f32 / self.height.max(1) as f32);
        let light_vp = directional_light_matrix(light.direction, camera.target, SHADOW_HALF_EXTENT);
        let frame = FrameUniforms::from_scene(camera, light, ambient, light_vp);
        queue.write_buffer(&self.frame_buffer, 0, bytemuck::bytes_of(&frame));

        let white = self.texture_cache.white();
        let (flat, ranges) = pack_draw_batches(draws, white);
        if !flat.is_empty() {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&flat));
        }

        let shadow_ranges: Vec<(MeshId, u32, u32)> = ranges
            .iter()
            .map(|&(mesh, _, start, count)| (mesh, start, count))
            .collect();

        self.shadow.encode(
            queue,
            encoder,
            light_vp,
            &self.mesh_cache,
            &self.instance_buffer,
            &shadow_ranges,
        );

        self.sky.encode(
            queue,
            encoder,
            &self.color_view,
            &self.depth_view,
            self.clear_color,
        );

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("offscreen-lit-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.color_view,
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

            for (mesh_id, tex_id, start, count) in ranges {
                let Some(gpu_mesh) = self.mesh_cache.get(mesh_id) else {
                    continue;
                };
                let Some(tex_bg) = self.texture_cache.bind_group(tex_id) else {
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
    }

    /// Draw the offscreen color target into an existing render pass (egui).
    pub fn blit_into(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        render_pass.set_pipeline(&self.blit_pipeline);
        render_pass.set_bind_group(0, &self.blit_bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}

fn create_targets(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (
    wgpu::Texture,
    wgpu::TextureView,
    wgpu::Texture,
    wgpu::TextureView,
) {
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

    let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen-depth"),
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
    let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

    (color_texture, color_view, depth_texture, depth_view)
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
