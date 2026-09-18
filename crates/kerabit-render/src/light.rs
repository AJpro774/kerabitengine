//! Scene lights for the clustered lit pass.
//!
//! **Limits (3.0):** up to [`MAX_LIGHTS`] (256) per frame in a storage buffer;
//! at most [`MAX_DIRECTIONAL_LIGHTS`] (4) of them directional. Point lights are
//! binned into a froxel grid on the GPU so each fragment only shades the lights
//! that overlap its cluster. Cascaded soft shadows come from the **first
//! directional** light only. Scene JSON still authors a single sun; multi-light
//! is a runtime / code API.

use kerabit_color::Color;
use kerabit_math::Vec3;

/// Maximum lights uploaded per frame (directional + point).
pub const MAX_LIGHTS: usize = 256;
/// Directional lights shaded per fragment (the rest are dropped).
pub const MAX_DIRECTIONAL_LIGHTS: usize = 4;

/// Directional (sun) or local point light.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightKind {
    /// Parallel rays; `direction` is travel direction (sun → scene).
    Directional,
    /// Omnidirectional; attenuates by distance vs `range`.
    Point,
}

/// A single light contributing to the lit pass.
#[derive(Clone, Debug)]
pub struct Light {
    pub kind: LightKind,
    /// Travel direction for [`LightKind::Directional`] (normalized on set).
    pub direction: Vec3,
    /// World position for [`LightKind::Point`].
    pub position: Vec3,
    pub color: Color,
    pub intensity: f32,
    /// Point-light falloff distance (world units). Unused for directional.
    pub range: f32,
}

impl Light {
    /// Directional sun. `direction` is where the light travels (typically downward).
    pub fn sun(direction: Vec3) -> Self {
        Self {
            kind: LightKind::Directional,
            direction: direction.normalize_or_zero(),
            position: Vec3::ZERO,
            color: Color::WHITE,
            intensity: 1.0,
            range: 0.0,
        }
    }

    /// Alias for [`Self::sun`].
    pub fn directional(direction: Vec3) -> Self {
        Self::sun(direction)
    }

    /// Point light at `position` with a default range of `10`.
    pub fn point(position: Vec3) -> Self {
        Self {
            kind: LightKind::Point,
            direction: Vec3::ZERO,
            position,
            color: Color::WHITE,
            intensity: 1.0,
            range: 10.0,
        }
    }

    /// Scale light color by intensity (written into frame `color_range`).
    pub fn intensity(mut self, intensity: f32) -> Self {
        self.intensity = intensity;
        self
    }

    /// Tint (default white).
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Point-light attenuation range (clamped ≥ 0.1). No-op for directional.
    pub fn range(mut self, range: f32) -> Self {
        self.range = range.max(0.1);
        self
    }

    /// `color * intensity` as xyz; w unused.
    pub fn color_intensity_array(&self) -> [f32; 4] {
        [
            self.color.r * self.intensity,
            self.color.g * self.intensity,
            self.color.b * self.intensity,
            1.0,
        ]
    }

    /// Direction as `[x, y, z, 0]` (directional only; legacy helper).
    pub fn direction_array(&self) -> [f32; 4] {
        [self.direction.x, self.direction.y, self.direction.z, 0.0]
    }

    /// First directional light in `lights`, if any (used for the shadow cascade).
    pub fn first_directional(lights: &[Light]) -> Option<&Light> {
        lights.iter().find(|l| l.kind == LightKind::Directional)
    }
}

/// Keep at most [`MAX_LIGHTS`] lights, and at most [`MAX_DIRECTIONAL_LIGHTS`]
/// directional ones, preserving order (truncates extras).
pub fn clamp_lights(lights: &[Light]) -> Vec<Light> {
    let mut dirs = 0usize;
    lights
        .iter()
        .filter(|l| {
            if l.kind == LightKind::Directional {
                dirs += 1;
                dirs <= MAX_DIRECTIONAL_LIGHTS
            } else {
                true
            }
        })
        .take(MAX_LIGHTS)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kerabit_math::vec3;

    #[test]
    fn sun_intensity_scales_color() {
        let light = Light::sun(vec3(-0.35, -1.0, -0.25)).intensity(1.2);
        let c = light.color_intensity_array();
        assert!((c[0] - 1.2).abs() < 1e-5);
        assert!((c[1] - 1.2).abs() < 1e-5);
    }

    #[test]
    fn point_has_range() {
        let light = Light::point(vec3(1.0, 2.0, 3.0)).range(5.0).intensity(2.0);
        assert_eq!(light.kind, LightKind::Point);
        assert!((light.range - 5.0).abs() < 1e-5);
        assert!((light.intensity - 2.0).abs() < 1e-5);
    }

    #[test]
    fn clamp_truncates() {
        let lights: Vec<_> = (0..300)
            .map(|i| Light::point(vec3(i as f32, 0.0, 0.0)))
            .collect();
        assert_eq!(clamp_lights(&lights).len(), MAX_LIGHTS);
        let suns: Vec<_> = (0..6).map(|_| Light::sun(vec3(0.0, -1.0, 0.0))).collect();
        assert_eq!(clamp_lights(&suns).len(), MAX_DIRECTIONAL_LIGHTS);
    }
}
