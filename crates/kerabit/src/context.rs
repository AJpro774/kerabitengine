//! Per-frame context passed to the [`crate::Kerabit::run`] closure.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use kerabit_audio::AudioEngine;
use kerabit_color::Color;
use kerabit_input::InputState;
use kerabit_math::Vec3;
use kerabit_physics::{Aabb, PhysicsWorld};
use kerabit_render::{clamp_lights, Camera, GpuState, Light, ParticleBurst};
use kerabit_script::{PrimitiveKind, ScriptEffects, ScriptRuntime};
use kerabit_world::{EntityId, World};

use crate::engine::{spawn_entities, Renderable};
use crate::entity::Entity;
use crate::material::Material;
use crate::mesh::Mesh;
use crate::scene::{load_script_attachments, Prefab, Scene, SceneError};
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
    pub(crate) lights: &'a mut Vec<Light>,
    pub(crate) ambient: &'a mut Color,
    pub(crate) clear_color: &'a mut Color,
    pub(crate) scripts: &'a mut ScriptRuntime,
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

    /// Active lights (max [`MAX_LIGHTS`]). Soft shadows use the first directional.
    #[inline]
    pub fn lights(&self) -> &[Light] {
        self.lights
    }

    /// Replace lights (truncated to [`MAX_LIGHTS`]). Empty list restores a default sun.
    pub fn set_lights(&mut self, lights: impl IntoIterator<Item = Light>) {
        *self.lights = clamp_lights(&lights.into_iter().collect::<Vec<_>>());
        if self.lights.is_empty() {
            *self.lights = vec![Light::sun(kerabit_math::vec3(-0.35, -1.0, -0.25)).intensity(1.2)];
        }
    }

    /// Primary light (slot 0). Prefer [`Self::lights`] / [`Self::set_lights`] for multi-light.
    #[inline]
    pub fn light_mut(&mut self) -> &mut Light {
        if self.lights.is_empty() {
            self.lights
                .push(Light::sun(kerabit_math::vec3(-0.35, -1.0, -0.25)).intensity(1.2));
        }
        &mut self.lights[0]
    }

    /// Emit a simple particle billboard burst (CPU sim, GPU quads).
    pub fn spawn_particles(&mut self, burst: ParticleBurst) {
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.spawn_particles(burst);
        }
    }

    /// Static colliders + kinematic queries (AABB / ray / sphere cast).
    #[inline]
    pub fn physics(&mut self) -> &mut PhysicsWorld {
        self.physics
    }

    /// Sound playback (play by path, volume, loop, spatial, buses).
    #[inline]
    pub fn audio(&mut self) -> &mut AudioEngine {
        self.audio
    }

    /// Point the audio listener at the active camera (stereo positional SFX).
    ///
    /// Call once per frame when using [`AudioEngine::play_at`] /
    /// [`AudioEngine::play_at_with`].
    pub fn sync_audio_listener(&mut self) {
        let eye = self.camera.eye;
        let target = self.camera.target;
        let up = self.camera.up;
        self.audio.follow_look_at(eye, target, up);
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

    /// Last Rhai error from this frame's script tick, if any.
    #[inline]
    pub fn script_error(&self) -> Option<&str> {
        self.scripts.last_error()
    }

    /// Run loaded Rhai scripts against this frame (called automatically after the `run` closure).
    pub(crate) fn tick_scripts(&mut self) {
        let effects = self
            .scripts
            .tick(self.dt, self.input, self.world, self.quit);
        self.apply_script_effects(effects);
    }

    /// Apply side effects that need GPU / physics / audio / UI.
    fn apply_script_effects(&mut self, effects: ScriptEffects) {
        let base = self.scripts.base_dir().map(Path::to_path_buf);

        for name in effects.despawns {
            let _ = self.despawn(&name);
        }

        for spawn in effects.spawns {
            let color = Color::rgb(spawn.color[0], spawn.color[1], spawn.color[2]);
            let mesh = match spawn.kind {
                PrimitiveKind::Cube => Mesh::cube(),
                PrimitiveKind::Plane => Mesh::plane(spawn.scale[0].abs().max(0.01)),
            };
            let scale = match spawn.kind {
                PrimitiveKind::Cube => Vec3::new(spawn.scale[0], spawn.scale[1], spawn.scale[2]),
                PrimitiveKind::Plane => Vec3::ONE,
            };
            let entity = Entity::new(spawn.name)
                .mesh(mesh)
                .material(Material::color(color))
                .at(Vec3::new(spawn.at[0], spawn.at[1], spawn.at[2]))
                .scale(scale);
            let _ = self.spawn(entity);
        }

        for prefab in effects.prefabs {
            let path = resolve_script_path(base.as_deref(), &prefab.path);
            if let Ok(prefab_data) = Prefab::load(&path) {
                let mut scratch = Scene::default();
                prefab_data.instantiate(
                    &mut scratch,
                    Vec3::new(prefab.offset[0], prefab.offset[1], prefab.offset[2]),
                );
                if let Ok(entities) = scratch.build_entities() {
                    for entity in entities {
                        let _ = self.spawn(entity);
                    }
                }
            }
        }

        for mv in effects.moves {
            let Some(entity) = self.world.get(&mv.name) else {
                continue;
            };
            let pos = entity.transform.translation();
            let scale = entity.transform.scale();
            let half = (scale * 0.5).abs().max(Vec3::splat(0.05));
            // Velocity so move_and_collide travels (dx, 0, dz) this frame.
            let inv_dt = if self.dt > 1e-6 { 1.0 / self.dt } else { 0.0 };
            let velocity = Vec3::new(mv.dx * inv_dt, 0.0, mv.dz * inv_dt);
            let result = self.physics.move_and_collide(pos, velocity, half, self.dt);
            if let Some(e) = self.world.get_mut(&mv.name) {
                e.transform.set_translation(result.position);
            }
        }

        for name in effects.register_boxes {
            let Some(entity) = self.world.get(&name) else {
                continue;
            };
            let pos = entity.transform.translation();
            let scale = entity.transform.scale();
            let half = (scale * 0.5).abs().max(Vec3::splat(0.05));
            self.physics
                .add_aabb(Aabb::from_center_half_extents(pos, half));
        }

        for play in effects.plays {
            let path = resolve_script_path(base.as_deref(), &play.path);
            match play.at {
                Some(at) => {
                    let _ = self.audio.play_at(&path, Vec3::new(at[0], at[1], at[2]));
                }
                None => {
                    let _ = self.audio.play(&path);
                }
            }
        }

        for p in effects.particles {
            self.spawn_particles(ParticleBurst {
                origin: Vec3::new(p.origin[0], p.origin[1], p.origin[2]),
                count: p.count.max(1),
                color: Color::rgb(p.color[0], p.color[1], p.color[2]),
                ..ParticleBurst::default()
            });
        }

        if let Some(cam) = effects.camera {
            *self.camera = self.camera.clone().look_at(
                Vec3::new(cam.eye[0], cam.eye[1], cam.eye[2]),
                Vec3::new(cam.target[0], cam.target[1], cam.target[2]),
            );
        }

        for r in effects.ui_rects {
            self.ui.rect(
                r.x,
                r.y,
                r.w,
                r.h,
                Color::rgba(r.color[0], r.color[1], r.color[2], r.color[3]),
            );
        }
        for t in effects.ui_texts {
            self.ui.text(
                t.x,
                t.y,
                t.size,
                Color::rgb(t.color[0], t.color[1], t.color[2]),
                &t.text,
            );
        }
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
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.clear_particles();
        }
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
    /// new camera. Scene JSON still authors a single sun (slot 0).
    pub fn apply_scene(&mut self, scene: &Scene) -> Result<(), SceneError> {
        let entities = scene.build_entities()?;
        self.clear_world();

        *self.clear_color = scene.clear_color;
        *self.ambient = scene.ambient;
        *self.lights = vec![scene.to_light()];
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
        // Resolve scripts like load_scene: prefer last scene dir, else keep prior base_dir.
        let base = self.scripts.base_dir().map(PathBuf::from);
        load_script_attachments(
            self.scripts,
            scene.script_attachments(),
            base.as_deref(),
        )?;
        Ok(())
    }

    /// Load `.kerabit.json` from `path` and [`Self::apply_scene`].
    ///
    /// Script paths in `extras.script` / `components.script` resolve relative
    /// to the scene file.
    pub fn load_scene(&mut self, path: impl AsRef<Path>) -> Result<(), SceneError> {
        let path = path.as_ref();
        let scene = Scene::load(path)?;
        let attachments = scene.script_attachments();
        let entities = scene.build_entities()?;
        self.clear_world();

        *self.clear_color = scene.clear_color;
        *self.ambient = scene.ambient;
        *self.lights = vec![scene.to_light()];
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
        load_script_attachments(self.scripts, attachments, path.parent())?;
        Ok(())
    }
}

fn resolve_script_path(base: Option<&Path>, rel: &str) -> PathBuf {
    let p = Path::new(rel);
    if p.is_absolute() {
        p.to_path_buf()
    } else if let Some(dir) = base {
        dir.join(p)
    } else {
        p.to_path_buf()
    }
}
