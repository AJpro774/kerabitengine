//! Orbit / pan / zoom camera for the editor viewport (distinct from scene camera).

use kerabit::{vec3, Camera, Vec3};

/// Editor view camera controlled by orbit / pan / zoom — not written to the Scene.
#[derive(Clone, Debug)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 12.0,
            yaw: 0.6,
            pitch: 0.55,
            fov_y: 50.0,
            near: 0.05,
            far: 500.0,
        }
    }
}

impl OrbitCamera {
    pub fn eye(&self) -> Vec3 {
        let cp = self.pitch.cos();
        let sp = self.pitch.sin();
        let cy = self.yaw.cos();
        let sy = self.yaw.sin();
        let offset = vec3(cp * sy, sp, cp * cy) * self.distance;
        self.target + offset
    }

    pub fn to_camera(&self) -> Camera {
        Camera::perspective(self.fov_y)
            .look_at(self.eye(), self.target)
            .near_far(self.near, self.far)
    }

    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw += dx * 0.01;
        self.pitch = (self.pitch + dy * 0.01).clamp(-1.4, 1.4);
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        let eye = self.eye();
        let forward = (self.target - eye).normalize_or_zero();
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(forward).normalize_or_zero();
        let scale = self.distance * 0.002;
        self.target += right * (-dx * scale) + up * (dy * scale);
    }

    pub fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance * (1.0 - delta * 0.1)).clamp(0.5, 400.0);
    }
}
