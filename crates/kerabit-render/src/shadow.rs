//! Directional sun shadow map (depth-only pass + comparison sampler).

use kerabit_math::{Mat4, Vec3};

use crate::uniforms::InstanceRaw;
use crate::vertex::Vertex;

/// Shadow map resolution (square). Soft PCF samples neighboring texels.
pub const SHADOW_MAP_SIZE: u32 = 2048;
/// Half-extent of the orthographic light frustum around the focus point (world units).
pub const SHADOW_HALF_EXTENT: f32 = 32.0;
/// Constant depth bias written into frame uniforms (plus slope term in the pipeline).
pub const SHADOW_BIAS: f32 = 0.002;

const SHADER_SHADOW: &str = include_str!("../shaders/shadow.wgsl");

/// GPU uniforms for the depth-only shadow pass.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ShadowUniforms {
    pub light_view_proj: [[f32; 4]; 4],
}

/// Orthographic light view-projection for a directional sun.
///
/// `light_dir` is travel direction (sun → scene). `focus` is typically the
/// camera look-at target so Reach grounds stay inside the cascade.
pub fn directional_light_matrix(light_dir: Vec3, focus: Vec3, half_extent: f32) -> Mat4 {
    let mut dir = light_dir.normalize_or_zero();
    if dir.length_squared() < 1e-8 {
        dir = Vec3::NEG_Y;
    }
    let distance = half_extent * 2.5;
    let eye = focus - dir * distance;
    let up = if dir.cross(Vec3::Y).length_squared() < 1e-4 {
        Vec3::X
    } else {
        Vec3::Y
    };
    let view = Mat4::look_at_rh(eye, focus, up);
    let near = 0.5;
    let far = distance + half_extent * 2.0;
    let proj = Mat4::orthographic_rh(
        -half_extent,
        half_extent,
        -half_extent,
        half_extent,
        near,
        far,
    );
    proj * view
}

/// Depth texture + comparison sampler + depth-only pipeline for the sun shadow map.
pub struct ShadowMap {
    pub view: wgpu::TextureView,
    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub pipeline: wgpu::RenderPipeline,
    pub uniform_buffer: wgpu::Buffer,
    pub uniform_bind_group: wgpu::BindGroup,
    _texture: wgpu::Texture,
    _sampler: wgpu::Sampler,
}

impl ShadowMap {
    pub fn new(device: &wgpu::Device) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow-map"),
            size: wgpu::Extent3d {
                width: SHADOW_MAP_SIZE,
                height: SHADOW_MAP_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow-comparison-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow-sample-bgl"),
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
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow-sample-bg"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let uniform_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow-pass-bgl"),
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

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow-uniforms"),
            size: std::mem::size_of::<ShadowUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow-pass-bg"),
            layout: &uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shadow"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SHADOW.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow-pipeline-layout"),
            bind_group_layouts: &[&uniform_bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout(), InstanceRaw::layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                // Front-face cull reduces shadow acne on thin geometry.
                cull_mode: Some(wgpu::Face::Front),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 1.5,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            view,
            bind_group,
            bind_group_layout,
            pipeline,
            uniform_buffer,
            uniform_bind_group,
            _texture: texture,
            _sampler: sampler,
        }
    }

    /// Upload light matrix and encode the depth-only shadow pass.
    pub fn encode(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        light_view_proj: Mat4,
        mesh_cache: &crate::mesh_gpu::MeshCache,
        instance_buffer: &wgpu::Buffer,
        ranges: &[(crate::MeshId, u32, u32)],
    ) {
        let uniforms = ShadowUniforms {
            light_view_proj: light_view_proj.to_cols_array_2d(),
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow-pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.view,
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
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);

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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kerabit_math::vec3;

    #[test]
    fn light_matrix_is_finite() {
        let m = directional_light_matrix(vec3(-0.35, -1.0, -0.25), Vec3::ZERO, 32.0);
        for v in m.to_cols_array() {
            assert!(v.is_finite());
        }
    }
}
