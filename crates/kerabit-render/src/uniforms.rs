//! GPU uniform / instance layouts matching `shaders/lit.wgsl`.

use kerabit_color::Color;
use kerabit_math::{Mat4, Vec3};

use crate::camera::Camera;
use crate::light::{Light, LightKind, MAX_LIGHTS};
use crate::shadow::{SHADOW_BIAS, SHADOW_MAP_SIZE};

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

/// Frame uniforms: view-proj, camera, ambient, shadow matrix, up to 4 lights.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FrameUniforms {
    pub view_proj: [[f32; 4]; 4],
    pub camera_pos: [f32; 4],
    pub ambient: [f32; 4],
    pub light_view_proj: [[f32; 4]; 4],
    /// `x` = depth bias, `y` = 1 / shadow map size, `z` = light count, `w` unused.
    pub shadow_params: [f32; 4],
    pub lights: [GpuLight; MAX_LIGHTS],
}

impl FrameUniforms {
    /// Build from a light list (max [`MAX_LIGHTS`]). Shadow cascade follows the
    /// first directional light, else identity / unused.
    pub fn from_lights(
        camera: &Camera,
        lights: &[Light],
        ambient: Color,
        light_view_proj: Mat4,
    ) -> Self {
        let pos = camera.position();
        let mut gpu_lights = [GpuLight::empty(); MAX_LIGHTS];
        let count = lights.len().min(MAX_LIGHTS);
        for (i, light) in lights.iter().take(count).enumerate() {
            gpu_lights[i] = GpuLight::from_light(light);
        }
        Self {
            view_proj: camera.view_proj().to_cols_array_2d(),
            camera_pos: [pos.x, pos.y, pos.z, 1.0],
            ambient: ambient.to_array(),
            light_view_proj: light_view_proj.to_cols_array_2d(),
            shadow_params: [
                SHADOW_BIAS,
                1.0 / SHADOW_MAP_SIZE as f32,
                count as f32,
                0.0,
            ],
            lights: gpu_lights,
        }
    }

    /// Convenience: single sun (legacy E5 path).
    pub fn from_scene(
        camera: &Camera,
        light: &Light,
        ambient: Color,
        light_view_proj: Mat4,
    ) -> Self {
        Self::from_lights(camera, std::slice::from_ref(light), ambient, light_view_proj)
    }
}

/// Per-instance GPU data (vertex step mode `Instance`).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceRaw {
    pub model: [[f32; 4]; 4],
    pub albedo: [f32; 4],
    /// `x` = roughness, `y` = metallic; `zw` padding.
    pub params: [f32; 4],
}

impl InstanceRaw {
    pub fn new(model: Mat4, albedo: Color, roughness: f32, metallic: f32) -> Self {
        Self {
            model: model.to_cols_array_2d(),
            albedo: albedo.to_array(),
            params: [roughness, metallic, 0.0, 0.0],
        }
    }

    pub const ATTRIBUTES: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
        3 => Float32x4,
        4 => Float32x4,
        5 => Float32x4,
        6 => Float32x4,
        7 => Float32x4,
        8 => Float32x4,
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

/// One opaque draw: mesh handle + model + material params.
#[derive(Clone, Debug)]
pub struct DrawItem {
    pub mesh: crate::MeshId,
    pub model: Mat4,
    pub albedo: Color,
    pub roughness: f32,
    pub metallic: f32,
    /// GPU albedo map; `None` uses the 1×1 white default at draw time.
    pub albedo_texture: Option<crate::TextureId>,
    /// GPU normal map; `None` uses flat normal default.
    pub normal_texture: Option<crate::TextureId>,
}

impl DrawItem {
    /// Draw with default mid roughness (`0.5`), dielectric (`metallic = 0`), white textures.
    pub fn new(mesh: crate::MeshId, model: Mat4, albedo: Color) -> Self {
        Self {
            mesh,
            model,
            albedo,
            roughness: 0.5,
            metallic: 0.0,
            albedo_texture: None,
            normal_texture: None,
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

    pub fn at(mesh: crate::MeshId, translation: Vec3, albedo: Color) -> Self {
        Self::new(mesh, Mat4::from_translation(translation), albedo)
    }

    pub fn to_instance(&self) -> InstanceRaw {
        InstanceRaw::new(self.model, self.albedo, self.roughness, self.metallic)
    }
}

/// Max instances per frame (release-friendly path for ~1k cubes).
pub const MAX_INSTANCES: usize = 2048;

/// Pack draws into a flat instance buffer + per-mesh ranges (shared by lit + shadow).
///
/// Batch key: mesh + albedo tex + normal tex.
pub fn pack_draw_batches(
    draws: &[DrawItem],
    white: crate::TextureId,
    flat_normal: crate::TextureId,
) -> (
    Vec<InstanceRaw>,
    Vec<(crate::MeshId, crate::TextureId, crate::TextureId, u32, u32)>,
) {
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
    let mut ranges: Vec<(crate::MeshId, crate::TextureId, crate::TextureId, u32, u32)> =
        Vec::with_capacity(batches.len());
    for (mesh, albedo, normal, instances) in batches {
        let start = flat.len() as u32;
        let count = instances.len() as u32;
        flat.extend(instances);
        ranges.push((mesh, albedo, normal, start, count));
    }
    (flat, ranges)
}
