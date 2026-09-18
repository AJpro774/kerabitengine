//! GPU mesh upload and cache (`MeshId` → vertex/index buffers).

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use wgpu::util::DeviceExt;

use crate::mesh::Mesh;
use crate::picking::Aabb;

/// Opaque handle to a mesh resident on the GPU.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MeshId(u32);

impl MeshId {
    /// Raw id (for debugging / logging).
    pub fn as_u32(self) -> u32 {
        self.0
    }

    /// Build from a raw id (tests / tooling only; ids come from [`MeshCache::upload`]).
    pub fn from_raw(id: u32) -> Self {
        Self(id)
    }
}

/// GPU buffers for one uploaded mesh.
pub struct GpuMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
    /// Local-space AABB of the CPU mesh (used for frustum culling).
    pub local_aabb: Aabb,
}

/// Uploads CPU [`Mesh`]es once and looks them up by [`MeshId`].
///
/// Identical vertex/index contents reuse the same [`MeshId`] (important for
/// instancing many copies of `Mesh::cube()`).
#[derive(Default)]
pub struct MeshCache {
    next_id: u32,
    meshes: HashMap<MeshId, GpuMesh>,
    by_content: HashMap<u64, MeshId>,
}

impl MeshCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Upload `mesh` to the GPU and return a stable [`MeshId`].
    pub fn upload(&mut self, device: &wgpu::Device, mesh: &Mesh) -> MeshId {
        let key = mesh_content_key(mesh);
        if let Some(id) = self.by_content.get(&key) {
            return *id;
        }

        let id = MeshId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh-vertices"),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh-indices"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        self.meshes.insert(
            id,
            GpuMesh {
                vertex_buffer,
                index_buffer,
                index_count: mesh.index_count(),
                local_aabb: Aabb::from_mesh(mesh),
            },
        );
        self.by_content.insert(key, id);
        id
    }

    pub fn get(&self, id: MeshId) -> Option<&GpuMesh> {
        self.meshes.get(&id)
    }

    /// Local AABB for an uploaded mesh (unit cube if unknown).
    pub fn local_aabb(&self, id: MeshId) -> Aabb {
        self.meshes
            .get(&id)
            .map(|m| m.local_aabb)
            .unwrap_or_else(|| Aabb::from_center_half_extents(kerabit_math::Vec3::ZERO, kerabit_math::Vec3::splat(0.5)))
    }
}

fn mesh_content_key(mesh: &Mesh) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytemuck::cast_slice::<_, u8>(&mesh.vertices).hash(&mut hasher);
    mesh.indices.hash(&mut hasher);
    hasher.finish()
}
