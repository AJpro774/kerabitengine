//! Character controller built on [`PhysicsWorld::move_and_collide`].

use kerabit_math::Vec3;

use crate::{MoveResult, PhysicsWorld};

/// Input / result for one character step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterMove {
    /// Final position after resolution.
    pub position: Vec3,
    /// True if any collider blocked movement this step.
    pub hit: bool,
    /// True if a short downward probe found ground under the feet.
    pub grounded: bool,
}

/// Kinematic character controller wrapping [`PhysicsWorld::move_and_collide`].
///
/// Use [`Self::move_wish`] for gravity + jump (3D sandbox), or
/// [`Self::move_planar`] for top-down / fixed-Y games like Reach.
#[derive(Clone, Debug)]
pub struct CharacterController {
    /// World-space center of the character AABB.
    pub position: Vec3,
    /// Current linear velocity (written by move helpers).
    pub velocity: Vec3,
    /// Half-extents of the character collider.
    pub half_extents: Vec3,
    /// Gravity acceleration applied on Y each step (`move_wish`).
    pub gravity: f32,
    /// Upward speed applied when jumping while grounded.
    pub jump_speed: f32,
    /// Max horizontal speed for wish-direction moves.
    pub max_speed: f32,
    /// Whether the last step found ground beneath the feet.
    pub grounded: bool,
    /// Snap Y to this value after planar moves (`None` = leave Y as resolved).
    pub planar_y: Option<f32>,
}

impl CharacterController {
    /// New controller at `position` with the given AABB half-extents.
    pub fn new(position: Vec3, half_extents: Vec3) -> Self {
        Self {
            position,
            velocity: Vec3::ZERO,
            half_extents: half_extents.abs(),
            gravity: 20.0,
            jump_speed: 8.0,
            max_speed: 6.0,
            grounded: false,
            planar_y: None,
        }
    }

    /// Top-down helper: pin Y after each move (Reach-style).
    pub fn planar(position: Vec3, half_extents: Vec3) -> Self {
        let mut cc = Self::new(position, half_extents);
        cc.gravity = 0.0;
        cc.planar_y = Some(position.y);
        cc
    }

    /// Builder: max horizontal speed.
    pub fn with_max_speed(mut self, speed: f32) -> Self {
        self.max_speed = speed.max(0.0);
        self
    }

    /// Builder: jump impulse speed.
    pub fn with_jump_speed(mut self, speed: f32) -> Self {
        self.jump_speed = speed.max(0.0);
        self
    }

    /// Builder: gravity magnitude (positive = downward).
    pub fn with_gravity(mut self, gravity: f32) -> Self {
        self.gravity = gravity;
        self
    }

    /// Move using a wish direction on XZ, optional jump, and gravity on Y.
    ///
    /// `wish_dir` is horizontal intent (Y ignored); length is clamped to 1 before
    /// multiplying by `max_speed`. Uses [`PhysicsWorld::move_and_collide`].
    pub fn move_wish(
        &mut self,
        phys: &PhysicsWorld,
        wish_dir: Vec3,
        jump: bool,
        dt: f32,
    ) -> CharacterMove {
        let dt = dt.max(0.0);
        let mut wish = Vec3::new(wish_dir.x, 0.0, wish_dir.z);
        if wish.length_squared() > 1.0 {
            wish = wish.normalize();
        } else if wish.length_squared() > 1e-8 {
            // keep partial stick
        } else {
            wish = Vec3::ZERO;
        }

        self.velocity.x = wish.x * self.max_speed;
        self.velocity.z = wish.z * self.max_speed;

        if self.grounded && jump {
            self.velocity.y = self.jump_speed;
            self.grounded = false;
        }

        self.velocity.y -= self.gravity * dt;

        let result = phys.move_and_collide(self.position, self.velocity, self.half_extents, dt);
        self.position = result.position;

        // Zero vertical velocity when blocked upward/downward this frame.
        if result.hit {
            // Probe: if we couldn't fall, likely grounded or ceiling.
            let down = phys.move_and_collide(
                self.position,
                Vec3::new(0.0, -1.0, 0.0),
                self.half_extents,
                0.08,
            );
            if (down.position.y - self.position.y).abs() < 1e-4 {
                if self.velocity.y < 0.0 {
                    self.velocity.y = 0.0;
                }
            }
        }

        self.grounded = self.probe_ground(phys);
        if self.grounded && self.velocity.y < 0.0 {
            self.velocity.y = 0.0;
        }

        CharacterMove {
            position: self.position,
            hit: result.hit,
            grounded: self.grounded,
        }
    }

    /// Planar / top-down move: horizontal wish only, optional Y snap.
    ///
    /// Compatible with Reach-style gameplay (no gravity). `speed` overrides
    /// [`Self::max_speed`] for this call when > 0.
    pub fn move_planar(
        &mut self,
        phys: &PhysicsWorld,
        wish_dir: Vec3,
        speed: f32,
        dt: f32,
    ) -> CharacterMove {
        let speed = if speed > 0.0 { speed } else { self.max_speed };
        let mut wish = Vec3::new(wish_dir.x, 0.0, wish_dir.z);
        let velocity = if wish.length_squared() > 1e-8 {
            wish = wish.normalize();
            wish * speed
        } else {
            Vec3::ZERO
        };
        self.velocity = velocity;

        let result: MoveResult =
            phys.move_and_collide(self.position, velocity, self.half_extents, dt);
        self.position = result.position;
        if let Some(y) = self.planar_y {
            self.position.y = y;
        }
        self.grounded = true;

        CharacterMove {
            position: self.position,
            hit: result.hit,
            grounded: true,
        }
    }

    fn probe_ground(&self, phys: &PhysicsWorld) -> bool {
        let probe = phys.move_and_collide(
            self.position,
            Vec3::new(0.0, -1.0, 0.0),
            self.half_extents,
            0.12,
        );
        (probe.position.y - self.position.y).abs() < 1e-3 || probe.hit
    }
}
