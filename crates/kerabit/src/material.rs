//! Material with albedo, optional roughness (P4), and optional albedo texture (P5).

use std::path::Path;

use kerabit_assets::{AssetError, Texture};
use kerabit_color::Color;

/// Surface appearance for a spawned entity.
///
/// `roughness` is a simple lit-shader control (0 = shiny specular, 1 = matte).
/// Albedo-only materials from P3 stay valid via [`Material::color`].
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    albedo: Color,
    roughness: f32,
    albedo_texture: Option<Texture>,
}

impl Material {
    /// Albedo tint with default mid roughness (`0.5`). Still shaded by the lit pipeline.
    #[inline]
    pub fn color(albedo: Color) -> Self {
        Self {
            albedo,
            roughness: 0.5,
            albedo_texture: None,
        }
    }

    /// Override roughness in `0.0..=1.0` (clamped). Chain after [`Self::color`].
    #[inline]
    pub fn roughness(mut self, roughness: f32) -> Self {
        self.roughness = roughness.clamp(0.0, 1.0);
        self
    }

    /// Attach a CPU albedo texture (sampled in the lit shader via UVs).
    #[inline]
    pub fn with_texture(mut self, texture: Texture) -> Self {
        self.albedo_texture = Some(texture);
        self
    }

    /// Load a PNG as the albedo map (multiplied by [`Self::albedo`], default white).
    pub fn load_png(path: impl AsRef<Path>) -> Result<Self, AssetError> {
        let texture = Texture::load_png(path)?;
        Ok(Self::color(Color::WHITE).with_texture(texture))
    }

    #[inline]
    pub fn albedo(&self) -> Color {
        self.albedo
    }

    #[inline]
    pub fn roughness_factor(&self) -> f32 {
        self.roughness
    }

    #[inline]
    pub fn albedo_texture(&self) -> Option<&Texture> {
        self.albedo_texture.as_ref()
    }
}

impl Default for Material {
    fn default() -> Self {
        Self::color(Color::WHITE)
    }
}
