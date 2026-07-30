//! Asset load errors.

use std::path::PathBuf;

/// Failure loading or decoding an asset.
#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    #[error("I/O error reading `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to load OBJ `{path}`: {message}")]
    Obj { path: PathBuf, message: String },
    #[error("failed to decode image `{path}`: {message}")]
    Image { path: PathBuf, message: String },
    #[error("failed to load glTF `{path}`: {message}")]
    Gltf { path: PathBuf, message: String },
    #[error("asset `{path}` has no mesh data")]
    EmptyMesh { path: PathBuf },
    #[error("mesh in `{path}` exceeds u16 index limit ({count} vertices)")]
    TooManyVertices { path: PathBuf, count: usize },
}

impl AssetError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
