//! Radiance `.hdr` (RGBE) equirectangular environment maps for image-based lighting.

use std::path::Path;

use image::ImageReader;

use crate::error::AssetError;

/// Linear RGB float pixels of an equirectangular panorama (row-major, top row first).
#[derive(Clone, Debug, PartialEq)]
pub struct HdrImage {
    pub width: u32,
    pub height: u32,
    /// `width * height * 3` linear floats.
    pub rgb: Vec<f32>,
}

impl HdrImage {
    pub fn from_rgb32f(width: u32, height: u32, rgb: Vec<f32>) -> Self {
        debug_assert_eq!(rgb.len(), (width * height * 3) as usize);
        Self { width, height, rgb }
    }

    /// Decode a Radiance `.hdr` file (any `image`-supported float format works).
    pub fn load(path: impl AsRef<Path>) -> Result<Self, AssetError> {
        let path = path.as_ref();
        let reader = ImageReader::open(path)
            .map_err(|e| AssetError::io(path, e))?
            .with_guessed_format()
            .map_err(|e| AssetError::io(path, e))?;
        let img = reader.decode().map_err(|e| AssetError::Image {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        let rgb = img.to_rgb32f();
        let (width, height) = rgb.dimensions();
        if width == 0 || height == 0 {
            return Err(AssetError::Image {
                path: path.to_path_buf(),
                message: "empty image".into(),
            });
        }
        Ok(Self::from_rgb32f(width, height, rgb.into_raw()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_tiny_radiance_file() {
        let dir = std::env::temp_dir().join(format!("kerabit-hdr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sky.hdr");
        let (w, h) = (4u32, 2u32);
        let mut pixels = Vec::new();
        for y in 0..h {
            for x in 0..w {
                pixels.push(image::Rgb([x as f32 * 0.5, y as f32 * 2.0 + 0.25, 8.0]));
            }
        }
        let file = std::fs::File::create(&path).unwrap();
        image::codecs::hdr::HdrEncoder::new(std::io::BufWriter::new(file))
            .encode(&pixels, w as usize, h as usize)
            .unwrap();

        let img = HdrImage::load(&path).expect("decode hdr");
        assert_eq!((img.width, img.height), (w, h));
        assert_eq!(img.rgb.len(), (w * h * 3) as usize);
        // RGBE has ~1% precision; check the brightest channel and the origin.
        assert!((img.rgb[2] - 8.0).abs() < 0.1, "{}", img.rgb[2]);
        assert!(img.rgb[0].abs() < 0.05);
        let last = ((h - 1) * w + (w - 1)) as usize * 3;
        assert!((img.rgb[last + 1] - 2.25).abs() < 0.05, "{}", img.rgb[last + 1]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
