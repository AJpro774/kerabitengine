//! World, entities, and transforms for Kerabit.
//!
//! P4: parent/child hierarchy with world-matrix propagation.

use std::collections::{HashMap, HashSet};

use kerabit_math::{Mat4, Quat, Vec3};

/// Opaque entity identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EntityId(u64);

impl EntityId {
    /// Raw numeric id (for debugging / serialization later).
    #[inline]
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// Local-space transform: translation, rotation, and non-uniform scale.
///
/// Mutating fields through setters marks the transform dirty so callers can
/// refresh matrices via [`Transform::local_matrix`] or
/// [`World::update_world_matrices`].
#[derive(Clone, Copy, Debug)]
pub struct Transform {
    translation: Vec3,
    rotation: Quat,
    scale: Vec3,
    local_matrix: Mat4,
    world_matrix: Mat4,
    dirty: bool,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    /// Identity transform (origin, no rotation, unit scale).
    pub const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
        local_matrix: Mat4::IDENTITY,
        world_matrix: Mat4::IDENTITY,
        dirty: false,
    };

    /// Translation-only transform with identity rotation and unit scale.
    #[inline]
    pub fn from_translation(translation: Vec3) -> Self {
        Self::from_trs(translation, Quat::IDENTITY, Vec3::ONE)
    }

    /// Build from TRS components.
    #[inline]
    pub fn from_trs(translation: Vec3, rotation: Quat, scale: Vec3) -> Self {
        let mut t = Self {
            translation,
            rotation,
            scale,
            local_matrix: Mat4::IDENTITY,
            world_matrix: Mat4::IDENTITY,
            dirty: true,
        };
        t.rebuild_local_matrix();
        t.world_matrix = t.local_matrix;
        t
    }

    #[inline]
    pub fn translation(&self) -> Vec3 {
        self.translation
    }

    #[inline]
    pub fn rotation(&self) -> Quat {
        self.rotation
    }

    #[inline]
    pub fn scale(&self) -> Vec3 {
        self.scale
    }

    /// Whether the cached local matrix is out of date.
    #[inline]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Cached local matrix (`T * R * S`). Rebuilds if dirty.
    #[inline]
    pub fn local_matrix(&mut self) -> Mat4 {
        if self.dirty {
            self.rebuild_local_matrix();
        }
        self.local_matrix
    }

    /// Cached local matrix without requiring `&mut`. Returns the last computed
    /// matrix; call [`Self::local_matrix`] or [`World::update_world_matrices`]
    /// first if the transform may be dirty.
    #[inline]
    pub fn local_matrix_cached(&self) -> Mat4 {
        self.local_matrix
    }

    /// Cached world matrix (`parent_world * local`). Updated by
    /// [`World::update_world_matrices`].
    #[inline]
    pub fn world_matrix_cached(&self) -> Mat4 {
        self.world_matrix
    }

    /// Force-rebuild the local matrix and clear the dirty flag.
    #[inline]
    pub fn update_local_matrix(&mut self) {
        if self.dirty {
            self.rebuild_local_matrix();
        }
    }

    #[inline]
    pub fn set_translation(&mut self, translation: Vec3) {
        self.translation = translation;
        self.dirty = true;
    }

    #[inline]
    pub fn set_rotation(&mut self, rotation: Quat) {
        self.rotation = rotation;
        self.dirty = true;
    }

    #[inline]
    pub fn set_scale(&mut self, scale: Vec3) {
        self.scale = scale;
        self.dirty = true;
    }

    /// Add to translation.
    #[inline]
    pub fn translate(&mut self, delta: Vec3) {
        self.translation += delta;
        self.dirty = true;
    }

    /// Rotate around the local Y axis by `radians`.
    #[inline]
    pub fn rotate_y(&mut self, radians: f32) {
        self.rotation = Quat::from_rotation_y(radians) * self.rotation;
        self.dirty = true;
    }

    /// Rotate around the local X axis by `radians`.
    #[inline]
    pub fn rotate_x(&mut self, radians: f32) {
        self.rotation = Quat::from_rotation_x(radians) * self.rotation;
        self.dirty = true;
    }

    /// Rotate around the local Z axis by `radians`.
    #[inline]
    pub fn rotate_z(&mut self, radians: f32) {
        self.rotation = Quat::from_rotation_z(radians) * self.rotation;
        self.dirty = true;
    }

    fn rebuild_local_matrix(&mut self) {
        self.local_matrix =
            Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation);
        self.dirty = false;
    }

    fn set_world_matrix(&mut self, world: Mat4) {
        self.world_matrix = world;
    }
}

/// A spawned entity with optional name, local transform, and hierarchy links.
#[derive(Clone, Debug)]
pub struct Entity {
    id: EntityId,
    name: Option<String>,
    pub transform: Transform,
    parent: Option<EntityId>,
    children: Vec<EntityId>,
}

impl Entity {
    #[inline]
    pub fn id(&self) -> EntityId {
        self.id
    }

    #[inline]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    #[inline]
    pub fn parent(&self) -> Option<EntityId> {
        self.parent
    }

    #[inline]
    pub fn children(&self) -> &[EntityId] {
        &self.children
    }

    /// Convenience: rotate around Y (radians). Marks transform dirty.
    #[inline]
    pub fn rotate_y(&mut self, radians: f32) {
        self.transform.rotate_y(radians);
    }
}

/// Entity store with optional name → id map and parent/child hierarchy.
#[derive(Debug, Default)]
pub struct World {
    next_id: u64,
    entities: HashMap<EntityId, Entity>,
    names: HashMap<String, EntityId>,
}

impl World {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of living entities.
    #[inline]
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// Spawn an unnamed entity with the given transform.
    pub fn spawn(&mut self, transform: Transform) -> EntityId {
        self.spawn_inner(None, transform)
    }

    /// Spawn a named entity. Panics if `name` is already in use.
    pub fn spawn_named(&mut self, name: impl Into<String>, transform: Transform) -> EntityId {
        let name = name.into();
        assert!(
            !self.names.contains_key(&name),
            "entity name `{name}` already exists"
        );
        self.spawn_inner(Some(name), transform)
    }

    /// Remove an entity by id. Children are orphaned (parent cleared).
    /// Returns `true` if it existed.
    ///
    /// At runtime through the Kerabit facade, prefer `Context::despawn` so GPU
    /// draw entries stay in sync — raw world despawn alone leaves orphans in
    /// the engine renderable map.
    pub fn despawn(&mut self, id: EntityId) -> bool {
        let Some(entity) = self.entities.remove(&id) else {
            return false;
        };
        if let Some(name) = entity.name {
            self.names.remove(&name);
        }
        if let Some(parent) = entity.parent {
            if let Some(p) = self.entities.get_mut(&parent) {
                p.children.retain(|c| *c != id);
            }
        }
        for child in entity.children {
            if let Some(c) = self.entities.get_mut(&child) {
                c.parent = None;
            }
        }
        true
    }

    /// Remove every entity and name. Entity ids are not reused (`next_id` kept).
    pub fn clear(&mut self) {
        self.entities.clear();
        self.names.clear();
    }

    /// Look up by name.
    pub fn get(&self, name: &str) -> Option<&Entity> {
        let id = *self.names.get(name)?;
        self.entities.get(&id)
    }

    /// Mutable look up by name.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Entity> {
        let id = *self.names.get(name)?;
        self.entities.get_mut(&id)
    }

    /// Mutable transform by name (convenience helper).
    pub fn transform_mut(&mut self, name: &str) -> Option<&mut Transform> {
        self.get_mut(name).map(|e| &mut e.transform)
    }

    /// Look up by id.
    pub fn get_by_id(&self, id: EntityId) -> Option<&Entity> {
        self.entities.get(&id)
    }

    /// Mutable look up by id.
    pub fn get_mut_by_id(&mut self, id: EntityId) -> Option<&mut Entity> {
        self.entities.get_mut(&id)
    }

    /// Mutable transform by id.
    pub fn transform_mut_by_id(&mut self, id: EntityId) -> Option<&mut Transform> {
        self.get_mut_by_id(id).map(|e| &mut e.transform)
    }

    /// Resolve a name to an [`EntityId`].
    pub fn id_of(&self, name: &str) -> Option<EntityId> {
        self.names.get(name).copied()
    }

    /// Parent of `id`, if any.
    pub fn parent_of(&self, id: EntityId) -> Option<EntityId> {
        self.entities.get(&id).and_then(|e| e.parent)
    }

    /// Children of `id`.
    pub fn children_of(&self, id: EntityId) -> &[EntityId] {
        self.entities
            .get(&id)
            .map(|e| e.children.as_slice())
            .unwrap_or(&[])
    }

    /// Attach `child` under `parent` (or detach if `parent` is `None`).
    ///
    /// Panics if either id is missing, if `child == parent`, or if the link
    /// would create a cycle.
    pub fn set_parent(&mut self, child: EntityId, parent: Option<EntityId>) {
        assert!(
            self.entities.contains_key(&child),
            "set_parent: unknown child {:?}",
            child
        );
        if let Some(p) = parent {
            assert!(
                self.entities.contains_key(&p),
                "set_parent: unknown parent {:?}",
                p
            );
            assert_ne!(child, p, "set_parent: entity cannot parent itself");
            assert!(
                !self.is_ancestor(child, p),
                "set_parent: cycle detected"
            );
        }

        let old_parent = self.entities.get(&child).and_then(|e| e.parent);
        if old_parent == parent {
            return;
        }

        if let Some(old) = old_parent {
            if let Some(p) = self.entities.get_mut(&old) {
                p.children.retain(|c| *c != child);
            }
        }

        if let Some(p) = parent {
            self.entities.get_mut(&p).unwrap().children.push(child);
        }

        self.entities.get_mut(&child).unwrap().parent = parent;
    }

    /// Attach named `child` under named `parent`. Returns `false` if either name is missing.
    pub fn attach(&mut self, child: &str, parent: &str) -> bool {
        let (Some(c), Some(p)) = (self.id_of(child), self.id_of(parent)) else {
            return false;
        };
        self.set_parent(c, Some(p));
        true
    }

    /// Detach named `child` from its parent. Returns `false` if the name is missing.
    pub fn detach(&mut self, child: &str) -> bool {
        let Some(c) = self.id_of(child) else {
            return false;
        };
        self.set_parent(c, None);
        true
    }

    /// Iterate all entities.
    pub fn iter(&self) -> impl Iterator<Item = &Entity> {
        self.entities.values()
    }

    /// Iterate all entities mutably.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Entity> {
        self.entities.values_mut()
    }

    /// Rebuild local matrices for every dirty transform.
    pub fn update_local_matrices(&mut self) {
        for entity in self.entities.values_mut() {
            entity.transform.update_local_matrix();
        }
    }

    /// Rebuild local matrices, then propagate world matrices parent → child.
    pub fn update_world_matrices(&mut self) {
        self.update_local_matrices();

        let ids: Vec<EntityId> = self.entities.keys().copied().collect();
        let mut depths: Vec<(u32, EntityId)> = ids
            .iter()
            .map(|&id| (self.hierarchy_depth(id), id))
            .collect();
        depths.sort_by_key(|(d, _)| *d);

        for (_, id) in depths {
            let local = self
                .entities
                .get(&id)
                .map(|e| e.transform.local_matrix_cached())
                .unwrap_or(Mat4::IDENTITY);
            let world = match self.entities.get(&id).and_then(|e| e.parent) {
                Some(pid) => {
                    let parent_world = self
                        .entities
                        .get(&pid)
                        .map(|e| e.transform.world_matrix_cached())
                        .unwrap_or(Mat4::IDENTITY);
                    parent_world * local
                }
                None => local,
            };
            if let Some(entity) = self.entities.get_mut(&id) {
                entity.transform.set_world_matrix(world);
            }
        }
    }

    fn is_ancestor(&self, ancestor: EntityId, mut node: EntityId) -> bool {
        let mut seen = HashSet::new();
        while let Some(p) = self.entities.get(&node).and_then(|e| e.parent) {
            if !seen.insert(p) {
                break;
            }
            if p == ancestor {
                return true;
            }
            node = p;
        }
        false
    }

    fn hierarchy_depth(&self, mut id: EntityId) -> u32 {
        let mut depth = 0u32;
        let mut seen = HashSet::new();
        while let Some(p) = self.entities.get(&id).and_then(|e| e.parent) {
            if !seen.insert(p) {
                break;
            }
            depth = depth.saturating_add(1);
            id = p;
        }
        depth
    }

    fn spawn_inner(&mut self, name: Option<String>, transform: Transform) -> EntityId {
        self.next_id = self.next_id.checked_add(1).expect("EntityId overflow");
        let id = EntityId(self.next_id);
        if let Some(ref n) = name {
            self.names.insert(n.clone(), id);
        }
        self.entities.insert(
            id,
            Entity {
                id,
                name,
                transform,
                parent: None,
                children: Vec::new(),
            },
        );
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kerabit_math::vec3;
    use std::f32::consts::{FRAC_PI_2, PI};

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    fn mat4_approx_eq(a: Mat4, b: Mat4) -> bool {
        a.to_cols_array()
            .iter()
            .zip(b.to_cols_array().iter())
            .all(|(x, y)| approx_eq(*x, *y))
    }

    #[test]
    fn identity_local_matrix() {
        let mut t = Transform::IDENTITY;
        assert!(!t.is_dirty());
        assert!(mat4_approx_eq(t.local_matrix(), Mat4::IDENTITY));
    }

    #[test]
    fn translation_marks_dirty_and_builds_matrix() {
        let mut t = Transform::IDENTITY;
        t.set_translation(vec3(1.0, 2.0, 3.0));
        assert!(t.is_dirty());
        let m = t.local_matrix();
        assert!(!t.is_dirty());
        let expected = Mat4::from_translation(vec3(1.0, 2.0, 3.0));
        assert!(mat4_approx_eq(m, expected));
    }

    #[test]
    fn scale_rotation_translation_compose() {
        let translation = vec3(2.0, 0.0, -1.0);
        let rotation = Quat::from_rotation_y(FRAC_PI_2);
        let scale = vec3(2.0, 3.0, 4.0);
        let mut t = Transform::from_trs(translation, rotation, scale);
        assert!(!t.is_dirty());
        let expected = Mat4::from_scale_rotation_translation(scale, rotation, translation);
        assert!(mat4_approx_eq(t.local_matrix(), expected));
    }

    #[test]
    fn rotate_y_applies_and_dirties() {
        let mut t = Transform::IDENTITY;
        t.rotate_y(PI);
        assert!(t.is_dirty());
        let m = t.local_matrix();
        // 180° around Y: +X → −X
        let p = m.transform_point3(vec3(1.0, 0.0, 0.0));
        assert!(approx_eq(p.x, -1.0));
        assert!(approx_eq(p.y, 0.0));
        assert!(approx_eq(p.z, 0.0));
    }

    #[test]
    fn from_translation_helper() {
        let mut t = Transform::from_translation(vec3(5.0, 0.0, 0.0));
        assert_eq!(t.translation(), vec3(5.0, 0.0, 0.0));
        assert!(!t.is_dirty());
        let col = t.local_matrix().w_axis;
        assert!(approx_eq(col.x, 5.0));
    }

    #[test]
    fn spawn_despawn_and_named_lookup() {
        let mut world = World::new();
        let a = world.spawn(Transform::from_translation(vec3(1.0, 0.0, 0.0)));
        let b = world.spawn_named("cube", Transform::from_translation(vec3(0.0, 2.0, 0.0)));

        assert_eq!(world.len(), 2);
        assert_eq!(world.id_of("cube"), Some(b));
        assert_eq!(
            world.get("cube").unwrap().transform.translation(),
            vec3(0.0, 2.0, 0.0)
        );
        assert!(world.get_by_id(a).is_some());

        world.get_mut("cube").unwrap().rotate_y(0.5);
        assert!(world.get("cube").unwrap().transform.is_dirty());

        assert!(world.despawn(b));
        assert!(world.get("cube").is_none());
        assert_eq!(world.len(), 1);
        assert!(!world.despawn(b));
    }

    #[test]
    fn clear_removes_all_entities() {
        let mut world = World::new();
        world.spawn_named("a", Transform::IDENTITY);
        world.spawn_named("b", Transform::IDENTITY);
        assert_eq!(world.len(), 2);
        world.clear();
        assert!(world.is_empty());
        assert!(world.get("a").is_none());
        world.spawn_named("c", Transform::IDENTITY);
        assert_eq!(world.len(), 1);
        assert!(world.get("c").is_some());
    }

    #[test]
    fn update_local_matrices_clears_dirty() {
        let mut world = World::new();
        world.spawn_named("a", Transform::IDENTITY);
        world
            .get_mut("a")
            .unwrap()
            .transform
            .translate(vec3(1.0, 0.0, 0.0));
        assert!(world.get("a").unwrap().transform.is_dirty());
        world.update_local_matrices();
        assert!(!world.get("a").unwrap().transform.is_dirty());
        let m = world.get("a").unwrap().transform.local_matrix_cached();
        assert!(mat4_approx_eq(m, Mat4::from_translation(vec3(1.0, 0.0, 0.0))));
    }

    #[test]
    #[should_panic(expected = "already exists")]
    fn duplicate_name_panics() {
        let mut world = World::new();
        world.spawn_named("cube", Transform::IDENTITY);
        world.spawn_named("cube", Transform::IDENTITY);
    }

    #[test]
    fn child_follows_parent_world_matrix() {
        let mut world = World::new();
        let parent = world.spawn_named("parent", Transform::from_translation(vec3(10.0, 0.0, 0.0)));
        let child = world.spawn_named("child", Transform::from_translation(vec3(1.0, 2.0, 0.0)));
        world.set_parent(child, Some(parent));

        world.update_world_matrices();
        let world_pos = world
            .get("child")
            .unwrap()
            .transform
            .world_matrix_cached()
            .transform_point3(Vec3::ZERO);
        assert!(approx_eq(world_pos.x, 11.0));
        assert!(approx_eq(world_pos.y, 2.0));

        world
            .transform_mut("parent")
            .unwrap()
            .translate(vec3(5.0, 0.0, 0.0));
        world.update_world_matrices();
        let world_pos = world
            .get("child")
            .unwrap()
            .transform
            .world_matrix_cached()
            .transform_point3(Vec3::ZERO);
        assert!(approx_eq(world_pos.x, 16.0));
    }

    #[test]
    fn attach_by_name_and_orphan_on_despawn() {
        let mut world = World::new();
        world.spawn_named("root", Transform::IDENTITY);
        world.spawn_named("leaf", Transform::from_translation(vec3(0.0, 1.0, 0.0)));
        assert!(world.attach("leaf", "root"));
        assert_eq!(world.parent_of(world.id_of("leaf").unwrap()), world.id_of("root"));

        let root = world.id_of("root").unwrap();
        world.despawn(root);
        assert!(world.get("leaf").unwrap().parent().is_none());
    }

    #[test]
    #[should_panic(expected = "cycle")]
    fn set_parent_rejects_cycle() {
        let mut world = World::new();
        let a = world.spawn_named("a", Transform::IDENTITY);
        let b = world.spawn_named("b", Transform::IDENTITY);
        world.set_parent(b, Some(a));
        world.set_parent(a, Some(b));
    }
}
