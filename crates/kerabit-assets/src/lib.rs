//! Asset loaders for Kerabit: OBJ meshes, PNG albedo, and minimal glTF.
//!
//! Loaders produce [`kerabit_render::Mesh`] and CPU [`Texture`] data compatible
//! with the public `Mesh` / `Material` wrappers in the `kerabit` facade.

mod error;
mod gltf_lite;
mod obj;
mod texture;

pub use error::AssetError;
pub use gltf_lite::{load_gltf, GltfMesh};
pub use obj::load_obj;
pub use texture::Texture;
