//! AABB collision, ray/sphere casts, and kinematic blocking for Kerabit.
//!
//! No PhysX / Rapier — axis-aligned boxes only. Game code registers static
//! colliders and resolves movement with [`PhysicsWorld::move_and_collide`].

use kerabit_math::Vec3;

/// Opaque collider handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ColliderId(u64);

impl ColliderId {
    /// Raw id (debug / serialization).
    #[inline]
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// Axis-aligned bounding box in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    /// Inclusive minimum corner.
    pub min: Vec3,
    /// Inclusive maximum corner.
    pub max: Vec3,
}

impl Aabb {
    /// Build from min/max corners (swaps axes if needed so min ≤ max).
    #[inline]
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self {
            min: min.min(max),
            max: min.max(max),
        }
    }

    /// Box centered at `center` with the given half-extents.
    #[inline]
    pub fn from_center_half_extents(center: Vec3, half_extents: Vec3) -> Self {
        let half = half_extents.abs();
        Self {
            min: center - half,
            max: center + half,
        }
    }

    /// Center of the box.
    #[inline]
    pub fn center(self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    /// Half-extents from center to faces.
    #[inline]
    pub fn half_extents(self) -> Vec3 {
        (self.max - self.min) * 0.5
    }

    /// True if this box overlaps `other` (touching edges count as overlap).
    #[inline]
    pub fn overlaps(self, other: Self) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
            && self.min.z <= other.max.z
            && self.max.z >= other.min.z
    }

    /// Closest point on/in the box to `p`.
    #[inline]
    pub fn closest_point(self, p: Vec3) -> Vec3 {
        p.clamp(self.min, self.max)
    }

    /// Expand by `margin` on every side (used for sphere → AABB inflate).
    #[inline]
    pub fn expand(self, margin: f32) -> Self {
        let m = Vec3::splat(margin);
        Self {
            min: self.min - m,
            max: self.max + m,
        }
    }

    /// Minkowski sum with another AABB's half-extents (for swept volume tests).
    #[inline]
    pub fn expand_by_half_extents(self, half: Vec3) -> Self {
        let h = half.abs();
        Self {
            min: self.min - h,
            max: self.max + h,
        }
    }
}

/// Result of a ray vs AABB query.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    /// Distance along the ray (`origin + direction * t`).
    pub t: f32,
    /// World-space hit point.
    pub point: Vec3,
    /// Outward normal of the hit face.
    pub normal: Vec3,
    /// Collider that was hit.
    pub collider: ColliderId,
}

/// Result of a sphere cast vs AABBs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SphereCastHit {
    /// Distance traveled along `direction` before contact.
    pub distance: f32,
    /// Sphere center at contact.
    pub point: Vec3,
    /// Contact normal (from obstacle toward sphere).
    pub normal: Vec3,
    /// Collider that was hit.
    pub collider: ColliderId,
}

/// Result of a kinematic move with blocking.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveResult {
    /// Final center position after resolution.
    pub position: Vec3,
    /// True if any collider blocked the path.
    pub hit: bool,
}

/// Static AABB collider store + queries.
#[derive(Debug, Default)]
pub struct PhysicsWorld {
    next_id: u64,
    colliders: Vec<(ColliderId, Aabb)>,
}

impl PhysicsWorld {
    /// Empty physics world.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of registered colliders.
    #[inline]
    pub fn len(&self) -> usize {
        self.colliders.len()
    }

    /// True if no colliders are registered.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.colliders.is_empty()
    }

    /// Register a static AABB; returns a handle for later remove/update.
    pub fn add_aabb(&mut self, aabb: Aabb) -> ColliderId {
        let id = ColliderId(self.next_id);
        self.next_id += 1;
        self.colliders.push((id, aabb));
        id
    }

    /// Remove a collider by id. Returns `true` if it existed.
    pub fn remove(&mut self, id: ColliderId) -> bool {
        if let Some(i) = self.colliders.iter().position(|(c, _)| *c == id) {
            self.colliders.swap_remove(i);
            true
        } else {
            false
        }
    }

    /// Remove every collider. Collider ids are not reused (`next_id` kept).
    pub fn clear(&mut self) {
        self.colliders.clear();
    }

    /// Replace an existing collider's AABB.
    pub fn set_aabb(&mut self, id: ColliderId, aabb: Aabb) -> bool {
        if let Some((_, box_)) = self.colliders.iter_mut().find(|(c, _)| *c == id) {
            *box_ = aabb;
            true
        } else {
            false
        }
    }

    /// Lookup a collider AABB.
    pub fn get(&self, id: ColliderId) -> Option<Aabb> {
        self.colliders
            .iter()
            .find(|(c, _)| *c == id)
            .map(|(_, a)| *a)
    }

    /// True if `aabb` overlaps any registered collider.
    pub fn overlaps_aabb(&self, aabb: Aabb) -> bool {
        self.colliders.iter().any(|(_, other)| aabb.overlaps(*other))
    }

    /// First collider overlapping `aabb`, if any.
    pub fn first_overlap(&self, aabb: Aabb) -> Option<(ColliderId, Aabb)> {
        self.colliders
            .iter()
            .find(|(_, other)| aabb.overlaps(*other))
            .map(|(id, a)| (*id, *a))
    }

    /// Raycast against all AABBs. `direction` should be normalized.
    ///
    /// Returns the closest hit with `0 <= t <= max_t`.
    pub fn raycast(&self, origin: Vec3, direction: Vec3, max_t: f32) -> Option<RayHit> {
        let mut best: Option<RayHit> = None;
        for &(id, aabb) in &self.colliders {
            if let Some((t, normal)) = ray_aabb(origin, direction, aabb, max_t) {
                if best.map(|h| t < h.t).unwrap_or(true) {
                    best = Some(RayHit {
                        t,
                        point: origin + direction * t,
                        normal,
                        collider: id,
                    });
                }
            }
        }
        best
    }

    /// Sphere cast (swept sphere) vs AABBs. `direction` should be normalized.
    ///
    /// Implemented as a raycast against AABBs expanded by `radius`.
    pub fn sphere_cast(
        &self,
        origin: Vec3,
        radius: f32,
        direction: Vec3,
        max_distance: f32,
    ) -> Option<SphereCastHit> {
        let radius = radius.max(0.0);
        let mut best: Option<SphereCastHit> = None;
        for &(id, aabb) in &self.colliders {
            let expanded = aabb.expand(radius);
            if let Some((t, normal)) = ray_aabb(origin, direction, expanded, max_distance) {
                if best.map(|h| t < h.distance).unwrap_or(true) {
                    best = Some(SphereCastHit {
                        distance: t,
                        point: origin + direction * t,
                        normal,
                        collider: id,
                    });
                }
            }
        }
        best
    }

    /// Kinematic AABB move with axis-separated block resolution.
    ///
    /// Moves an AABB of `half_extents` centered at `position` by `velocity * dt`,
    /// sliding along surfaces when blocked. Uses a swept test (expanded-AABB
    /// raycast) so large timesteps do not tunnel through thin walls.
    pub fn move_and_collide(
        &self,
        position: Vec3,
        velocity: Vec3,
        half_extents: Vec3,
        dt: f32,
    ) -> MoveResult {
        let half = half_extents.abs();
        let mut pos = position;
        let delta = velocity * dt;
        let mut hit = false;

        // If already overlapping, push out before sweeping.
        {
            let mover = Aabb::from_center_half_extents(pos, half);
            if let Some((_, obstacle)) = self.first_overlap(mover) {
                hit = true;
                pos = resolve_penetration(pos, half, obstacle);
            }
        }

        // Resolve one axis at a time so we slide along walls.
        for axis in 0..3 {
            let travel = delta[axis];
            if travel.abs() < f32::EPSILON {
                continue;
            }

            let mut dir = Vec3::ZERO;
            dir[axis] = travel.signum();
            let max_dist = travel.abs();

            let mut allowed = max_dist;
            for &(_, obstacle) in &self.colliders {
                let expanded = obstacle.expand_by_half_extents(half);
                // Origin inside expanded box → already touching / penetrating.
                if point_in_aabb(pos, expanded) {
                    allowed = 0.0;
                    hit = true;
                    break;
                }
                if let Some((t, _)) = ray_aabb(pos, dir, expanded, max_dist) {
                    if t < allowed {
                        allowed = t;
                        hit = true;
                    }
                }
            }

            if allowed > 0.0 {
                // Back off a hair when we hit so the next frame is not stuck inside.
                let pad = if hit && allowed < max_dist {
                    EPSILON
                } else {
                    0.0
                };
                pos[axis] += dir[axis] * (allowed - pad).max(0.0);
            }
        }

        // Final depenetration for corner cases.
        let mover = Aabb::from_center_half_extents(pos, half);
        if let Some((_, obstacle)) = self.first_overlap(mover) {
            hit = true;
            pos = resolve_penetration(pos, half, obstacle);
        }

        MoveResult {
            position: pos,
            hit,
        }
    }
}

const EPSILON: f32 = 1e-4;

#[inline]
fn point_in_aabb(p: Vec3, aabb: Aabb) -> bool {
    p.x >= aabb.min.x
        && p.x <= aabb.max.x
        && p.y >= aabb.min.y
        && p.y <= aabb.max.y
        && p.z >= aabb.min.z
        && p.z <= aabb.max.z
}

fn resolve_penetration(center: Vec3, half: Vec3, obstacle: Aabb) -> Vec3 {
    let mover = Aabb::from_center_half_extents(center, half);
    let px_pos = mover.max.x - obstacle.min.x;
    let px_neg = obstacle.max.x - mover.min.x;
    let py_pos = mover.max.y - obstacle.min.y;
    let py_neg = obstacle.max.y - mover.min.y;
    let pz_pos = mover.max.z - obstacle.min.z;
    let pz_neg = obstacle.max.z - mover.min.z;

    let mut best_axis = 0usize;
    let mut best_sign = 1.0f32;
    let mut best = f32::MAX;

    for (axis, (pos, neg)) in [(px_pos, px_neg), (py_pos, py_neg), (pz_pos, pz_neg)]
        .into_iter()
        .enumerate()
    {
        if pos < best && pos >= 0.0 {
            best = pos;
            best_axis = axis;
            best_sign = -1.0;
        }
        if neg < best && neg >= 0.0 {
            best = neg;
            best_axis = axis;
            best_sign = 1.0;
        }
    }

    let mut out = center;
    out[best_axis] += best_sign * (best + EPSILON);
    out
}

/// Slab method ray vs AABB. Returns `(t_enter, face_normal)` if hit in range.
fn ray_aabb(origin: Vec3, dir: Vec3, aabb: Aabb, max_t: f32) -> Option<(f32, Vec3)> {
    let mut t_min = 0.0f32;
    let mut t_max = max_t;
    let mut hit_axis = 0usize;
    let mut hit_sign = 1.0f32;

    for axis in 0..3 {
        let o = origin[axis];
        let d = dir[axis];
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
        let mut sign = -1.0;
        if t1 > t2 {
            std::mem::swap(&mut t1, &mut t2);
            sign = 1.0;
        }

        if t1 > t_min {
            t_min = t1;
            hit_axis = axis;
            hit_sign = sign;
        }
        t_max = t_max.min(t2);

        if t_min > t_max {
            return None;
        }
    }

    if t_min < 0.0 || t_min > max_t {
        // Origin inside: report exit? For gameplay, treat as no forward hit.
        return None;
    }

    let mut normal = Vec3::ZERO;
    normal[hit_axis] = hit_sign;
    Some((t_min, normal))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kerabit_math::vec3;

    #[test]
    fn aabb_overlap() {
        let a = Aabb::from_center_half_extents(vec3(0.0, 0.0, 0.0), vec3(0.5, 0.5, 0.5));
        let b = Aabb::from_center_half_extents(vec3(0.9, 0.0, 0.0), vec3(0.5, 0.5, 0.5));
        let c = Aabb::from_center_half_extents(vec3(2.0, 0.0, 0.0), vec3(0.5, 0.5, 0.5));
        assert!(a.overlaps(b));
        assert!(!a.overlaps(c));
    }

    #[test]
    fn raycast_hits_box() {
        let mut phys = PhysicsWorld::new();
        let id = phys.add_aabb(Aabb::from_center_half_extents(
            vec3(0.0, 0.0, 5.0),
            vec3(1.0, 1.0, 1.0),
        ));
        let hit = phys
            .raycast(vec3(0.0, 0.0, 0.0), vec3(0.0, 0.0, 1.0), 100.0)
            .expect("hit");
        assert_eq!(hit.collider, id);
        assert!((hit.t - 4.0).abs() < 1e-3);
        assert!((hit.normal.z + 1.0).abs() < 1e-3);
    }

    #[test]
    fn move_and_collide_blocks() {
        let mut phys = PhysicsWorld::new();
        phys.add_aabb(Aabb::from_center_half_extents(
            vec3(2.0, 0.5, 0.0),
            vec3(0.5, 0.5, 0.5),
        ));
        let half = vec3(0.4, 0.4, 0.4);
        let start = vec3(0.0, 0.5, 0.0);
        let result = phys.move_and_collide(start, vec3(10.0, 0.0, 0.0), half, 1.0);
        assert!(result.hit);
        // Should stop just before the obstacle (obstacle min x = 1.5, half = 0.4 → ~1.1).
        assert!(result.position.x < 1.2);
        assert!(result.position.x > 0.9);
    }

    #[test]
    fn sphere_cast_finds_wall() {
        let mut phys = PhysicsWorld::new();
        phys.add_aabb(Aabb::from_center_half_extents(
            vec3(0.0, 0.0, 3.0),
            vec3(1.0, 1.0, 0.5),
        ));
        let hit = phys
            .sphere_cast(vec3(0.0, 0.0, 0.0), 0.5, vec3(0.0, 0.0, 1.0), 10.0)
            .expect("hit");
        // Expanded AABB min.z = 2.5, radius expansion → contact at 2.5 - 0? expand by 0.5 → min.z=2.0
        assert!(hit.distance < 2.1);
        assert!(hit.distance > 1.9);
    }

    #[test]
    fn clear_removes_all_colliders() {
        let mut phys = PhysicsWorld::new();
        phys.add_aabb(Aabb::from_center_half_extents(
            vec3(0.0, 0.0, 0.0),
            vec3(1.0, 1.0, 1.0),
        ));
        phys.add_aabb(Aabb::from_center_half_extents(
            vec3(5.0, 0.0, 0.0),
            vec3(1.0, 1.0, 1.0),
        ));
        assert_eq!(phys.len(), 2);
        phys.clear();
        assert!(phys.is_empty());
        assert!(phys
            .raycast(vec3(0.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0), 100.0)
            .is_none());
    }
}
