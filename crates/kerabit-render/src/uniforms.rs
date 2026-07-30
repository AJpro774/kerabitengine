//! GPU uniform / instance layouts matching `shaders/lit.wgsl`.

use kerabit_color::Color;
use kerabit_math::{Mat4, Vec3};

use crate::camera::Camera;
use crate::light::Light;
use crate::shadow::{SHADOW_BIAS, SHADOW_MAP_SIZE};

/// Frame uniforms: view-proj, camera pos, sun, ambient, shadow matrix.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FrameUniforms {
    pub view_proj: [[f32; 4]; 4],
    pub camera_pos: [f32; 4],
    pub light_dir: [f32; 4],
    pub light_color: [f32; 4],
    pub ambient: [f32; 4],
    pub light_view_proj: [[f32; 4]; 4],
    /// `x` = depth bias, `y` = 1 / shadow map size, `zw` unused.
    pub shadow_params: [f32; 4],
}

impl FrameUniforms {
    pub fn from_scene(
        camera: &Camera,
        light: &Light,
        ambient: Color,
        light_view_proj: Mat4,
    ) -> Self {
        let pos = camera.position();
        Self {
            view_proj: camera.view_proj().to_cols_array_2d(),
            camera_pos: [pos.x, pos.y, pos.z, 1.0],
            light_dir: light.direction_array(),
            light_color: light.color_intensity_array(),
            ambient: ambient.to_array(),
            light_view_proj: light_view_proj.to_cols_array_2d(),
            shadow_params: [SHADOW_BIAS, 1.0 / SHADOW_MAP_SIZE as f32, 0.0, 0.0],
        }
    }
}

/// Per-instance GPU data (vertex step mode `Instance`).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceRaw {
    pub model: [[f32; 4]; 4],
    pub albedo: [f32; 4],
    /// `x` = roughness; `yzw` padding for 16-byte alignment.
    pub params: [f32; 4],
}

impl InstanceRaw {
    pub fn new(model: Mat4, albedo: Color, roughness: f32) -> Self {
        Self {
            model: model.to_cols_array_2d(),
            albedo: albedo.to_array(),
            params: [roughness, 0.0, 0.0, 0.0],
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
    /// GPU albedo map; `None` uses the 1×1 white default at draw time.
    pub albedo_texture: Option<crate::TextureId>,
}

impl DrawItem {
    /// Draw with default mid roughness (`0.5`) and white albedo texture.
    pub fn new(mesh: crate::MeshId, model: Mat4, albedo: Color) -> Self {
        Self {
            mesh,
            model,
            albedo,
            roughness: 0.5,
            albedo_texture: None,
        }
    }

    pub fn with_roughness(mut self, roughness: f32) -> Self {
        self.roughness = roughness.clamp(0.0, 1.0);
        self
    }

    pub fn with_texture(mut self, texture: crate::TextureId) -> Self {
        self.albedo_texture = Some(texture);
        self
    }

    pub fn at(mesh: crate::MeshId, translation: Vec3, albedo: Color) -> Self {
        Self::new(mesh, Mat4::from_translation(translation), albedo)
    }

    pub fn to_instance(&self) -> InstanceRaw {
        InstanceRaw::new(self.model, self.albedo, self.roughness)
    }
}

/// Max instances per frame (release-friendly path for ~1k cubes).
pub const MAX_INSTANCES: usize = 2048;

/// Pack draws into a flat instance buffer + per-mesh ranges (shared by lit + shadow).
pub fn pack_draw_batches(
    draws: &[DrawItem],
    white: crate::TextureId,
) -> (
    Vec<InstanceRaw>,
    Vec<(crate::MeshId, crate::TextureId, u32, u32)>,
) {
    let mut batches: Vec<(crate::MeshId, crate::TextureId, Vec<InstanceRaw>)> = Vec::new();
    for item in draws.iter().take(MAX_INSTANCES) {
        let tex = item.albedo_texture.unwrap_or(white);
        let raw = item.to_instance();
        if let Some((_, _, instances)) = batches
            .iter_mut()
            .find(|(id, t, _)| *id == item.mesh && *t == tex)
        {
            instances.push(raw);
        } else {
            batches.push((item.mesh, tex, vec![raw]));
        }
    }

    let mut flat: Vec<InstanceRaw> = Vec::with_capacity(draws.len().min(MAX_INSTANCES));
    let mut ranges: Vec<(crate::MeshId, crate::TextureId, u32, u32)> =
        Vec::with_capacity(batches.len());
    for (mesh, tex, instances) in batches {
        let start = flat.len() as u32;
        let count = instances.len() as u32;
        flat.extend(instances);
        ranges.push((mesh, tex, start, count));
    }
    (flat, ranges)
}
