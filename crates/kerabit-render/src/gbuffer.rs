//! Depth prepass + thin G-buffer (world normal / roughness, motion vectors)
//! and the Hi-Z depth pyramid used by SSR.

use crate::mesh_gpu::MeshCache;
use crate::uniforms::InstanceRaw;
use crate::vertex::Vertex;
use crate::MeshId;

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub const NORMAL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub const VELOCITY_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;
pub const HIZ_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;
/// Hi-Z levels (level 0 = full-res depth copy).
pub const HIZ_MIPS: u32 = 5;

const SHADER_PREPASS: &str = include_str!("../shaders/prepass.wgsl");
const SHADER_HIZ: &str = include_str!("../shaders/hiz.wgsl");

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct HizParams {
    mode: [f32; 4],
}

/// Screen-sized geometry buffers written by the prepass.
pub struct GBuffer {
    width: u32,
    height: u32,
    _depth: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
    _normal: wgpu::Texture,
    pub normal_view: wgpu::TextureView,
    _velocity: wgpu::Texture,
    pub velocity_view: wgpu::TextureView,
    _hiz: wgpu::Texture,
    /// All Hi-Z mips (for sampling with explicit level).
    pub hiz_view: wgpu::TextureView,
    hiz_mip_views: Vec<wgpu::TextureView>,
    prepass_pipeline: wgpu::RenderPipeline,
    hiz_pipeline: wgpu::RenderPipeline,
    hiz_bgl: wgpu::BindGroupLayout,
    hiz_bind_groups: Vec<wgpu::BindGroup>,
    hiz_params: [wgpu::Buffer; 2],
}

impl GBuffer {
    pub fn new(
        device: &wgpu::Device,
        frame_bgl: &wgpu::BindGroupLayout,
        frame_wgsl: &str,
        width: u32,
        height: u32,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("prepass"),
            source: wgpu::ShaderSource::Wgsl(format!("{frame_wgsl}\n{SHADER_PREPASS}").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("prepass-layout"),
            bind_group_layouts: &[frame_bgl],
            push_constant_ranges: &[],
        });
        let prepass_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("prepass-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout(), InstanceRaw::layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[
                    Some(wgpu::ColorTargetState {
                        format: NORMAL_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(wgpu::ColorTargetState {
                        format: VELOCITY_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                ],
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

        let hiz_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hiz"),
            source: wgpu::ShaderSource::Wgsl(SHADER_HIZ.into()),
        });
        let hiz_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hiz-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let hiz_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hiz-layout"),
            bind_group_layouts: &[&hiz_bgl],
            push_constant_ranges: &[],
        });
        let hiz_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hiz-pipeline"),
            layout: Some(&hiz_layout),
            vertex: wgpu::VertexState {
                module: &hiz_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &hiz_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HIZ_FORMAT,
                    blend: None,
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
        let hiz_params = [
            uniform_buffer(device, "hiz-params-copy", &HizParams { mode: [1.0, 0.0, 0.0, 0.0] }),
            uniform_buffer(device, "hiz-params-reduce", &HizParams { mode: [0.0; 4] }),
        ];

        let mut this = Self {
            width: 0,
            height: 0,
            _depth: dummy_texture(device),
            depth_view: dummy_view(device),
            _normal: dummy_texture(device),
            normal_view: dummy_view(device),
            _velocity: dummy_texture(device),
            velocity_view: dummy_view(device),
            _hiz: dummy_texture(device),
            hiz_view: dummy_view(device),
            hiz_mip_views: Vec::new(),
            prepass_pipeline,
            hiz_pipeline,
            hiz_bgl,
            hiz_bind_groups: Vec::new(),
            hiz_params,
        };
        this.resize(device, width, height);
        this
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;
        let make = |label: &str, format: wgpu::TextureFormat, mips: u32| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: mips,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let depth = make("gbuffer-depth", DEPTH_FORMAT, 1);
        let normal = make("gbuffer-normal-rough", NORMAL_FORMAT, 1);
        let velocity = make("gbuffer-velocity", VELOCITY_FORMAT, 1);
        let hiz_mips = HIZ_MIPS.min(width.max(height).ilog2()).max(2);
        let hiz = make("hiz", HIZ_FORMAT, hiz_mips);

        self.depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        self.normal_view = normal.create_view(&wgpu::TextureViewDescriptor::default());
        self.velocity_view = velocity.create_view(&wgpu::TextureViewDescriptor::default());
        self.hiz_view = hiz.create_view(&wgpu::TextureViewDescriptor::default());
        self.hiz_mip_views = (0..hiz_mips)
            .map(|level| {
                hiz.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("hiz-mip"),
                    base_mip_level: level,
                    mip_level_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();

        // Level 0 reads scene depth (its hiz slot must not alias the mip being
        // written, so bind the last mip); level i reads level i-1.
        let mut groups = Vec::with_capacity(hiz_mips as usize);
        let last = hiz_mips as usize - 1;
        for level in 0..hiz_mips as usize {
            let src_hiz = if level == 0 {
                &self.hiz_mip_views[last.max(1)]
            } else {
                &self.hiz_mip_views[level - 1]
            };
            let params = if level == 0 { &self.hiz_params[0] } else { &self.hiz_params[1] };
            groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("hiz-bg"),
                layout: &self.hiz_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.depth_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(src_hiz),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: params.as_entire_binding(),
                    },
                ],
            }));
        }
        self.hiz_bind_groups = groups;
        self._depth = depth;
        self._normal = normal;
        self._velocity = velocity;
        self._hiz = hiz;
    }

    /// Clear depth / normals / velocity and rasterize every batch.
    pub fn encode_prepass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        frame_bind_group: &wgpu::BindGroup,
        mesh_cache: &MeshCache,
        instance_buffer: &wgpu::Buffer,
        ranges: &[(MeshId, u32, u32)],
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("prepass"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: &self.normal_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 1.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: &self.velocity_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                }),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.prepass_pipeline);
        pass.set_bind_group(0, frame_bind_group, &[]);
        draw_ranges(&mut pass, mesh_cache, instance_buffer, ranges);
    }

    /// Build the Hi-Z pyramid from the current depth.
    pub fn encode_hiz(&self, encoder: &mut wgpu::CommandEncoder) {
        for (level, view) in self.hiz_mip_views.iter().enumerate() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("hiz-level"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.hiz_pipeline);
            pass.set_bind_group(0, &self.hiz_bind_groups[level], &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

/// Issue one indexed instanced draw per batch range (shared by prepass / lit / shadow).
pub(crate) fn draw_ranges(
    pass: &mut wgpu::RenderPass<'_>,
    mesh_cache: &MeshCache,
    instance_buffer: &wgpu::Buffer,
    ranges: &[(MeshId, u32, u32)],
) {
    for &(mesh_id, start, count) in ranges {
        let Some(gpu_mesh) = mesh_cache.get(mesh_id) else {
            continue;
        };
        let byte_offset = start as u64 * std::mem::size_of::<InstanceRaw>() as u64;
        let byte_size = count as u64 * std::mem::size_of::<InstanceRaw>() as u64;
        pass.set_vertex_buffer(0, gpu_mesh.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, instance_buffer.slice(byte_offset..byte_offset + byte_size));
        pass.set_index_buffer(gpu_mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..gpu_mesh.index_count, 0, 0..count);
    }
}

pub(crate) fn uniform_buffer<T: bytemuck::Pod>(device: &wgpu::Device, label: &str, value: &T) -> wgpu::Buffer {
    use wgpu::util::DeviceExt as _;
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::bytes_of(value),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    })
}

fn dummy_texture(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("placeholder"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn dummy_view(device: &wgpu::Device) -> wgpu::TextureView {
    dummy_texture(device).create_view(&wgpu::TextureViewDescriptor::default())
}
