//! Perspective camera: FOV, look-at, aspect from the window.

use kerabit_math::{look_at, Mat4, Vec3};

/// Perspective camera used to fill frame view-proj uniforms.
#[derive(Clone, Debug)]
pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    /// Vertical field of view in degrees.
    pub fov_y_degrees: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    /// Perspective camera with the given vertical FOV (degrees). Defaults look down −Z.
    pub fn perspective(fov_y_degrees: f32) -> Self {
        Self {
            eye: Vec3::new(0.0, 0.0, 5.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_y_degrees,
            aspect: 16.0 / 9.0,
            near: 0.1,
            far: 100.0,
        }
    }

    /// Place the eye and look toward `target` (up remains +Y unless changed).
    pub fn look_at(mut self, eye: Vec3, target: Vec3) -> Self {
        self.eye = eye;
        self.target = target;
        self
    }

    /// Override near/far clip planes.
    pub fn near_far(mut self, near: f32, far: f32) -> Self {
        self.near = near;
        self.far = far;
        self
    }

    /// Update aspect ratio from the window (`width / height`).
    pub fn set_aspect(&mut self, aspect: f32) {
        self.aspect = aspect.max(1e-4);
    }

    /// World-space eye position (also written to frame uniforms as camera pos).
    pub fn position(&self) -> Vec3 {
        self.eye
    }

    /// View matrix (right-handed look-at).
    pub fn view_matrix(&self) -> Mat4 {
        look_at(self.eye, self.target, self.up)
    }

    /// Projection matrix (right-handed perspective).
    pub fn projection_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(self.fov_y_degrees.to_radians(), self.aspect, self.near, self.far)
    }

    /// Combined `projection * view` for clip-space transforms.
    pub fn view_proj(&self) -> Mat4 {
        self.projection_matrix() * self.view_matrix()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kerabit_math::vec3;

    #[test]
    fn perspective_look_at_is_finite() {
        let cam = Camera::perspective(60.0).look_at(vec3(5.0, 3.0, 7.0), Vec3::ZERO);
        let vp = cam.view_proj();
        for v in vp.to_cols_array() {
            assert!(v.is_finite());
        }
        assert_eq!(cam.position(), vec3(5.0, 3.0, 7.0));
    }
}
