//! Simple CPU particle billboards for games (M1).

use kerabit_color::Color;
use kerabit_math::Vec3;

use crate::camera::Camera;

const SHADER_PARTICLE: &str = include_str!("../shaders/particle.wgsl");

/// Soft cap so a burst cannot flood the GPU.
pub const MAX_PARTICLES: usize = 1024;

/// One-shot / continuous-friendly burst descriptor (public via facade).
#[derive(Clone, Debug)]
pub struct ParticleBurst {
    pub origin: Vec3,
    pub count: u32,
    pub color: Color,
    pub size: f32,
    pub speed: f32,
    pub lifetime: f32,
    /// Directional bias (world); zero = isotropic spray.
    pub velocity: Vec3,
    /// Random cone half-angle scale (0..=1-ish).
    pub spread: f32,
}

impl Default for ParticleBurst {
    fn default() -> Self {
        Self {
            origin: Vec3::ZERO,
            count: 24,
            color: Color::ORANGE,
            size: 0.12,
            speed: 2.5,
            lifetime: 0.7,
            velocity: Vec3::Y,
            spread: 0.85,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct ParticleGpu {
    pos_size: [f32; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct ParticleFrame {
    view_proj: [[f32; 4]; 4],
    camera_right: [f32; 4],
    camera_up: [f32; 4],
}

struct LiveParticle {
    pos: Vec3,
    vel: Vec3,
    color: Color,
    size: f32,
    age: f32,
    lifetime: f32,
}

/// CPU-simulated billboard particles + GPU draw path.
pub struct ParticleSystem {
    live: Vec<LiveParticle>,
    pipeline: wgpu::RenderPipeline,
    frame_buffer: wgpu::Buffer,
    frame_bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    scratch: Vec<ParticleGpu>,
    rng: u32,
}

impl ParticleSystem {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particle"),
            source: wgpu::ShaderSource::Wgsl(SHADER_PARTICLE.into()),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particle-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let frame_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle-frame"),
            size: std::mem::size_of::<ParticleFrame>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("particle-frame-bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame_buffer.as_entire_binding(),
            }],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particle-layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("particle-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<ParticleGpu>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x4,
                        1 => Float32x4,
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle-instances"),
            size: (std::mem::size_of::<ParticleGpu>() * MAX_PARTICLES) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            live: Vec::new(),
            pipeline,
            frame_buffer,
            frame_bind_group,
            instance_buffer,
            scratch: Vec::with_capacity(256),
            rng: 0xC0FFEE,
        }
    }

    fn next_f01(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.rng >> 8) as f32 / 16_777_216.0
    }

    /// Emit a burst (clamped so total live ≤ [`MAX_PARTICLES`]).
    pub fn emit(&mut self, burst: &ParticleBurst) {
        let room = MAX_PARTICLES.saturating_sub(self.live.len());
        let n = (burst.count as usize).min(room);
        for _ in 0..n {
            let rx = self.next_f01() * 2.0 - 1.0;
            let ry = self.next_f01() * 2.0 - 1.0;
            let rz = self.next_f01() * 2.0 - 1.0;
            let jitter = Vec3::new(rx, ry, rz) * burst.spread;
            let dir = (burst.velocity + jitter).normalize_or_zero();
            let speed = burst.speed * (0.6 + 0.8 * self.next_f01());
            let size = burst.size * (0.7 + 0.6 * self.next_f01());
            let lifetime = burst.lifetime.max(0.05) * (0.7 + 0.6 * self.next_f01());
            self.live.push(LiveParticle {
                pos: burst.origin,
                vel: dir * speed,
                color: burst.color,
                size,
                age: 0.0,
                lifetime,
            });
        }
    }

    pub fn clear(&mut self) {
        self.live.clear();
    }

    pub fn update(&mut self, dt: f32) {
        for p in &mut self.live {
            p.age += dt;
            p.vel.y -= 3.5 * dt;
            p.pos += p.vel * dt;
        }
        self.live.retain(|p| p.age < p.lifetime);
    }

    pub fn encode(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        camera: &Camera,
        color_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
    ) {
        if self.live.is_empty() {
            return;
        }

        let forward = (camera.target - camera.eye).normalize_or_zero();
        let right = forward.cross(camera.up).normalize_or_zero();
        let up = right.cross(forward).normalize_or_zero();
        let frame = ParticleFrame {
            view_proj: camera.view_proj().to_cols_array_2d(),
            camera_right: [right.x, right.y, right.z, 0.0],
            camera_up: [up.x, up.y, up.z, 0.0],
        };
        queue.write_buffer(&self.frame_buffer, 0, bytemuck::bytes_of(&frame));

        self.scratch.clear();
        for p in self.live.iter().take(MAX_PARTICLES) {
            let t = (p.age / p.lifetime).clamp(0.0, 1.0);
            let fade = 1.0 - t;
            let size = p.size * (1.0 - 0.35 * t);
            self.scratch.push(ParticleGpu {
                pos_size: [p.pos.x, p.pos.y, p.pos.z, size],
                color: [
                    p.color.r,
                    p.color.g,
                    p.color.b,
                    p.color.a * fade,
                ],
            });
        }
        let count = self.scratch.len() as u32;
        queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&self.scratch),
        );

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("particle-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
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
        pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        pass.draw(0..6, 0..count);
    }
}
