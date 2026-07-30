//! RGBA color type and named constants for Kerabit.

/// Linear-ish RGBA color with components in `0.0..=1.0` (not clamped on create).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    /// Opaque orange accent (public API demo material).
    pub const ORANGE: Color = Color::rgb(1.0, 0.55, 0.1);

    /// Mid gray (typical ground / neutral).
    pub const GRAY: Color = Color::rgb(0.45, 0.45, 0.48);

    /// Opaque white.
    pub const WHITE: Color = Color::rgb(1.0, 1.0, 1.0);

    /// Opaque black.
    pub const BLACK: Color = Color::rgb(0.0, 0.0, 0.0);

    /// Create an opaque RGB color.
    #[inline]
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Create an RGBA color.
    #[inline]
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Components as `[r, g, b, a]`.
    #[inline]
    pub const fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// RGB only as `[r, g, b]`.
    #[inline]
    pub const fn to_rgb_array(self) -> [f32; 3] {
        [self.r, self.g, self.b]
    }

    /// Linear interpolate toward `other` by `t` (clamped to `0..=1`).
    #[inline]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }
}

impl Default for Color {
    fn default() -> Self {
        Color::WHITE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_is_opaque() {
        let c = Color::rgb(0.1, 0.2, 0.3);
        assert_eq!(c.a, 1.0);
        assert_eq!(c.to_array(), [0.1, 0.2, 0.3, 1.0]);
    }

    #[test]
    fn rgba_preserves_alpha() {
        let c = Color::rgba(0.0, 0.0, 0.0, 0.5);
        assert_eq!(c.a, 0.5);
    }

    #[test]
    fn named_constants() {
        assert_eq!(Color::WHITE.to_rgb_array(), [1.0, 1.0, 1.0]);
        assert_eq!(Color::BLACK.to_rgb_array(), [0.0, 0.0, 0.0]);
        assert!(Color::ORANGE.r > Color::ORANGE.g);
        assert!(Color::GRAY.r > 0.0);
    }

    #[test]
    fn lerp_midpoint() {
        let c = Color::BLACK.lerp(Color::WHITE, 0.5);
        assert!((c.r - 0.5).abs() < 1e-6);
        assert!((c.a - 1.0).abs() < 1e-6);
    }
}
