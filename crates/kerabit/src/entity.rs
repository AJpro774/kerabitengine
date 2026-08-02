//! Spawn descriptor for the public builder API.

use kerabit_math::{Quat, Vec3};

use crate::material::Material;
use crate::mesh::Mesh;

/// Description of an entity to [`crate::Kerabit::spawn`].
///
/// After the engine starts, look up live entities with
/// [`kerabit_world::World::get_mut`] via [`crate::Context::world_mut`].
#[derive(Clone, Debug)]
pub struct Entity {
    pub(crate) name: String,
    pub(crate) mesh: Option<Mesh>,
    pub(crate) material: Material,
    pub(crate) translation: Vec3,
    pub(crate) rotation: Quat,
    pub(crate) scale: Vec3,
    pub(crate) parent: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) layer: u32,
    pub(crate) enabled: bool,
}

impl Entity {
    /// Named entity (unique within the world).
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            mesh: None,
            material: Material::default(),
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
            parent: None,
            tags: Vec::new(),
            layer: kerabit_world::LAYER_DEFAULT,
            enabled: true,
        }
    }

    /// Attach a mesh (required for drawing).
    pub fn mesh(mut self, mesh: Mesh) -> Self {
        self.mesh = Some(mesh);
        self
    }

    /// Attach a material (default white).
    pub fn material(mut self, material: Material) -> Self {
        self.material = material;
        self
    }

    /// Set local-space translation (world-space if no parent).
    pub fn at(mut self, position: Vec3) -> Self {
        self.translation = position;
        self
    }

    /// Set local-space rotation (identity by default).
    pub fn rotation(mut self, rotation: Quat) -> Self {
        self.rotation = rotation;
        self
    }

    /// Set local-space non-uniform scale (`Vec3::ONE` by default).
    pub fn scale(mut self, scale: Vec3) -> Self {
        self.scale = scale;
        self
    }

    /// Parent this entity under another spawned entity by name.
    ///
    /// Local translation from [`Self::at`] is relative to the parent.
    pub fn parent(mut self, name: impl Into<String>) -> Self {
        self.parent = Some(name.into());
        self
    }

    /// Replace gameplay tags applied at spawn.
    pub fn tags(mut self, tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tags = tags.into_iter().map(Into::into).collect();
        self
    }

    /// Add a single tag at spawn.
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        let tag = tag.into();
        if !self.tags.iter().any(|t| t == &tag) {
            self.tags.push(tag);
        }
        self
    }

    /// Bitmask layer applied at spawn (default [`kerabit_world::LAYER_DEFAULT`]).
    pub fn layer(mut self, layer: u32) -> Self {
        self.layer = layer;
        self
    }

    /// Whether the entity starts enabled (drawn / queryable as enabled).
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}
