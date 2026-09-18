//! Asset loaders for Kerabit: OBJ meshes, PNG albedo, minimal glTF, and
//! Radiance `.hdr` environments.
//!
//! Loaders produce [`kerabit_render::Mesh`] and CPU [`Texture`] / [`HdrImage`]
//! data compatible with the public wrappers in the `kerabit` facade.

mod error;
mod gltf_lite;
mod hdr;
mod obj;
mod texture;

pub use error::AssetError;
pub use gltf_lite::{load_gltf, GltfMesh};
pub use hdr::HdrImage;
pub use obj::load_obj;
pub use texture::Texture;
