//! Kerabit — lean native Rust 3D engine.
//!
//! # Status
//!
//! **P7 + E0**: `.kerabit.json` scenes + mid-run `Context::apply_scene` reload.
//! wgpu / winit types are never re-exported.
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
    //! AABB collision, ray/sphere casts, kinematic move.
    pub use kerabit_physics::*;
}

pub mod audio {
    //! Sound playback (path, volume, loop).
    pub use kerabit_audio::*;
}

pub use assets::{load_gltf, Texture};
pub use context::Context;
pub use engine::Kerabit;
pub use entity::Entity;
pub use kerabit_assets::AssetError;
pub use kerabit_audio::{AudioEngine, AudioError, SoundId};
pub use kerabit_color::Color;
pub use kerabit_input::{InputState, Key, MouseButton};
pub use kerabit_math::{vec2, vec3, Deg, Mat3, Mat4, Quat, Rad, Vec2, Vec3, Vec4};
pub use kerabit_physics::{Aabb, ColliderId, MoveResult, PhysicsWorld, RayHit, SphereCastHit};
pub use kerabit_render::{Camera, Light};
pub use kerabit_world::{EntityId, Transform, World};
pub use material::Material;
pub use mesh::Mesh;
pub use scene::{
    Scene, SceneCamera, SceneEntity, SceneError, SceneLight, SceneMaterial, SceneMesh,
    SCENE_VERSION,
};
pub use ui::Ui;

/// Convenience re-exports for game code.
pub mod prelude {
    pub use crate::{
        load_gltf, vec3, Aabb, AssetError, AudioEngine, AudioError, Camera, Color, Context, Deg,
        Entity, EntityId, InputState, Kerabit, Key, Light, Material, Mat4, Mesh, MouseButton,
        PhysicsWorld, Quat, Rad, Scene, SceneError, SoundId, Texture, Transform, Ui, Vec2, Vec3,
        World,
    };
}
