//! Kerabit — lean native Rust 3D engine.
//!
//! # Status
//!
//! **P7 + E0 + M2**: scenes, mid-run reload, dynamics, character controller,
//! clip animation. wgpu / winit types are never re-exported.
//!
//! Game authors should depend on this crate only.

mod assets;
mod context;
mod engine;
mod entity;
mod input_map;
mod material;
mod mesh;
mod scene;
mod ui;

pub mod math {
    //! Math types (glam re-exports and helpers).
    pub use kerabit_math::*;
}

pub mod color {
    //! RGBA colors and named constants.
    pub use kerabit_color::*;
}

pub mod world {
    //! Entities, transforms, and the scene store.
    //!
    //! Live spawned entities are [`kerabit_world::Entity`] (accessed via
    //! [`World::get`] / [`World::get_mut`]). The public spawn builder is
    //! [`crate::Entity`].
    pub use kerabit_world::*;
}

pub mod input {
    //! Input snapshot and key / mouse enums.
    pub use kerabit_input::*;
}

pub mod physics {
    //! AABB collision, ray/sphere casts, dynamics, character controller.
    pub use kerabit_physics::*;
}

pub mod anim {
    //! Clip animation playback on transform hierarchies.
    pub use kerabit_anim::*;
}

pub mod audio {
    //! Sound playback (path, volume, loop, spatial, buses, streaming music).
    pub use kerabit_audio::*;
}

pub use assets::{load_gltf, Texture};
pub use context::Context;
pub use engine::Kerabit;
pub use entity::Entity;
pub use kerabit_anim::{
    translation_clip, AnimChannel, AnimationClip, AnimationPlayer, Interpolation, QuatKey, Vec3Key,
};
pub use kerabit_assets::AssetError;
pub use kerabit_audio::{AudioEngine, AudioError, AudioListener, MixBus, SoundId};
pub use kerabit_color::Color;
pub use kerabit_input::{InputState, Key, MouseButton};
pub use kerabit_math::{vec2, vec3, Deg, Mat3, Mat4, Quat, Rad, Vec2, Vec3, Vec4};
pub use kerabit_physics::{
    Aabb, BodyId, BodyShape, CharacterController, CharacterMove, ColliderId, DynamicBody,
    MoveResult, PhysicsWorld, RayHit, SphereCastHit,
};
pub use kerabit_render::{Camera, Light, LightKind, ParticleBurst, MAX_LIGHTS};
pub use kerabit_world::{EntityId, Transform, World, LAYER_DEFAULT};
pub use material::Material;
pub use mesh::Mesh;
pub use scene::{
    Prefab, Scene, SceneCamera, SceneEntity, SceneError, SceneLight, SceneMap, SceneMaterial,
    SceneMesh, SCENE_VERSION,
};
pub use ui::Ui;

/// Convenience re-exports for game code.
pub mod prelude {
    pub use crate::{
        load_gltf, translation_clip, vec3, Aabb, AnimChannel, AnimationClip, AnimationPlayer,
        AssetError, AudioEngine, AudioError, AudioListener, BodyId, BodyShape, Camera,
        CharacterController, CharacterMove, Color, Context, Deg, DynamicBody, Entity, EntityId,
        InputState, Kerabit, Key, Light, LightKind, Material, Mat4, Mesh, MixBus, MouseButton,
        ParticleBurst, PhysicsWorld, Prefab, Quat, QuatKey, Rad, Scene, SceneError, SoundId,
        Texture, Transform, Ui, Vec2, Vec3, Vec3Key, World, LAYER_DEFAULT, MAX_LIGHTS,
    };
}
