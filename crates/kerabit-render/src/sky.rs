//! Fullscreen sky gradient pass (clear replacement).

use kerabit_color::Color;

const SHADER_SKY: &str = include_str!("../shaders/sky.wgsl");

/// GPU uniforms for the sky gradient.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SkyUniforms {
    pub top: [f32; 4],
    pub bottom: [f32; 4],
}

/// Derive a zenith color from the scene clear / horizon color.
///
/// No new Scene field: `clear_color` is the horizon; zenith lifts toward a
/// cooler daylight tint so Reach levels get a sky without authoring changes.
pub fn zenith_from_horizon(horizon: Color) -> Color {
    let cool = Color::rgb(0.42, 0.58, 0.82);
    Color::rgb(
        horizon.r * 0.35 + cool.r * 0.65,
        horizon.g * 0.35 + cool.g * 0.65,
        horizon.b * 0.35 + cool.b * 0.65,
    )
}

/// Fullscreen triangle pipeline that paints a vertical sky gradient.
pub struct SkyPass {
    pub pipeline: wgpu::RenderPipeline,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
}

impl SkyPass {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SKY.into()),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sky-bgl"),
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

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky-uniforms"),
            size: std::mem::size_of::<SkyUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky-bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky-pipeline-layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky-pipeline"),
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
                    format: color_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            uniform_buffer,
            bind_group,
        }
    }

    pub fn encode(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        color_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        horizon: Color,
    ) {
        let top = zenith_from_horizon(horizon);
        let uniforms = SkyUniforms {
            top: top.to_array(),
            bottom: horizon.to_array(),
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sky-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: horizon.r as f64,
                        g: horizon.g as f64,
                        b: horizon.b as f64,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}
