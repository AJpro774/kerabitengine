//! Specular resolve pass: SSR (Hi-Z assisted) with IBL fallback, composited
//! onto the lit color using the per-pixel specular weight from the lit pass.

use crate::post::HDR_FORMAT;

const SHADER_SSR: &str = include_str!("../shaders/ssr.wgsl");

/// Views the resolve reads from (all owned by the scene renderer).
pub struct SsrInputs<'a> {
    pub color: &'a wgpu::TextureView,
    pub spec: &'a wgpu::TextureView,
    pub depth: &'a wgpu::TextureView,
    pub normal: &'a wgpu::TextureView,
    pub hiz: &'a wgpu::TextureView,
    pub env_specular: &'a wgpu::TextureView,
}

pub struct SsrPass {
    bgl: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    bind_group: Option<wgpu::BindGroup>,
    _output: Option<wgpu::Texture>,
    /// Lit color + resolved specular (HDR). Stable until the size changes.
    pub output_view: Option<wgpu::TextureView>,
    size: (u32, u32),
}

impl SsrPass {
    pub fn new(device: &wgpu::Device, frame_bgl: &wgpu::BindGroupLayout, frame_wgsl: &str) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ssr"),
            source: wgpu::ShaderSource::Wgsl(format!("{frame_wgsl}\n{SHADER_SSR}").into()),
        });
        let tex = |binding: u32, sample_type: wgpu::TextureSampleType, dim: wgpu::TextureViewDimension| {
            wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type,
                    view_dimension: dim,
                    multisampled: false,
                },
                count: None,
            }
        };
        use wgpu::TextureSampleType as T;
        use wgpu::TextureViewDimension as D;
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ssr-bgl"),
            entries: &[
                tex(0, T::Float { filterable: true }, D::D2),
                tex(1, T::Float { filterable: true }, D::D2),
                tex(2, T::Depth, D::D2),
                tex(3, T::Float { filterable: true }, D::D2),
                tex(4, T::Float { filterable: false }, D::D2),
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                tex(6, T::Float { filterable: true }, D::Cube),
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ssr-layout"),
            bind_group_layouts: &[frame_bgl, &bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ssr-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR_FORMAT,
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ssr-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        Self {
            bgl,
            pipeline,
            sampler,
            bind_group: None,
            _output: None,
            output_view: None,
            size: (0, 0),
        }
    }

    /// Bind the current input views; the output target is only recreated when the size changes.
    pub fn rebind(&mut self, device: &wgpu::Device, width: u32, height: u32, inputs: &SsrInputs<'_>) {
        let size = (width.max(1), height.max(1));
        if self.output_view.is_none() || self.size != size {
            let output = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("hdr-resolved"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: HDR_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            self.output_view = Some(output.create_view(&wgpu::TextureViewDescriptor::default()));
            self._output = Some(output);
            self.size = size;
        }
        self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ssr-bg"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(inputs.color),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(inputs.spec),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(inputs.depth),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(inputs.normal),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(inputs.hiz),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(inputs.env_specular),
                },
            ],
        }));
    }

    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder, frame_bind_group: &wgpu::BindGroup) {
        let (Some(bg), Some(out)) = (&self.bind_group, &self.output_view) else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("specular-resolve"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: out,
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
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, frame_bind_group, &[]);
        pass.set_bind_group(1, bg, &[]);
        pass.draw(0..3, 0..1);
    }
}
