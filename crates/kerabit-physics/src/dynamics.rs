//! Dynamic rigid bodies (AABB / sphere) with gravity and static resolve.

use kerabit_math::Vec3;

use crate::{resolve_penetration, Aabb};

/// Opaque dynamic body handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BodyId(pub(crate) u64);

impl BodyId {
    /// Raw id (debug / serialization).
    #[inline]
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// Collision shape for a dynamic body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BodyShape {
    /// Axis-aligned box with half-extents.
    Aabb { half_extents: Vec3 },
    /// Sphere with radius (resolved as AABB of edge `2 * radius` for M2).
    Sphere { radius: f32 },
}

impl BodyShape {
    /// Half-extents used for AABB resolve (spheres use equal XYZ).
    #[inline]
    pub fn half_extents(self) -> Vec3 {
        match self {
            BodyShape::Aabb { half_extents } => half_extents.abs(),
            BodyShape::Sphere { radius } => {
                let r = radius.abs();
                Vec3::splat(r)
            }
        }
    }
}

/// Dynamic rigid body integrated by [`crate::PhysicsWorld::step`].
#[derive(Clone, Debug)]
pub struct DynamicBody {
    pub(crate) id: BodyId,
    /// World-space center.
    pub position: Vec3,
    /// Linear velocity.
    pub velocity: Vec3,
    /// Collision shape.
    pub shape: BodyShape,
    /// Mass in kg (> 0). Unused for M2 resolve (equal mass vs static).
    pub mass: f32,
    /// Bounce factor `0..=1` applied on contact normal.
    pub restitution: f32,
    /// Linear damping per second (velocity *= exp(-damping * dt)).
    pub damping: f32,
}

impl DynamicBody {
    /// AABB dynamic body at rest.
    pub fn aabb(position: Vec3, half_extents: Vec3, mass: f32) -> Self {
        Self {
            id: BodyId(0),
            position,
            velocity: Vec3::ZERO,
            shape: BodyShape::Aabb {
                half_extents: half_extents.abs(),
            },
            mass: mass.max(1e-4),
            restitution: 0.0,
            damping: 0.05,
        }
    }

    /// Sphere dynamic body at rest (resolved as a cube of matching radius).
    pub fn sphere(position: Vec3, radius: f32, mass: f32) -> Self {
        Self {
            id: BodyId(0),
            position,
            velocity: Vec3::ZERO,
            shape: BodyShape::Sphere {
                radius: radius.abs(),
            },
            mass: mass.max(1e-4),
            restitution: 0.2,
            damping: 0.05,
        }
    }

    /// Body id assigned by the physics world.
    #[inline]
    pub fn id(&self) -> BodyId {
        self.id
    }

    /// World AABB for queries / debug.
    #[inline]
    pub fn world_aabb(&self) -> Aabb {
        Aabb::from_center_half_extents(self.position, self.shape.half_extents())
    }

    /// Builder: set restitution (`0..=1`).
    pub fn with_restitution(mut self, restitution: f32) -> Self {
        self.restitution = restitution.clamp(0.0, 1.0);
        self
    }

    /// Builder: set linear damping.
    pub fn with_damping(mut self, damping: f32) -> Self {
        self.damping = damping.max(0.0);
        self
    }
}

pub(crate) fn integrate_dynamics(bodies: &mut [DynamicBody], gravity: Vec3, dt: f32) {
    for body in bodies.iter_mut() {
        body.velocity += gravity * dt;
        if body.damping > 0.0 {
            let damp = (-body.damping * dt).exp();
            body.velocity *= damp;
        }
        body.position += body.velocity * dt;
    }
}

pub(crate) fn resolve_dynamic_vs_statics(body: &mut DynamicBody, statics: &[Aabb]) {
    let half = body.shape.half_extents();
    // Iterate a few times for stacked contacts.
    for _ in 0..4 {
        let mover = Aabb::from_center_half_extents(body.position, half);
        let mut resolved = false;
        for obstacle in statics {
            if !mover.overlaps(*obstacle) {
                continue;
            }
            let before = body.position;
            body.position = resolve_penetration(body.position, half, *obstacle);
            let delta = body.position - before;
            // Cancel / bounce velocity along the push-out axis.
            for axis in 0..3 {
                if delta[axis].abs() < 1e-8 {
                    continue;
                }
                let sign = delta[axis].signum();
                // If velocity points into the obstacle, bounce / zero it.
                if body.velocity[axis] * sign < 0.0 {
                    body.velocity[axis] = -body.velocity[axis] * body.restitution;
                } else if body.velocity[axis].abs() < 1e-3 {
                    body.velocity[axis] = 0.0;
                }
            }
            resolved = true;
            break;
        }
        if !resolved {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kerabit_math::vec3;

    #[test]
    fn sphere_half_extents_match_radius() {
        let s = BodyShape::Sphere { radius: 0.5 };
        assert_eq!(s.half_extents(), vec3(0.5, 0.5, 0.5));
    }
}
