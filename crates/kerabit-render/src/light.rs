//! Directional sun light for the lit frame uniforms.
//!
//! Authoring model (E5): one sun + ambient. Soft shadows come from the
//! renderer automatically — there is no multi-light Scene array.

use kerabit_color::Color;
use kerabit_math::Vec3;

/// Directional light (sun). Direction is the travel direction (sun → scene).
#[derive(Clone, Debug)]
pub struct Light {
    pub direction: Vec3,
    pub color: Color,
    pub intensity: f32,
}

impl Light {
    /// Directional sun. `direction` is where the light travels (typically downward).
    pub fn sun(direction: Vec3) -> Self {
        Self {
            direction: direction.normalize_or_zero(),
            color: Color::WHITE,
            intensity: 1.0,
        }
    }

    /// Scale light color by intensity (written into frame `light_color`).
    pub fn intensity(mut self, intensity: f32) -> Self {
        self.intensity = intensity;
        self
    }

    /// Tint the sun (default white).
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// `light_color` xyz for frame uniforms = color * intensity.
    pub fn color_intensity_array(&self) -> [f32; 4] {
        [
            self.color.r * self.intensity,
            self.color.g * self.intensity,
            self.color.b * self.intensity,
            1.0,
        ]
    }

    /// Direction as `[x, y, z, 0]`.
    pub fn direction_array(&self) -> [f32; 4] {
        [self.direction.x, self.direction.y, self.direction.z, 0.0]
    }
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
}
