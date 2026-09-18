//! Cascaded directional shadow maps (4 cascades, stable texel-snapped fit).

use kerabit_math::{Mat4, Vec3, Vec4};

use crate::camera::Camera;
use crate::gbuffer::draw_ranges;
use crate::mesh_gpu::MeshCache;
use crate::MeshId;

/// Per-cascade shadow map resolution (square).
pub const SHADOW_MAP_SIZE: u32 = 2048;
/// Number of cascades along the view direction.
pub const CASCADE_COUNT: usize = 4;
/// Farthest view distance that receives cascaded shadows (world units).
pub const SHADOW_DISTANCE: f32 = 120.0;
/// Constant depth bias written into frame uniforms (plus slope term in the pipeline).
pub const SHADOW_BIAS: f32 = 0.0015;
/// Legacy single-cascade half extent (kept for [`directional_light_matrix`] callers).
pub const SHADOW_HALF_EXTENT: f32 = 32.0;

const SHADER_SHADOW: &str = include_str!("../shaders/shadow.wgsl");

/// GPU uniforms for one cascade's depth-only pass.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ShadowUniforms {
    pub light_view_proj: [[f32; 4]; 4],
}

/// Light view-projections + view-space split distances for one frame.
#[derive(Clone, Debug)]
pub struct CascadeSet {
    pub view_proj: [Mat4; CASCADE_COUNT],
    /// Far distance of each cascade (view space, positive).
    pub splits: [f32; CASCADE_COUNT],
    /// Width of the blend band before each split (view units).
    pub blend: f32,
}

impl Default for CascadeSet {
    fn default() -> Self {
        Self {
            view_proj: [Mat4::IDENTITY; CASCADE_COUNT],
            splits: [1.0e9; CASCADE_COUNT],
            blend: 1.0,
        }
    }
}

/// Orthographic light view-projection for a directional sun around `focus`
/// (single-cascade helper; cascades use [`fit_cascades`]).
pub fn directional_light_matrix(light_dir: Vec3, focus: Vec3, half_extent: f32) -> Mat4 {
    let dir = safe_dir(light_dir);
    let distance = half_extent * 2.5;
    let eye = focus - dir * distance;
    let view = Mat4::look_at_rh(eye, focus, light_up(dir));
    let proj = Mat4::orthographic_rh(
        -half_extent,
        half_extent,
        -half_extent,
        half_extent,
        0.5,
        distance + half_extent * 2.0,
    );
    proj * view
}

fn safe_dir(light_dir: Vec3) -> Vec3 {
    let dir = light_dir.normalize_or_zero();
    if dir.length_squared() < 1e-8 {
        Vec3::NEG_Y
    } else {
        dir
    }
}

fn light_up(dir: Vec3) -> Vec3 {
    if dir.cross(Vec3::Y).length_squared() < 1e-4 {
        Vec3::X
    } else {
        Vec3::Y
    }
}

/// Split the camera frustum into cascades (practical split scheme) and fit a
/// texel-snapped orthographic light frustum around each slice's bounding sphere.
pub fn fit_cascades(camera: &Camera, light_dir: Vec3) -> CascadeSet {
    let dir = safe_dir(light_dir);
    let near = camera.near.max(0.01);
    let far = camera.far.min(SHADOW_DISTANCE).max(near + 0.1);
    let lambda = 0.7;
    let mut bounds = [0.0f32; CASCADE_COUNT + 1];
    bounds[0] = near;
    for (i, bound) in bounds.iter_mut().enumerate().skip(1) {
        let s = i as f32 / CASCADE_COUNT as f32;
        let log = near * (far / near).powf(s);
        let uni = near + (far - near) * s;
        *bound = lambda * log + (1.0 - lambda) * uni;
    }

    let forward = (camera.target - camera.eye).normalize_or_zero();
    let forward = if forward.length_squared() < 1e-8 { Vec3::NEG_Z } else { forward };
    let right = forward.cross(camera.up).normalize_or_zero();
    let right = if right.length_squared() < 1e-8 { Vec3::X } else { right };
    let up = right.cross(forward).normalize_or_zero();
    let tan_half_v = (camera.fov_y_degrees.to_radians() * 0.5).tan();
    let tan_half_h = tan_half_v * camera.aspect.max(1e-4);

    let mut set = CascadeSet::default();
    let up_axis = light_up(dir);
    for c in 0..CASCADE_COUNT {
        let (d0, d1) = (bounds[c], bounds[c + 1]);
        let mut corners = [Vec3::ZERO; 8];
        let mut k = 0;
        for &d in &[d0, d1] {
            let hh = d * tan_half_h;
            let hv = d * tan_half_v;
            let center = camera.eye + forward * d;
            for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                corners[k] = center + right * (hh * sx) + up * (hv * sy);
                k += 1;
            }
        }
        let center = corners.iter().copied().sum::<Vec3>() / 8.0;
        let radius = corners
            .iter()
            .map(|p| p.distance(center))
            .fold(0.0f32, f32::max)
            .max(0.5);
        // Round the radius so the cascade extent is stable across frames.
        let radius = (radius * 16.0).ceil() / 16.0;

        let eye = center - dir * (radius * 2.0);
        let view = Mat4::look_at_rh(eye, center, up_axis);
        let mut proj = Mat4::orthographic_rh(-radius, radius, -radius, radius, 0.05, radius * 4.0);
        // Snap the light-space origin to a texel so a moving camera does not shimmer.
        let vp = proj * view;
        let half = SHADOW_MAP_SIZE as f32 * 0.5;
        let origin = vp * Vec4::new(0.0, 0.0, 0.0, 1.0) * half;
        let rounded = origin.round();
        let offset = (rounded - origin) / half;
        proj.w_axis.x += offset.x;
        proj.w_axis.y += offset.y;
        set.view_proj[c] = proj * view;
        set.splits[c] = d1;
    }
    set.blend = ((far - near) * 0.04).max(0.5);
    set
}

/// Depth array texture + comparison sampler + depth-only pipeline for cascades.
pub struct ShadowMap {
    /// All cascades as a 2D array (bind with `texture_depth_2d_array`).
    pub array_view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    layer_views: Vec<wgpu::TextureView>,
    pipeline: wgpu::RenderPipeline,
    uniform_buffers: Vec<wgpu::Buffer>,
    uniform_bind_groups: Vec<wgpu::BindGroup>,
    _texture: wgpu::Texture,
}

impl ShadowMap {
    pub fn new(device: &wgpu::Device) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow-cascades"),
            size: wgpu::Extent3d {
                width: SHADOW_MAP_SIZE,
                height: SHADOW_MAP_SIZE,
                depth_or_array_layers: CASCADE_COUNT as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let array_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("shadow-cascades-array"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let layer_views = (0..CASCADE_COUNT as u32)
            .map(|layer| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("shadow-cascade-layer"),
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: layer,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();

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
        let mut uniform_buffers = Vec::with_capacity(CASCADE_COUNT);
        let mut uniform_bind_groups = Vec::with_capacity(CASCADE_COUNT);
        for _ in 0..CASCADE_COUNT {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("shadow-cascade-uniforms"),
                size: std::mem::size_of::<ShadowUniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            uniform_bind_groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("shadow-cascade-bg"),
                layout: &uniform_bgl,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            }));
            uniform_buffers.push(buffer);
        }

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
                buffers: &[crate::vertex::Vertex::layout(), crate::uniforms::InstanceRaw::layout()],
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
            array_view,
            sampler,
            layer_views,
            pipeline,
            uniform_buffers,
            uniform_bind_groups,
            _texture: texture,
        }
    }

    /// Upload cascade matrices and encode one depth-only pass per cascade.
    pub fn encode(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        cascades: &CascadeSet,
        mesh_cache: &MeshCache,
        instance_buffer: &wgpu::Buffer,
        ranges: &[(MeshId, u32, u32)],
    ) {
        for c in 0..CASCADE_COUNT {
            let uniforms = ShadowUniforms {
                light_view_proj: cascades.view_proj[c].to_cols_array_2d(),
            };
            queue.write_buffer(&self.uniform_buffers[c], 0, bytemuck::bytes_of(&uniforms));
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow-cascade"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.layer_views[c],
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
            pass.set_bind_group(0, &self.uniform_bind_groups[c], &[]);
            draw_ranges(&mut pass, mesh_cache, instance_buffer, ranges);
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

    #[test]
    fn cascades_are_monotonic_and_cover_the_frustum() {
        let cam = Camera::perspective(60.0).look_at(vec3(0.0, 10.0, 15.0), Vec3::ZERO);
        let set = fit_cascades(&cam, vec3(-0.4, -1.0, -0.25));
        for i in 1..CASCADE_COUNT {
            assert!(set.splits[i] > set.splits[i - 1]);
        }
        assert!((set.splits[CASCADE_COUNT - 1] - cam.far.min(SHADOW_DISTANCE)).abs() < 1e-3);
        // A point in the middle of cascade 1 projects inside its light frustum.
        let d = (set.splits[0] + set.splits[1]) * 0.5;
        let p = cam.eye + (cam.target - cam.eye).normalize() * d;
        let clip = set.view_proj[1] * Vec4::new(p.x, p.y, p.z, 1.0);
        let ndc = clip / clip.w;
        assert!(ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0 && (0.0..=1.0).contains(&ndc.z));
        for m in &set.view_proj {
            assert!(m.to_cols_array().iter().all(|v| v.is_finite()));
        }
    }

    #[test]
    fn cascade_fit_is_stable_under_small_camera_moves() {
        let cam_a = Camera::perspective(60.0).look_at(vec3(0.0, 10.0, 15.0), Vec3::ZERO);
        let mut cam_b = cam_a.clone();
        cam_b.eye += vec3(0.001, 0.0, 0.0);
        cam_b.target += vec3(0.001, 0.0, 0.0);
        let a = fit_cascades(&cam_a, vec3(-0.4, -1.0, -0.25));
        let b = fit_cascades(&cam_b, vec3(-0.4, -1.0, -0.25));
        // Texel snapping keeps the projected origin on the same texel grid.
        let half = SHADOW_MAP_SIZE as f32 * 0.5;
        let oa = a.view_proj[0] * Vec4::new(0.0, 0.0, 0.0, 1.0) * half;
        let ob = b.view_proj[0] * Vec4::new(0.0, 0.0, 0.0, 1.0) * half;
        assert!((oa.x - ob.x).abs() < 1e-2 && (oa.y - ob.y).abs() < 1e-2, "{oa:?} vs {ob:?}");
    }
}
