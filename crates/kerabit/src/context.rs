//! Per-frame context passed to the [`crate::Kerabit::run`] closure.

use std::collections::HashMap;
use std::path::Path;

use kerabit_audio::AudioEngine;
use kerabit_color::Color;
use kerabit_input::InputState;
use kerabit_physics::PhysicsWorld;
use kerabit_render::{Camera, GpuState, Light};
use kerabit_world::{EntityId, World};

use crate::engine::{spawn_entities, Renderable};
use crate::entity::Entity;
use crate::scene::{Scene, SceneError};
use crate::ui::Ui;

/// Frame context: timing, input, scene, camera, physics, audio, UI, and quit.
pub struct Context<'a> {
    pub(crate) dt: f32,
    pub(crate) input: &'a InputState,
    pub(crate) world: &'a mut World,
    pub(crate) camera: &'a mut Camera,
    pub(crate) physics: &'a mut PhysicsWorld,
    pub(crate) audio: &'a mut AudioEngine,
    pub(crate) ui: &'a mut Ui,
    pub(crate) quit: &'a mut bool,
    pub(crate) gpu: Option<&'a mut GpuState>,
    pub(crate) renderables: &'a mut HashMap<EntityId, Renderable>,
    pub(crate) light: &'a mut Light,
    pub(crate) ambient: &'a mut Color,
    pub(crate) clear_color: &'a mut Color,
}

impl Context<'_> {
    /// Seconds since the previous frame.
    #[inline]
    pub fn dt(&self) -> f32 {
        self.dt
    }

    /// Input snapshot for this frame.
    #[inline]
    pub fn input(&self) -> &InputState {
        self.input
    }

    /// Immutable world (named entity lookup).
    #[inline]
    pub fn world(&self) -> &World {
        self.world
    }

    /// Mutable world (rotate / translate entities).
    ///
    /// Prefer [`Self::despawn`] / [`Self::spawn`] / [`Self::clear_world`] when
    /// adding or removing entities so GPU draw entries stay in sync.
    #[inline]
    pub fn world_mut(&mut self) -> &mut World {
        self.world
    }

    /// Active camera (read).
    #[inline]
    pub fn camera(&self) -> &Camera {
        self.camera
    }

    /// Active camera (write) — orbit / WASD in game code.
    #[inline]
    pub fn camera_mut(&mut self) -> &mut Camera {
        self.camera
    }

    /// Static colliders + kinematic queries (AABB / ray / sphere cast).
    #[inline]
    pub fn physics(&mut self) -> &mut PhysicsWorld {
        self.physics
    }

    /// Sound playback (play by path, volume, loop).
    #[inline]
    pub fn audio(&mut self) -> &mut AudioEngine {
        self.audio
    }

    /// Immediate-mode screen overlay (text + rect). Cleared each frame.
    ///
    /// Coordinates are normalized `0..=1`, origin top-left — see [`Ui`].
    #[inline]
    pub fn ui(&mut self) -> &mut Ui {
        self.ui
    }

    /// Request the window to close after this frame.
    #[inline]
    pub fn quit(&mut self) {
        *self.quit = true;
    }

    /// Remove all entities, GPU draw entries, and physics colliders.
    ///
    /// Does not change camera, light, ambient, or clear color — follow with
    /// [`Self::apply_scene`] (or manual setup) to rebuild the level. Stays
    /// inside the current demand-run (no window / EventLoop recreate).
    pub fn clear_world(&mut self) {
        self.world.clear();
        self.renderables.clear();
        self.physics.clear();
    }

    /// Despawn a named entity and drop its GPU draw entry.
    ///
    /// Returns `false` if no entity with that name exists.
    pub fn despawn(&mut self, name: &str) -> bool {
        let Some(id) = self.world.id_of(name) else {
            return false;
        };
        self.despawn_id(id)
    }

    /// Despawn by [`EntityId`] and drop its GPU draw entry.
    pub fn despawn_id(&mut self, id: EntityId) -> bool {
        if !self.world.despawn(id) {
            return false;
        }
        self.renderables.remove(&id);
        true
    }

    /// Upload mesh/material and insert into the world + draw list (mid-run spawn).
    ///
    /// Requires the GPU to be ready (after the first frame of a running session).
    pub fn spawn(&mut self, entity: Entity) -> Result<EntityId, SceneError> {
        let gpu = self
            .gpu
            .as_mut()
            .ok_or_else(|| SceneError::Spawn("GPU not ready".into()))?;
        let ids = spawn_entities(self.world, self.renderables, gpu, vec![entity])?;
        Ok(ids[0])
    }

    /// Clear the world, then apply camera / light / ambient / clear color and
    /// spawn every entity from `scene` — without ending the demand-run.
    ///
    /// Physics colliders are cleared; the game must re-register statics
    /// (e.g. wall AABBs) after this call. Window aspect is preserved on the
    /// new camera.
    pub fn apply_scene(&mut self, scene: &Scene) -> Result<(), SceneError> {
        let entities = scene.build_entities()?;
        self.clear_world();

        *self.clear_color = scene.clear_color;
        *self.ambient = scene.ambient;
        *self.light = scene.to_light();
        let aspect = self.gpu.as_ref().map(|g| g.aspect()).unwrap_or(16.0 / 9.0);
        *self.camera = scene.to_camera();
        self.camera.set_aspect(aspect);
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.clear_color = scene.clear_color;
        }

        let gpu = self
            .gpu
            .as_mut()
            .ok_or_else(|| SceneError::Spawn("GPU not ready".into()))?;
        spawn_entities(self.world, self.renderables, gpu, entities)?;
        Ok(())
    }

    /// Load `.kerabit.json` from `path` and [`Self::apply_scene`].
    pub fn load_scene(&mut self, path: impl AsRef<Path>) -> Result<(), SceneError> {
        self.apply_scene(&Scene::load(path)?)
    }
}
