//! Public asset helpers (OBJ / PNG / glTF lite).

use std::path::Path;

use kerabit_assets::AssetError;

use crate::material::Material;
use crate::mesh::Mesh;

pub use kerabit_assets::Texture;

/// Load the first mesh + base-color material from a glTF / GLB file.
///
/// No animation. Base color factor and optional base color texture are applied.
pub fn load_gltf(path: impl AsRef<Path>) -> Result<(Mesh, Material), AssetError> {
    let loaded = kerabit_assets::load_gltf(path)?;
    let mut material = Material::color(loaded.albedo);
    if let Some(tex) = loaded.albedo_texture {
        material = material.with_texture(tex);
    }
    Ok((Mesh::from_render(loaded.mesh), material))
}
