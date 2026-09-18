//! GPU renderer (wgpu) for Kerabit.
//!
//! 3.0 render tier: depth prepass + G-buffer, clustered lights (256), four
//! cascaded soft shadows, SSAO, HDR image-based lighting, screen-space
//! reflections, TAA, mesh LODs, bloom + ACES tonemap, particle billboards,
//! sky gradient. Do not leak `wgpu` / `winit` types through the `kerabit` facade.

mod app;
mod camera;
mod environment;
mod font8x8;
mod gbuffer;
mod gpu;
mod light;
mod lights;
mod mesh;
mod mesh_gpu;
mod offscreen;
mod overlay;
mod particles;
mod picking;
mod post;
mod scene_renderer;
mod shadow;
mod sky;
mod ssao;
mod ssr;
mod taa;
mod texture;
mod uniforms;
mod vertex;

pub use app::{run_hardcoded_cube, run_two_meshes};
pub use camera::Camera;
pub use environment::{EquirectImage, SPECULAR_MIPS, SPECULAR_SIZE};
pub use gpu::{GpuState, SurfaceError};
pub use light::{clamp_lights, Light, LightKind, MAX_DIRECTIONAL_LIGHTS, MAX_LIGHTS};
pub use mesh::{Mesh, MeshBuilder};
pub use mesh_gpu::{MeshCache, MeshId};
pub use offscreen::OffscreenLitRenderer;
pub use overlay::{OverlayCommands, OverlayQuad};
pub use particles::{ParticleBurst, ParticleSystem, MAX_PARTICLES};
pub use picking::{
    aabb_in_frustum, pick_closest, pointer_to_ndc, ray_aabb, ray_from_ndc, ray_plane_y, Aabb, Ray,
};
pub use post::{PostStack, HDR_FORMAT};
pub use scene_renderer::{SceneRenderer, DEFAULT_SKY_ENV_INTENSITY};
pub use shadow::{
    directional_light_matrix, fit_cascades, CascadeSet, ShadowMap, CASCADE_COUNT, SHADOW_DISTANCE,
    SHADOW_HALF_EXTENT, SHADOW_MAP_SIZE,
};
pub use sky::zenith_from_horizon;
pub use texture::{TextureCache, TextureId};
pub use uniforms::{
    DrawItem, FrameUniforms, GpuLight, InstanceRaw, LodLevel, ObjectUniforms, RenderSettings,
    CLUSTER_X, CLUSTER_Y, CLUSTER_Z, MAX_LIGHTS_PER_CLUSTER,
};
pub use vertex::Vertex;
