//! Math types and helpers for Kerabit.
//!
//! Re-exports [`glam`] f32 types and adds thin helpers used by the public API
//! (`vec3`, degree/radian newtypes, look-at matrix).

pub use glam::{Mat3, Mat4, Quat, Vec2, Vec3, Vec4};

/// Construct a [`Vec3`] from components.
#[inline]
pub const fn vec3(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

/// Construct a [`Vec2`] from components.
#[inline]
pub const fn vec2(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}

/// Angle in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Deg(pub f32);

/// Angle in radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rad(pub f32);

impl Deg {
    /// Convert degrees to radians.
    #[inline]
    pub fn to_rad(self) -> Rad {
        Rad(self.0.to_radians())
    }

    /// Degrees as a raw `f32`.
    #[inline]
    pub fn as_f32(self) -> f32 {
        self.0
    }
}

impl Rad {
    /// Convert radians to degrees.
    #[inline]
    pub fn to_deg(self) -> Deg {
        Deg(self.0.to_degrees())
    }

    /// Radians as a raw `f32`.
    #[inline]
    pub fn as_f32(self) -> f32 {
        self.0
    }
}

impl From<Deg> for Rad {
    #[inline]
    fn from(d: Deg) -> Self {
        d.to_rad()
    }
}

impl From<Rad> for Deg {
    #[inline]
    fn from(r: Rad) -> Self {
        r.to_deg()
    }
}

/// Build a right-handed look-at view matrix.
///
/// Eye looks toward `target` with the given `up` vector (typically `Vec3::Y`).
#[inline]
pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
    Mat4::look_at_rh(eye, target, up)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vec3_helper() {
        let v = vec3(1.0, 2.0, 3.0);
        assert_eq!(v, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn deg_rad_roundtrip() {
        let d = Deg(90.0);
        let r = d.to_rad();
        assert!((r.as_f32() - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        let back = r.to_deg();
        assert!((back.as_f32() - 90.0).abs() < 1e-4);
    }

    #[test]
    fn look_at_produces_finite_matrix() {
        let m = look_at(vec3(5.0, 3.0, 7.0), Vec3::ZERO, Vec3::Y);
        for col in m.to_cols_array() {
            assert!(col.is_finite());
        }
    }
}
