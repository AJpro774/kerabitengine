//! Public mesh builders — no wgpu surface.

use std::path::Path;

use kerabit_assets::AssetError;
use kerabit_render::Mesh as RenderMesh;

/// Indexed triangle mesh (CPU). Built-ins plus asset loaders on the public surface.
#[derive(Clone, Debug)]
pub struct Mesh {
    pub(crate) inner: RenderMesh,
}

impl Mesh {
    /// Unit cube centered at the origin (edge length 1).
    #[inline]
    pub fn cube() -> Self {
        Self {
            inner: RenderMesh::cube(),
        }
    }

    /// Axis-aligned XZ plane of edge length `size`, normal +Y.
    #[inline]
    pub fn plane(size: f32) -> Self {
        Self {
            inner: RenderMesh::plane(size),
        }
    }

    /// Load the first mesh from an OBJ file (positions, normals, UVs).
    pub fn load_obj(path: impl AsRef<Path>) -> Result<Self, AssetError> {
        Ok(Self {
            inner: kerabit_assets::load_obj(path)?,
        })
    }

    /// Wrap a render-crate mesh (used by asset helpers).
    #[inline]
    pub(crate) fn from_render(inner: RenderMesh) -> Self {
        Self { inner }
    }

    #[inline]
    pub(crate) fn as_render(&self) -> &RenderMesh {
        &self.inner
    }
}
