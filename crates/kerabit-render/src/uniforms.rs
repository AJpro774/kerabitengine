//! GPU uniform / instance layouts matching `shaders/frame.wgsl` and the
//! instanced vertex layout shared by `prepass.wgsl`, `lit.wgsl`, `shadow.wgsl`.

use kerabit_color::Color;
use kerabit_math::{Mat4, Vec3};

use crate::camera::Camera;
use crate::light::{Light, LightKind};
use crate::shadow::{CascadeSet, CASCADE_COUNT, SHADOW_BIAS, SHADOW_MAP_SIZE};

/// Light-cluster grid (froxels): tiles across the screen × depth slices.
pub const CLUSTER_X: u32 = 16;
pub const CLUSTER_Y: u32 = 9;
pub const CLUSTER_Z: u32 = 24;
pub const CLUSTER_COUNT: u32 = CLUSTER_X * CLUSTER_Y * CLUSTER_Z;
/// Point lights a single cluster may reference (extra lights are dropped for that cluster).
pub const MAX_LIGHTS_PER_CLUSTER: u32 = 64;

/// One GPU light slot (directional or point).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuLight {
    /// xyz = direction (dir) or position (point); w = kind (`0` dir, `1` point).
    pub pos_or_dir: [f32; 4],
    /// xyz = color * intensity; w = range (point) or `0` (dir).
    pub color_range: [f32; 4],
}

impl GpuLight {
    pub fn from_light(light: &Light) -> Self {
        match light.kind {
            LightKind::Directional => Self {
                pos_or_dir: [
                    light.direction.x,
                    light.direction.y,
                    light.direction.z,
                    0.0,
                ],
                color_range: [
                    light.color.r * light.intensity,
                    light.color.g * light.intensity,
                    light.color.b * light.intensity,
                    0.0,
                ],
            },
            LightKind::Point => Self {
                pos_or_dir: [
                    light.position.x,
                    light.position.y,
                    light.position.z,
                    1.0,
                ],
                color_range: [
                    light.color.r * light.intensity,
                    light.color.g * light.intensity,
                    light.color.b * light.intensity,
                    light.range.max(0.1),
                ],
            },
        }
    }

    pub fn empty() -> Self {
        Self {
            pos_or_dir: [0.0, -1.0, 0.0, 0.0],
            color_range: [0.0, 0.0, 0.0, 0.0],
        }
    }
}

/// Feature toggles written to `FrameUniforms::flags` (1.0 = on).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderSettings {
    pub ssao: bool,
    pub ibl: bool,
    pub ssr: bool,
    pub taa: bool,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            ssao: true,
            ibl: true,
            ssr: true,
            taa: true,
        }
    }
}

/// Everything the frame uniform block needs that is not derived from the camera.
pub struct FrameInputs<'a> {
    pub camera: &'a Camera,
    /// Projection with the TAA sub-pixel jitter applied (equals `camera.projection_matrix()` when TAA is off).
    pub proj_jittered: Mat4,
    /// Unjittered `proj * view` of the previous frame (for reprojection).
    pub prev_view_proj: Mat4,
    /// Current jitter in NDC units (`2 * jitter_px / size`).
    pub jitter_ndc: [f32; 2],
    pub ambient: Color,
    pub env_intensity: f32,
    pub dir_light_count: u32,
    pub point_light_count: u32,
    pub width: u32,
    pub height: u32,
    pub cascades: &'a CascadeSet,
    /// 9 SH coefficients (RGB) of the environment irradiance.
    pub sh: &'a [[f32; 4]; 9],
    pub settings: RenderSettings,
}

/// Frame uniforms (`group(0) binding(0)` in every scene shader).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FrameUniforms {
    pub view_proj: [[f32; 4]; 4],
    pub view: [[f32; 4]; 4],
    pub proj: [[f32; 4]; 4],
    pub inv_view_proj: [[f32; 4]; 4],
    pub inv_proj: [[f32; 4]; 4],
    pub prev_view_proj: [[f32; 4]; 4],
    pub camera_pos: [f32; 4],
    /// rgb = ambient color, w = environment (IBL) intensity.
    pub ambient: [f32; 4],
    /// x near, y far, z directional light count, w point light count.
    pub params: [f32; 4],
    /// x width, y height, z 1/width, w 1/height.
    pub screen: [f32; 4],
    /// x tiles X, y tiles Y, z slices, w = slices / ln(far / near).
    pub cluster: [f32; 4],
    /// xy = current jitter (NDC), zw unused.
    pub jitter: [f32; 4],
    pub cascade_vp: [[[f32; 4]; 4]; CASCADE_COUNT],
    pub cascade_splits: [f32; 4],
    /// x depth bias, y 1 / shadow map size, z cascade count, w blend band (view units).
    pub shadow_params: [f32; 4],
    pub sh: [[f32; 4]; 9],
    /// x ssao, y ibl, z ssr, w taa (1.0 = enabled).
    pub flags: [f32; 4],
}

impl FrameUniforms {
    pub fn build(input: &FrameInputs<'_>) -> Self {
        let cam = input.camera;
        let view = cam.view_matrix();
        let proj = input.proj_jittered;
        let view_proj = proj * view;
        let pos = cam.position();
        let near = cam.near.max(1e-3);
        let far = cam.far.max(near + 1e-3);
        let mut cascade_vp = [[[0.0; 4]; 4]; CASCADE_COUNT];
        for (i, m) in input.cascades.view_proj.iter().enumerate() {
            cascade_vp[i] = m.to_cols_array_2d();
        }
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        Self {
            view_proj: view_proj.to_cols_array_2d(),
            view: view.to_cols_array_2d(),
            proj: proj.to_cols_array_2d(),
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
            inv_proj: proj.inverse().to_cols_array_2d(),
            prev_view_proj: input.prev_view_proj.to_cols_array_2d(),
            camera_pos: [pos.x, pos.y, pos.z, 1.0],
            ambient: [
                input.ambient.r,
                input.ambient.g,
                input.ambient.b,
                input.env_intensity,
            ],
            params: [
                near,
                far,
                input.dir_light_count as f32,
                input.point_light_count as f32,
            ],
            screen: [
                input.width as f32,
                input.height as f32,
                1.0 / input.width.max(1) as f32,
                1.0 / input.height.max(1) as f32,
            ],
            cluster: [
                CLUSTER_X as f32,
                CLUSTER_Y as f32,
                CLUSTER_Z as f32,
                CLUSTER_Z as f32 / (far / near).ln(),
            ],
            jitter: [input.jitter_ndc[0], input.jitter_ndc[1], 0.0, 0.0],
            cascade_vp,
            cascade_splits: input.cascades.splits,
            shadow_params: [
                SHADOW_BIAS,
                1.0 / SHADOW_MAP_SIZE as f32,
                CASCADE_COUNT as f32,
                input.cascades.blend,
            ],
            sh: *input.sh,
            flags: [
                b(input.settings.ssao),
                b(input.settings.ibl),
                b(input.settings.ssr),
                b(input.settings.taa),
            ],
        }
    }
}

/// Per-instance GPU data (vertex step mode `Instance`).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceRaw {
    pub model: [[f32; 4]; 4],
    /// Previous frame's model matrix (motion vectors for TAA).
    pub prev_model: [[f32; 4]; 4],
    pub albedo: [f32; 4],
    /// `x` = roughness, `y` = metallic; `zw` padding.
    pub params: [f32; 4],
}

impl InstanceRaw {
    pub fn new(model: Mat4, prev_model: Mat4, albedo: Color, roughness: f32, metallic: f32) -> Self {
        Self {
            model: model.to_cols_array_2d(),
            prev_model: prev_model.to_cols_array_2d(),
            albedo: albedo.to_array(),
            params: [roughness, metallic, 0.0, 0.0],
        }
    }

    pub const ATTRIBUTES: [wgpu::VertexAttribute; 10] = wgpu::vertex_attr_array![
        3 => Float32x4,
        4 => Float32x4,
        5 => Float32x4,
        6 => Float32x4,
        7 => Float32x4,
        8 => Float32x4,
        9 => Float32x4,
        10 => Float32x4,
        11 => Float32x4,
        12 => Float32x4,
    ];

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// Legacy per-object uniform layout (kept for docs / tests; draws use [`InstanceRaw`]).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ObjectUniforms {
    pub model: [[f32; 4]; 4],
    pub albedo: [f32; 4],
    pub params: [f32; 4],
}

impl ObjectUniforms {
    pub fn new(model: Mat4, albedo: Color, roughness: f32) -> Self {
        Self {
            model: model.to_cols_array_2d(),
            albedo: albedo.to_array(),
            params: [roughness, 0.0, 0.0, 0.0],
        }
    }
}

/// A coarser mesh used beyond `distance` world units from the camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LodLevel {
    pub mesh: crate::MeshId,
    pub distance: f32,
}

/// One opaque draw: mesh handle + model + material params.
#[derive(Clone, Debug)]
pub struct DrawItem {
    pub mesh: crate::MeshId,
    pub model: Mat4,
    /// Model matrix from the previous frame; `None` means static (same as `model`).
    pub prev_model: Option<Mat4>,
    pub albedo: Color,
    pub roughness: f32,
    pub metallic: f32,
    /// GPU albedo map; `None` uses the 1×1 white default at draw time.
    pub albedo_texture: Option<crate::TextureId>,
    /// GPU normal map; `None` uses flat normal default.
    pub normal_texture: Option<crate::TextureId>,
    /// Optional LOD chain, nearest first. Selected per frame by camera distance.
    pub lods: [Option<LodLevel>; 2],
}

impl DrawItem {
    /// Draw with default mid roughness (`0.5`), dielectric (`metallic = 0`), white textures.
    pub fn new(mesh: crate::MeshId, model: Mat4, albedo: Color) -> Self {
        Self {
            mesh,
            model,
            prev_model: None,
            albedo,
            roughness: 0.5,
            metallic: 0.0,
            albedo_texture: None,
            normal_texture: None,
            lods: [None, None],
        }
    }

    pub fn with_roughness(mut self, roughness: f32) -> Self {
        self.roughness = roughness.clamp(0.0, 1.0);
        self
    }

    pub fn with_metallic(mut self, metallic: f32) -> Self {
        self.metallic = metallic.clamp(0.0, 1.0);
        self
    }

    pub fn with_texture(mut self, texture: crate::TextureId) -> Self {
        self.albedo_texture = Some(texture);
        self
    }

    pub fn with_normal_map(mut self, texture: crate::TextureId) -> Self {
        self.normal_texture = Some(texture);
        self
    }

    /// Previous-frame transform for motion vectors (omit for static objects).
    pub fn with_prev_model(mut self, prev: Mat4) -> Self {
        self.prev_model = Some(prev);
        self
    }

    /// Attach up to two coarser LODs (`distance` ascending).
    pub fn with_lods(mut self, lods: &[LodLevel]) -> Self {
        let mut sorted: Vec<LodLevel> = lods.to_vec();
        sorted.sort_by(|a, b| a.distance.total_cmp(&b.distance));
        self.lods = [sorted.first().copied(), sorted.get(1).copied()];
        self
    }

    pub fn at(mesh: crate::MeshId, translation: Vec3, albedo: Color) -> Self {
        Self::new(mesh, Mat4::from_translation(translation), albedo)
    }

    pub fn to_instance(&self) -> InstanceRaw {
        InstanceRaw::new(
            self.model,
            self.prev_model.unwrap_or(self.model),
            self.albedo,
            self.roughness,
            self.metallic,
        )
    }

    /// Mesh to draw at `distance` from the camera, walking the LOD chain.
    pub fn mesh_for_distance(&self, distance: f32) -> crate::MeshId {
        let mut mesh = self.mesh;
        for lod in self.lods.iter().flatten() {
            if distance >= lod.distance {
                mesh = lod.mesh;
            }
        }
        mesh
    }
}

/// Max instances per frame (M8: headroom for ~10k interactive cubes).
pub const MAX_INSTANCES: usize = 16384;

/// Per-batch range in the packed instance buffer: mesh, albedo, normal, start, count.
pub type DrawBatchRange = (crate::MeshId, crate::TextureId, crate::TextureId, u32, u32);

/// Frustum-cull draws and resolve their LOD for this camera.
///
/// Uses local mesh AABBs from `local_aabb`, transformed by each draw's model
/// matrix. Unknown meshes fall back to a unit cube (see [`MeshCache::local_aabb`]).
/// Returned items have `mesh` set to the selected LOD and an empty chain.
pub fn prepare_draws(
    view_proj: Mat4,
    camera_pos: Vec3,
    draws: &[DrawItem],
    mut local_aabb: impl FnMut(crate::MeshId) -> crate::Aabb,
) -> Vec<DrawItem> {
    let mut out = Vec::with_capacity(draws.len().min(MAX_INSTANCES));
    for item in draws {
        if out.len() >= MAX_INSTANCES {
            break;
        }
        let world = local_aabb(item.mesh).transformed(item.model);
        if !crate::aabb_in_frustum(view_proj, world) {
            continue;
        }
        let mut selected = item.clone();
        if item.lods.iter().any(Option::is_some) {
            let center = (world.min + world.max) * 0.5;
            let distance = center.distance(camera_pos);
            selected.mesh = item.mesh_for_distance(distance);
            selected.lods = [None, None];
        }
        out.push(selected);
    }
    out
}

/// Pack draws into a flat instance buffer + per-mesh ranges (shared by prepass, lit, shadow).
///
/// Batch key: mesh + albedo tex + normal tex.
pub fn pack_draw_batches(
    draws: &[DrawItem],
    white: crate::TextureId,
    flat_normal: crate::TextureId,
) -> (Vec<InstanceRaw>, Vec<DrawBatchRange>) {
    let mut batches: Vec<(
        crate::MeshId,
        crate::TextureId,
        crate::TextureId,
        Vec<InstanceRaw>,
    )> = Vec::new();
    for item in draws.iter().take(MAX_INSTANCES) {
        let albedo = item.albedo_texture.unwrap_or(white);
        let normal = item.normal_texture.unwrap_or(flat_normal);
        let raw = item.to_instance();
        if let Some((_, _, _, instances)) = batches
            .iter_mut()
            .find(|(id, a, n, _)| *id == item.mesh && *a == albedo && *n == normal)
        {
            instances.push(raw);
        } else {
            batches.push((item.mesh, albedo, normal, vec![raw]));
        }
    }

    let mut flat: Vec<InstanceRaw> = Vec::with_capacity(draws.len().min(MAX_INSTANCES));
    let mut ranges: Vec<DrawBatchRange> = Vec::with_capacity(batches.len());
    for (mesh, albedo, normal, instances) in batches {
        let start = flat.len() as u32;
        let count = instances.len() as u32;
        flat.extend(instances);
        ranges.push((mesh, albedo, normal, start, count));
    }
    (flat, ranges)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh_gpu::MeshId;
    use crate::Aabb;

    #[test]
    fn frame_uniforms_are_vec4_aligned() {
        assert_eq!(std::mem::size_of::<FrameUniforms>() % 16, 0);
        assert_eq!(std::mem::size_of::<InstanceRaw>(), 160);
    }

    #[test]
    fn lod_chain_selects_by_distance() {
        let base = MeshId::from_raw(1);
        let lod1 = MeshId::from_raw(2);
        let lod2 = MeshId::from_raw(3);
        let item = DrawItem::new(base, Mat4::IDENTITY, Color::WHITE).with_lods(&[
            LodLevel { mesh: lod2, distance: 50.0 },
            LodLevel { mesh: lod1, distance: 20.0 },
        ]);
        assert_eq!(item.mesh_for_distance(5.0), base);
        assert_eq!(item.mesh_for_distance(20.0), lod1);
        assert_eq!(item.mesh_for_distance(49.0), lod1);
        assert_eq!(item.mesh_for_distance(80.0), lod2);
    }

    #[test]
    fn prepare_draws_resolves_lod_and_culls() {
        let base = MeshId::from_raw(1);
        let lod1 = MeshId::from_raw(2);
        let cam = Camera::perspective(60.0).look_at(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO);
        let near_item = DrawItem::new(base, Mat4::IDENTITY, Color::WHITE)
            .with_lods(&[LodLevel { mesh: lod1, distance: 30.0 }]);
        let far_item = DrawItem::new(base, Mat4::from_translation(Vec3::new(0.0, 0.0, -40.0)), Color::WHITE)
            .with_lods(&[LodLevel { mesh: lod1, distance: 30.0 }]);
        let behind = DrawItem::new(base, Mat4::from_translation(Vec3::new(0.0, 0.0, 50.0)), Color::WHITE);
        let unit = |_: MeshId| Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(0.5));
        let out = prepare_draws(
            cam.view_proj(),
            cam.position(),
            &[near_item, far_item, behind],
            unit,
        );
        assert_eq!(out.len(), 2, "object behind the camera is culled");
        assert_eq!(out[0].mesh, base);
        assert_eq!(out[1].mesh, lod1);
        assert!(out[1].lods.iter().all(Option::is_none));
    }
}
