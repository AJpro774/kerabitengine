//! Ray / AABB picking helpers for editor viewports (and optional game tools).
//!
//! Kept in `kerabit-render` so games stay free of egui while sharing the same
//! mesh-bounds math the viewport uses.

use kerabit_math::{Mat4, Vec3, Vec4};

use crate::camera::Camera;
use crate::mesh::Mesh;

/// Axis-aligned bounding box (world or local).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    #[inline]
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self {
            min: min.min(max),
            max: min.max(max),
        }
    }

    #[inline]
    pub fn from_center_half_extents(center: Vec3, half_extents: Vec3) -> Self {
        let half = half_extents.abs();
        Self {
            min: center - half,
            max: center + half,
        }
    }

    /// Local AABB of mesh vertex positions. Empty mesh → unit cube.
    pub fn from_mesh(mesh: &Mesh) -> Self {
        if mesh.vertices.is_empty() {
            return Self::from_center_half_extents(Vec3::ZERO, Vec3::splat(0.5));
        }
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for v in &mesh.vertices {
            let p = Vec3::from_array(v.position);
            min = min.min(p);
            max = max.max(p);
        }
        Self::new(min, max)
    }

    /// Transform a local AABB by `model` (transforms the eight corners).
    pub fn transformed(self, model: Mat4) -> Self {
        let corners = [
            Vec3::new(self.min.x, self.min.y, self.min.z),
            Vec3::new(self.max.x, self.min.y, self.min.z),
            Vec3::new(self.min.x, self.max.y, self.min.z),
            Vec3::new(self.max.x, self.max.y, self.min.z),
            Vec3::new(self.min.x, self.min.y, self.max.z),
            Vec3::new(self.max.x, self.min.y, self.max.z),
            Vec3::new(self.min.x, self.max.y, self.max.z),
            Vec3::new(self.max.x, self.max.y, self.max.z),
        ];
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for c in corners {
            let w = model.transform_point3(c);
            min = min.min(w);
            max = max.max(w);
        }
        Self::new(min, max)
    }
}

/// Ray in world space (`origin + t * direction`, `direction` not necessarily unit).
#[derive(Clone, Copy, Debug)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
}

impl Ray {
    #[inline]
    pub fn new(origin: Vec3, direction: Vec3) -> Self {
        Self { origin, direction }
    }

    /// Point at parameter `t`.
    #[inline]
    pub fn at(self, t: f32) -> Vec3 {
        self.origin + self.direction * t
    }
}

/// Build a world-space picking ray from normalized device coords (`ndc` in −1…1,
/// y-up). Uses the camera's unprojection (inverse view-proj).
pub fn ray_from_ndc(camera: &Camera, ndc_x: f32, ndc_y: f32) -> Ray {
    let inv = camera.view_proj().inverse();
    let near = inv * Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
    let far = inv * Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
    let near3 = near.truncate() / near.w.max(1e-8);
    let far3 = far.truncate() / far.w.max(1e-8);
    let direction = (far3 - near3).normalize_or_zero();
    Ray::new(near3, direction)
}

/// Convert a pointer position inside a viewport rect to NDC (−1…1, y-up).
pub fn pointer_to_ndc(pointer: [f32; 2], rect_min: [f32; 2], rect_size: [f32; 2]) -> (f32, f32) {
    let w = rect_size[0].max(1.0);
    let h = rect_size[1].max(1.0);
    let u = (pointer[0] - rect_min[0]) / w;
    let v = (pointer[1] - rect_min[1]) / h;
    let ndc_x = u * 2.0 - 1.0;
    let ndc_y = 1.0 - v * 2.0;
    (ndc_x, ndc_y)
}

/// Slab-method ray vs AABB. Returns entry `t` along the ray if hit in `0..=max_t`.
pub fn ray_aabb(ray: Ray, aabb: Aabb, max_t: f32) -> Option<f32> {
    let mut t_min = 0.0f32;
    let mut t_max = max_t;

    for axis in 0..3 {
        let o = ray.origin[axis];
        let d = ray.direction[axis];
        let min_b = aabb.min[axis];
        let max_b = aabb.max[axis];

        if d.abs() < 1e-8 {
            if o < min_b || o > max_b {
                return None;
            }
            continue;
        }

        let inv = 1.0 / d;
        let mut t1 = (min_b - o) * inv;
        let mut t2 = (max_b - o) * inv;
        if t1 > t2 {
            std::mem::swap(&mut t1, &mut t2);
        }
        t_min = t_min.max(t1);
        t_max = t_max.min(t2);
        if t_min > t_max {
            return None;
        }
    }

    if t_min < 0.0 || t_min > max_t {
        None
    } else {
        Some(t_min)
    }
}

/// Closest hit among `(entity_index, world_aabb)` candidates.
pub fn pick_closest(ray: Ray, candidates: &[(usize, Aabb)], max_t: f32) -> Option<(usize, f32)> {
    let mut best: Option<(usize, f32)> = None;
    for &(index, aabb) in candidates {
        if let Some(t) = ray_aabb(ray, aabb, max_t) {
            if best.map(|(_, bt)| t < bt).unwrap_or(true) {
                best = Some((index, t));
            }
        }
    }
    best
}

/// Intersect ray with the infinite XZ plane at `y`.
pub fn ray_plane_y(ray: Ray, y: f32) -> Option<Vec3> {
    if ray.direction.y.abs() < 1e-8 {
        return None;
    }
    let t = (y - ray.origin.y) / ray.direction.y;
    if t < 0.0 {
        return None;
    }
    Some(ray.at(t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kerabit_math::vec3;

    #[test]
    fn mesh_aabb_cube() {
        let aabb = Aabb::from_mesh(&Mesh::cube());
        assert!((aabb.min.x + 0.5).abs() < 1e-5);
        assert!((aabb.max.x - 0.5).abs() < 1e-5);
    }

    #[test]
    fn pick_hits_translated_cube() {
        let local = Aabb::from_mesh(&Mesh::cube());
        let model = Mat4::from_translation(vec3(0.0, 0.0, 5.0));
        let world = local.transformed(model);
        let ray = Ray::new(vec3(0.0, 0.0, 0.0), vec3(0.0, 0.0, 1.0));
        let t = ray_aabb(ray, world, 100.0).expect("hit");
        assert!((t - 4.5).abs() < 1e-3);
        let (idx, _) = pick_closest(ray, &[(3, world)], 100.0).unwrap();
        assert_eq!(idx, 3);
    }

    #[test]
    fn ndc_ray_looks_forward() {
        let cam = Camera::perspective(60.0).look_at(vec3(0.0, 0.0, 5.0), Vec3::ZERO);
        let ray = ray_from_ndc(&cam, 0.0, 0.0);
        assert!(ray.direction.z < 0.0);
    }
}
