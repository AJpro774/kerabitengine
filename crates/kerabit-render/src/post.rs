//! Cheap bloom + ACES tonemap post stack (M1).
//!
//! Scene renders into an HDR color target; post extracts brights, blurs at half
//! resolution, then composites with tonemap onto the swapchain.

const SHADER_POST: &str = include_str!("../shaders/post.wgsl");

pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const BLOOM_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct BlurParams {
    dir: [f32; 4],
}

/// Owns HDR scene target + half-res bloom ping-pong + post pipelines.
pub struct PostStack {
    pub hdr_view: wgpu::TextureView,
    _hdr_texture: wgpu::Texture,
    bloom_a_view: wgpu::TextureView,
    _bloom_a: wgpu::Texture,
    bloom_b_view: wgpu::TextureView,
    _bloom_b: wgpu::Texture,
    width: u32,
    height: u32,
    sampler: wgpu::Sampler,
    tex_bgl: wgpu::BindGroupLayout,
    _blur_bgl: wgpu::BindGroupLayout,
    composite_bgl: wgpu::BindGroupLayout,
    extract_pipeline: wgpu::RenderPipeline,
    blur_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    blur_buffer: wgpu::Buffer,
    blur_bind_group: wgpu::BindGroup,
    extract_bg: wgpu::BindGroup,
    blur_src_a_bg: wgpu::BindGroup,
    blur_src_b_bg: wgpu::BindGroup,
    composite_bg: wgpu::BindGroup,
}

impl PostStack {
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
        let width = width.max(1);
        let height = height.max(1);

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("post-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let tex_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post-tex-bgl"),
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

        let blur_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post-blur-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let composite_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post-composite-bgl"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post"),
            source: wgpu::ShaderSource::Wgsl(SHADER_POST.into()),
        });

        let extract_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post-extract-layout"),
            bind_group_layouts: &[&tex_bgl],
            push_constant_ranges: &[],
        });
        let blur_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post-blur-layout"),
            bind_group_layouts: &[&tex_bgl, &blur_bgl],
            push_constant_ranges: &[],
        });
        let composite_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post-composite-layout"),
            bind_group_layouts: &[&composite_bgl],
            push_constant_ranges: &[],
        });

        let extract_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("post-extract"),
            layout: Some(&extract_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_extract"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: BLOOM_FORMAT,
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

        let blur_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("post-blur"),
            layout: Some(&blur_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_blur"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: BLOOM_FORMAT,
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

        let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("post-composite"),
            layout: Some(&composite_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_composite"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
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

        let blur_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("post-blur-params"),
            size: std::mem::size_of::<BlurParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let blur_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post-blur-bg"),
            layout: &blur_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: blur_buffer.as_entire_binding(),
            }],
        });

        let (hdr_texture, hdr_view, bloom_a, bloom_a_view, bloom_b, bloom_b_view) =
            create_targets(device, width, height);

        let extract_bg = make_tex_bg(device, &tex_bgl, &hdr_view, &sampler, "post-extract-bg");
        let blur_src_a_bg = make_tex_bg(device, &tex_bgl, &bloom_a_view, &sampler, "post-blur-a-bg");
        let blur_src_b_bg = make_tex_bg(device, &tex_bgl, &bloom_b_view, &sampler, "post-blur-b-bg");
        // Final blur lands in bloom_a (after vertical pass).
        let composite_bg =
            make_composite_bg(device, &composite_bgl, &hdr_view, &bloom_a_view, &sampler);

        Self {
            hdr_view,
            _hdr_texture: hdr_texture,
            bloom_a_view,
            _bloom_a: bloom_a,
            bloom_b_view,
            _bloom_b: bloom_b,
            width,
            height,
            sampler,
            tex_bgl,
            _blur_bgl: blur_bgl,
            composite_bgl,
            extract_pipeline,
            blur_pipeline,
            composite_pipeline,
            blur_buffer,
            blur_bind_group,
            extract_bg,
            blur_src_a_bg,
            blur_src_b_bg,
            composite_bg,
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;
        let (hdr_texture, hdr_view, bloom_a, bloom_a_view, bloom_b, bloom_b_view) =
            create_targets(device, width, height);
        self._hdr_texture = hdr_texture;
        self.hdr_view = hdr_view;
        self._bloom_a = bloom_a;
        self.bloom_a_view = bloom_a_view;
        self._bloom_b = bloom_b;
        self.bloom_b_view = bloom_b_view;
        self.extract_bg =
            make_tex_bg(device, &self.tex_bgl, &self.hdr_view, &self.sampler, "post-extract-bg");
        self.blur_src_a_bg = make_tex_bg(
            device,
            &self.tex_bgl,
            &self.bloom_a_view,
            &self.sampler,
            "post-blur-a-bg",
        );
        self.blur_src_b_bg = make_tex_bg(
            device,
            &self.tex_bgl,
            &self.bloom_b_view,
            &self.sampler,
            "post-blur-b-bg",
        );
        self.composite_bg = make_composite_bg(
            device,
            &self.composite_bgl,
            &self.hdr_view,
            &self.bloom_a_view,
            &self.sampler,
        );
    }

    /// Extract → blur H/V → tonemap+bloom into `surface_view`.
    pub fn encode(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        surface_view: &wgpu::TextureView,
    ) {
        let bw = (self.width / 2).max(1) as f32;
        let bh = (self.height / 2).max(1) as f32;

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post-extract"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.bloom_a_view,
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
            pass.set_pipeline(&self.extract_pipeline);
            pass.set_bind_group(0, &self.extract_bg, &[]);
            pass.draw(0..3, 0..1);
        }

        // Horizontal blur → B
        queue.write_buffer(
            &self.blur_buffer,
            0,
            bytemuck::bytes_of(&BlurParams {
                dir: [1.0 / bw, 0.0, 0.0, 0.0],
            }),
        );
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post-blur-h"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.bloom_b_view,
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
            pass.set_pipeline(&self.blur_pipeline);
            pass.set_bind_group(0, &self.blur_src_a_bg, &[]);
            pass.set_bind_group(1, &self.blur_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        // Vertical blur → A (composite samples bloom_a)
        queue.write_buffer(
            &self.blur_buffer,
            0,
            bytemuck::bytes_of(&BlurParams {
                dir: [0.0, 1.0 / bh, 0.0, 0.0],
            }),
        );
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post-blur-v"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.bloom_a_view,
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
            pass.set_pipeline(&self.blur_pipeline);
            pass.set_bind_group(0, &self.blur_src_b_bg, &[]);
            pass.set_bind_group(1, &self.blur_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post-composite"),
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
            pass.set_pipeline(&self.composite_pipeline);
            pass.set_bind_group(0, &self.composite_bg, &[]);
            pass.draw(0..3, 0..1);
        }
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
    wgpu::Texture,
    wgpu::TextureView,
) {
    let hdr = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("hdr-color"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: HDR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let hdr_view = hdr.create_view(&wgpu::TextureViewDescriptor::default());

    let bw = (width / 2).max(1);
    let bh = (height / 2).max(1);
    let bloom_a = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bloom-a"),
        size: wgpu::Extent3d {
            width: bw,
            height: bh,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: BLOOM_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let bloom_a_view = bloom_a.create_view(&wgpu::TextureViewDescriptor::default());
    let bloom_b = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bloom-b"),
        size: wgpu::Extent3d {
            width: bw,
            height: bh,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: BLOOM_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let bloom_b_view = bloom_b.create_view(&wgpu::TextureViewDescriptor::default());

    (hdr, hdr_view, bloom_a, bloom_a_view, bloom_b, bloom_b_view)
}

fn make_tex_bg(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    label: &str,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn make_composite_bg(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    hdr: &wgpu::TextureView,
    bloom: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("post-composite-bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(hdr),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(bloom),
            },
        ],
    })
}
