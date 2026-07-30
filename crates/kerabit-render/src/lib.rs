//! GPU renderer (wgpu) for Kerabit.
//!
//! Instanced lit draws (mesh batches + instance buffer), roughness-lite
//! specular, directional soft shadows, sky gradient, mesh content-hash cache.
//! Do not leak `wgpu` / `winit` types through the `kerabit` facade.

mod app;
mod camera;
mod font8x8;
mod gpu;
mod light;
mod mesh;
mod mesh_gpu;
mod offscreen;
mod overlay;
mod picking;
mod shadow;
mod sky;
mod texture;
mod uniforms;
mod vertex;

pub use app::{run_hardcoded_cube, run_two_meshes};
pub use camera::Camera;
pub use gpu::{GpuState, SurfaceError};
pub use light::Light;
pub use mesh::{Mesh, MeshBuilder};
pub use mesh_gpu::{MeshCache, MeshId};
pub use offscreen::OffscreenLitRenderer;
pub use overlay::{OverlayCommands, OverlayQuad};
pub use picking::{
    pick_closest, pointer_to_ndc, ray_aabb, ray_from_ndc, ray_plane_y, Aabb, Ray,
};
pub use shadow::{directional_light_matrix, ShadowMap, SHADOW_HALF_EXTENT, SHADOW_MAP_SIZE};
pub use sky::zenith_from_horizon;
pub use texture::{TextureCache, TextureId};
pub use uniforms::{DrawItem, FrameUniforms, InstanceRaw, ObjectUniforms};
pub use vertex::Vertex;
