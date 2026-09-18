//! Half-resolution SSAO (hemisphere kernel over the G-buffer) + depth-aware blur.

use crate::gbuffer::uniform_buffer;

const SHADER_SSAO: &str = include_str!("../shaders/ssao.wgsl");
const AO_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R16Float;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SsaoKernel {
    samples: [[f32; 4]; 16],
    params: [f32; 4],
}

/// Hemisphere kernel: more samples near the origin, deterministic (no rand dep).
fn build_kernel() -> SsaoKernel {
    let mut samples = [[0.0f32; 4]; 16];
    let mut seed = 0x1234_5678u32;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed >> 8) as f32 / (1u32 << 24) as f32
    };
    for (i, s) in samples.iter_mut().enumerate() {
        let mut x;
        let mut y;
        let mut z;
        loop {
            x = next() * 2.0 - 1.0;
            y = next() * 2.0 - 1.0;
            z = next();
            let len2 = x * x + y * y + z * z;
            if len2 > 1e-3 && len2 <= 1.0 {
                break;
            }
        }
        let len = (x * x + y * y + z * z).sqrt();
        let t = i as f32 / 16.0;
        let scale = 0.1 + 0.9 * t * t;
        s[0] = x / len * scale;
        s[1] = y / len * scale;
        s[2] = z / len * scale;
    }
    SsaoKernel {
        samples,
        params: [0.7, 0.02, 1.4, 0.0],
    }
}

pub struct SsaoPass {
    width: u32,
    height: u32,
    _ao: wgpu::Texture,
    ao_view: wgpu::TextureView,
    _blur: wgpu::Texture,
    /// Final (blurred) visibility, half resolution.
    pub view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    kernel: wgpu::Buffer,
    bgl: wgpu::BindGroupLayout,
    ao_bg: wgpu::BindGroup,
    blur_bg: wgpu::BindGroup,
    ao_pipeline: wgpu::RenderPipeline,
    blur_pipeline: wgpu::RenderPipeline,
}

impl SsaoPass {
    pub fn new(
        device: &wgpu::Device,
        frame_bgl: &wgpu::BindGroupLayout,
        frame_wgsl: &str,
        depth_view: &wgpu::TextureView,
        normal_view: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ssao"),
            source: wgpu::ShaderSource::Wgsl(format!("{frame_wgsl}\n{SHADER_SSAO}").into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ssao-bgl"),
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
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
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
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
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
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ssao-layout"),
            bind_group_layouts: &[frame_bgl, &bgl],
            push_constant_ranges: &[],
        });
        let make_pipeline = |label: &str, entry: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: AO_FORMAT,
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
            })
        };
        let ao_pipeline = make_pipeline("ssao-ao", "fs_ao");
        let blur_pipeline = make_pipeline("ssao-blur", "fs_blur");
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ssao-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let kernel = uniform_buffer(device, "ssao-kernel", &build_kernel());
        let (ao, ao_view, blur, view) = create_targets(device, width, height);
        let (ao_bg, blur_bg) =
            make_bind_groups(device, &bgl, depth_view, normal_view, &kernel, &ao_view, &view, &sampler);
        Self {
            width: width.max(1),
            height: height.max(1),
            _ao: ao,
            ao_view,
            _blur: blur,
            view,
            sampler,
            kernel,
            bgl,
            ao_bg,
            blur_bg,
            ao_pipeline,
            blur_pipeline,
        }
    }

    /// Recreate targets for a new screen size (G-buffer views must be the new ones).
    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        depth_view: &wgpu::TextureView,
        normal_view: &wgpu::TextureView,
    ) {
        self.width = width.max(1);
        self.height = height.max(1);
        let (ao, ao_view, blur, view) = create_targets(device, width, height);
        self._ao = ao;
        self.ao_view = ao_view;
        self._blur = blur;
        self.view = view;
        let (ao_bg, blur_bg) = make_bind_groups(
            device,
            &self.bgl,
            depth_view,
            normal_view,
            &self.kernel,
            &self.ao_view,
            &self.view,
            &self.sampler,
        );
        self.ao_bg = ao_bg;
        self.blur_bg = blur_bg;
    }

    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder, frame_bind_group: &wgpu::BindGroup) {
        for (label, pipeline, bg, target) in [
            ("ssao-ao", &self.ao_pipeline, &self.ao_bg, &self.ao_view),
            ("ssao-blur", &self.blur_pipeline, &self.blur_bg, &self.view),
        ] {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
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
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, frame_bind_group, &[]);
            pass.set_bind_group(1, bg, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

fn create_targets(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView, wgpu::Texture, wgpu::TextureView) {
    let make = |label: &str| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: (width / 2).max(1),
                height: (height / 2).max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: AO_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    };
    let ao = make("ssao-raw");
    let ao_view = ao.create_view(&wgpu::TextureViewDescriptor::default());
    let blur = make("ssao-blurred");
    let blur_view = blur.create_view(&wgpu::TextureViewDescriptor::default());
    (ao, ao_view, blur, blur_view)
}

fn make_bind_groups(
    device: &wgpu::Device,
    bgl: &wgpu::BindGroupLayout,
    depth_view: &wgpu::TextureView,
    normal_view: &wgpu::TextureView,
    kernel: &wgpu::Buffer,
    ao_view: &wgpu::TextureView,
    blur_view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> (wgpu::BindGroup, wgpu::BindGroup) {
    let make = |label: &str, ao_src: &wgpu::TextureView| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(normal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: kernel.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(ao_src),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    };
    // A texture cannot be both attachment and binding in one pass: the AO pass
    // (target = raw) binds the blur target it never reads; the blur pass
    // (target = blurred) reads the raw AO.
    (make("ssao-ao-bg", blur_view), make("ssao-blur-bg", ao_view))
}
