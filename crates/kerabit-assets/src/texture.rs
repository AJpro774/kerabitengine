//! CPU RGBA8 texture (albedo).

use std::path::Path;

use image::ImageReader;

use crate::error::AssetError;

/// CPU albedo texture: tightly packed RGBA8 (`width * height * 4` bytes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Texture {
    /// Create from raw RGBA8 pixels. Panics if `rgba.len() != width * height * 4`.
    pub fn from_rgba8(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        let expected = (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(4);
        assert_eq!(
            rgba.len(),
            expected,
            "Texture::from_rgba8: expected {expected} bytes, got {}",
            rgba.len()
        );
        Self {
            width,
            height,
            rgba,
        }
    }

    /// 1×1 opaque white (default when no albedo map is bound).
    pub fn white_1x1() -> Self {
        Self::from_rgba8(1, 1, vec![255, 255, 255, 255])
    }

    /// Load a PNG (or other `image`-enabled format) as RGBA8.
    pub fn load_png(path: impl AsRef<Path>) -> Result<Self, AssetError> {
        let path = path.as_ref();
        let reader = ImageReader::open(path)
            .map_err(|e| AssetError::io(path, e))?
            .with_guessed_format()
            .map_err(|e| AssetError::io(path, e))?;
        let img = reader.decode().map_err(|e| AssetError::Image {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        Ok(Self::from_rgba8(width, height, rgba.into_raw()))
    }

    /// Byte length of pixel storage.
    #[inline]
    pub fn byte_len(&self) -> usize {
        self.rgba.len()
    }
}
