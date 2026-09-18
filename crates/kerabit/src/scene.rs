//! `.kerabit.json` scene save/load mirroring the public spawn API.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use kerabit_color::Color;
use kerabit_math::{vec3, Quat, Vec3};
use kerabit_render::{Camera, Light};
use serde::{Deserialize, Serialize};

use crate::entity::Entity;
use crate::material::Material;
use crate::mesh::Mesh;
use crate::Kerabit;
use kerabit_juni::ScriptRuntime;

/// Errors from loading or saving a [`.kerabit.json`](Scene) file.
#[derive(Debug, thiserror::Error)]
pub enum SceneError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported scene version {0} (expected {SCENE_VERSION})")]
    UnsupportedVersion(u32),
    #[error("asset error: {0}")]
    Asset(#[from] kerabit_assets::AssetError),
    #[error("spawn error: {0}")]
    Spawn(String),
    #[error("script error: {0}")]
    Script(String),
}

/// Current `.kerabit.json` format version.
///
/// Stays at **1** for additive fields (`tags`, `components`, `extras`). Bump only when
/// existing files would fail to load without a migration.
pub const SCENE_VERSION: u32 = 1;

/// Reserved JSON object for future typed scene / entity data (Summit M1+).
///
/// Omitted or `{}` in JSON; engines ignore unknown keys until a feature consumes them.
pub type SceneMap = serde_json::Map<String, serde_json::Value>;

/// Authoring scene: entities, camera, light, clear/ambient — mirrors [`Kerabit`] spawn.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub clear_color: Color,
    pub ambient: Color,
    pub camera: SceneCamera,
    pub light: SceneLight,
    /// Optional image-based lighting (3.0). `None` uses the procedural sky.
    pub environment: Option<SceneEnvironment>,
    pub entities: Vec<SceneEntity>,
    /// Reserved root-level component bag (future systems). Empty today.
    pub components: SceneMap,
    /// Reserved root-level extras bag (tooling / forward-compat). Empty today.
    pub extras: SceneMap,
}

/// Equirectangular `.hdr` environment for IBL (path relative to the scene file).
#[derive(Clone, Debug, PartialEq)]
pub struct SceneEnvironment {
    pub hdr: PathBuf,
    /// Radiance multiplier (default `1.0`).
    pub intensity: f32,
}

/// A coarser mesh used beyond `distance` world units from the camera.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneLod {
    pub mesh: SceneMesh,
    pub distance: f32,
}

/// Camera fields stored in a scene file.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneCamera {
    pub fov_y: f32,
    pub eye: Vec3,
    pub target: Vec3,
    pub near: f32,
    pub far: f32,
}

/// Directional sun stored in a scene file.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneLight {
    pub direction: Vec3,
    pub intensity: f32,
    pub color: Color,
}

/// One spawned entity in a scene file.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneEntity {
    pub name: String,
    /// Gameplay / authoring tags (e.g. Reach roles: `player`, `goal`, `ground`, `wall`, `hazard`).
    /// Optional in JSON (`[]` / omitted). Names remain labels; games should prefer tags.
    pub tags: Vec<String>,
    pub mesh: SceneMesh,
    /// Optional LOD chain (3.0), nearest first; the renderer picks by camera distance.
    pub lods: Vec<SceneLod>,
    pub material: SceneMaterial,
    pub at: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
    pub parent: Option<String>,
    /// Reserved per-entity component bag (future systems). Empty today; ignored at spawn.
    pub components: SceneMap,
    /// Reserved per-entity extras bag (tooling / forward-compat). Empty today.
    pub extras: SceneMap,
}

impl Scene {
    /// Script files attached via `extras.script` or `components.script`.
    ///
    /// Each item is `(relative path, optional entity name)`. A `None` entity is
    /// a scene-level script; otherwise `self_entity()` in the script is that entity.
    pub fn script_attachments(&self) -> Vec<(String, Option<String>)> {
        let mut out = Vec::new();
        if let Some(p) = map_script_path(&self.extras).or_else(|| map_script_path(&self.components))
        {
            out.push((p, None));
        }
        for e in &self.entities {
            if let Some(p) = map_script_path(&e.extras).or_else(|| map_script_path(&e.components)) {
                out.push((p, Some(e.name.clone())));
            }
        }
        out
    }
}

/// Read `"script"` from a reserved JSON bag.
pub fn map_script_path(map: &SceneMap) -> Option<String> {
    map.get("script")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Mesh primitive or asset path (mirrors [`Mesh`] builders).
#[derive(Clone, Debug, PartialEq)]
pub enum SceneMesh {
    Cube,
    Plane { size: f32 },
    Obj { path: PathBuf },
    Gltf { path: PathBuf },
}

/// Material tint / roughness / metallic / optional albedo texture path.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneMaterial {
    pub color: Color,
    pub roughness: f32,
    /// Metalness (`0` dielectric … `1` metal). Default `0` when omitted from JSON.
    pub metallic: f32,
    pub texture: Option<PathBuf>,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            clear_color: Color::rgb(0.08, 0.09, 0.12),
            ambient: Color::rgb(0.15, 0.16, 0.18),
            camera: SceneCamera {
                fov_y: 60.0,
                eye: vec3(5.0, 3.0, 7.0),
                target: Vec3::ZERO,
                near: 0.1,
                far: 100.0,
            },
            light: SceneLight {
                direction: vec3(-0.35, -1.0, -0.25),
                intensity: 1.2,
                color: Color::WHITE,
            },
            environment: None,
            entities: Vec::new(),
            components: SceneMap::new(),
            extras: SceneMap::new(),
        }
    }
}

impl Scene {
    /// Load a `.kerabit.json` file from disk.
    ///
    /// Relative mesh / texture paths are resolved against the scene file's
    /// directory so `build_entities` works regardless of the process CWD.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, SceneError> {
        let path = path.as_ref();
        let text = fs::read_to_string(path)?;
        let mut scene = Self::from_json(&text)?;
        if let Some(dir) = path.parent() {
            scene.rebase_relative_assets(dir);
        }
        Ok(scene)
    }

    /// Parse scene JSON text.
    pub fn from_json(text: &str) -> Result<Self, SceneError> {
        let file: SceneFile = serde_json::from_str(text)?;
        if file.version != SCENE_VERSION {
            return Err(SceneError::UnsupportedVersion(file.version));
        }
        Ok(file.into_scene())
    }

    /// Serialize to pretty-printed JSON.
    pub fn to_json(&self) -> Result<String, SceneError> {
        let file = SceneFile::from_scene(self);
        Ok(serde_json::to_string_pretty(&file)?)
    }

    /// Write a `.kerabit.json` file.
    ///
    /// Mesh / texture paths that live under the destination directory are
    /// written relative to it so load+save does not bake absolute paths.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), SceneError> {
        let path = path.as_ref();
        let mut scene = self.clone();
        if let Some(dir) = path.parent() {
            scene.relativize_assets(dir);
        }
        fs::write(path, scene.to_json()?)?;
        Ok(())
    }

    /// Build a [`Kerabit`] window titled `title` from this scene.
    pub fn into_kerabit(self, title: impl Into<String>) -> Result<Kerabit, SceneError> {
        Kerabit::new(title).scene(self)
    }

    /// Collect entities by index into a [`Prefab`] (for editor Save Prefab).
    pub fn prefab_from_indices(&self, indices: &[usize]) -> Prefab {
        let mut entities = Vec::with_capacity(indices.len());
        for &i in indices {
            if let Some(e) = self.entities.get(i) {
                entities.push(e.clone());
            }
        }
        Prefab { entities }
    }

    fn rebase_relative_assets(&mut self, dir: &Path) {
        for e in &mut self.entities {
            rebase_mesh(&mut e.mesh, Some(dir));
            for lod in &mut e.lods {
                rebase_mesh(&mut lod.mesh, Some(dir));
            }
            rebase_material(&mut e.material, Some(dir));
        }
        if let Some(env) = &mut self.environment {
            env.hdr = resolve_asset_path(Some(dir), &env.hdr);
        }
    }

    fn relativize_assets(&mut self, dir: &Path) {
        for e in &mut self.entities {
            relativize_mesh(&mut e.mesh, dir);
            for lod in &mut e.lods {
                relativize_mesh(&mut lod.mesh, dir);
            }
            relativize_material(&mut e.material, dir);
        }
        if let Some(env) = &mut self.environment {
            env.hdr = relativize_path(dir, &env.hdr);
        }
    }

    /// Convert scene entities into spawn descriptors (resolves asset paths).
    pub fn build_entities(&self) -> Result<Vec<Entity>, SceneError> {
        let mut out = Vec::with_capacity(self.entities.len());
        for e in &self.entities {
            out.push(e.to_entity()?);
        }
        Ok(out)
    }

    pub(crate) fn to_camera(&self) -> Camera {
        Camera::perspective(self.camera.fov_y)
            .look_at(self.camera.eye, self.camera.target)
            .near_far(self.camera.near, self.camera.far)
    }

    pub(crate) fn to_light(&self) -> Light {
        Light::sun(self.light.direction)
            .intensity(self.light.intensity)
            .color(self.light.color)
    }
}

/// Reusable entity group for editor instancing (`.kerabit.prefab.json`).
///
/// Same entity wire format as scenes (mesh, material, tags, transforms, parent,
/// components/extras). No camera/light — instance into a [`Scene`] via
/// [`Prefab::instantiate`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Prefab {
    pub entities: Vec<SceneEntity>,
}

impl Prefab {
    /// Load a `.kerabit.prefab.json` file from disk.
    ///
    /// Relative mesh / texture paths are resolved against the prefab file's
    /// directory so instancing works regardless of the process CWD.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, SceneError> {
        let path = path.as_ref();
        let text = fs::read_to_string(path)?;
        let mut prefab = Self::from_json(&text)?;
        if let Some(dir) = path.parent() {
            for e in &mut prefab.entities {
                rebase_mesh(&mut e.mesh, Some(dir));
                rebase_material(&mut e.material, Some(dir));
            }
        }
        Ok(prefab)
    }

    /// Parse prefab JSON text.
    pub fn from_json(text: &str) -> Result<Self, SceneError> {
        let file: PrefabFile = serde_json::from_str(text)?;
        if file.version != SCENE_VERSION {
            return Err(SceneError::UnsupportedVersion(file.version));
        }
        Ok(Prefab {
            entities: file.entities.into_iter().map(EntityFile::into_scene).collect(),
        })
    }

    /// Serialize to pretty-printed JSON.
    pub fn to_json(&self) -> Result<String, SceneError> {
        let file = PrefabFile {
            version: SCENE_VERSION,
            entities: self.entities.iter().map(EntityFile::from_scene).collect(),
        };
        Ok(serde_json::to_string_pretty(&file)?)
    }

    /// Write a `.kerabit.prefab.json` file.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), SceneError> {
        let path = path.as_ref();
        let mut prefab = self.clone();
        if let Some(dir) = path.parent() {
            for e in &mut prefab.entities {
                relativize_mesh(&mut e.mesh, dir);
                relativize_material(&mut e.material, dir);
            }
        }
        fs::write(path, prefab.to_json()?)?;
        Ok(())
    }

    /// Clone entities into `scene` with unique names; returns indices of new entities.
    ///
    /// Parent links among prefab members are remapped to the new names. Parents
    /// outside the prefab are cleared. Positions are offset by `offset`.
    pub fn instantiate(&self, scene: &mut Scene, offset: Vec3) -> Vec<usize> {
        if self.entities.is_empty() {
            return Vec::new();
        }

        let existing: Vec<String> = scene.entities.iter().map(|e| e.name.clone()).collect();
        let mut name_map: HashMap<String, String> = HashMap::new();
        for e in &self.entities {
            let new_name = unique_entity_name(&existing, &name_map, &e.name);
            name_map.insert(e.name.clone(), new_name);
        }

        let start = scene.entities.len();
        for e in &self.entities {
            let new_name = name_map.get(&e.name).cloned().unwrap_or_else(|| e.name.clone());
            let parent = e.parent.as_ref().and_then(|p| name_map.get(p).cloned());
            scene.entities.push(SceneEntity {
                name: new_name,
                tags: e.tags.clone(),
                mesh: e.mesh.clone(),
                lods: e.lods.clone(),
                material: e.material.clone(),
                at: e.at + offset,
                rotation: e.rotation,
                scale: e.scale,
                parent,
                components: e.components.clone(),
                extras: e.extras.clone(),
            });
        }
        (start..scene.entities.len()).collect()
    }
}

fn unique_entity_name(
    existing: &[String],
    pending: &HashMap<String, String>,
    base: &str,
) -> String {
    let taken = |candidate: &str| {
        existing.iter().any(|n| n == candidate)
            || pending.values().any(|n| n == candidate)
    };
    if !taken(base) {
        return base.to_string();
    }
    for i in 2..10_000 {
        let candidate = format!("{base}_{i}");
        if !taken(&candidate) {
            return candidate;
        }
    }
    format!("{base}_{}", existing.len() + pending.len() + 1)
}

impl SceneEntity {
    /// Returns true if this entity carries `tag` (exact string match).
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }

    fn to_entity(&self) -> Result<Entity, SceneError> {
        let (mesh, material_override) = self.mesh.resolve()?;
        let mut material = self.material.to_material()?;
        if let Some(m) = material_override {
            // glTF base material fills gaps when the scene material is default-ish.
            if self.material.texture.is_none() && self.material.color == Color::WHITE {
                material = m.roughness(self.material.roughness);
            }
        }
        let mut entity = Entity::new(self.name.clone())
            .mesh(mesh)
            .material(material)
            .at(self.at)
            .rotation(self.rotation)
            .scale(self.scale)
            .tags(self.tags.clone());
        for lod in &self.lods {
            let (lod_mesh, _) = lod.mesh.resolve()?;
            entity = entity.lod(lod_mesh, lod.distance);
        }
        if let Some(parent) = &self.parent {
            entity = entity.parent(parent.clone());
        }
        Ok(entity)
    }
}

impl SceneMesh {
    fn resolve(&self) -> Result<(Mesh, Option<Material>), SceneError> {
        match self {
            SceneMesh::Cube => Ok((Mesh::cube(), None)),
            SceneMesh::Plane { size } => Ok((Mesh::plane(*size), None)),
            SceneMesh::Obj { path } => Ok((Mesh::load_obj(path)?, None)),
            SceneMesh::Gltf { path } => {
                let (mesh, material) = crate::load_gltf(path)?;
                Ok((mesh, Some(material)))
            }
        }
    }
}

impl SceneMaterial {
    fn to_material(&self) -> Result<Material, SceneError> {
        let mut m = Material::color(self.color)
            .roughness(self.roughness)
            .metallic(self.metallic);
        if let Some(path) = &self.texture {
            let tex = crate::Texture::load_png(path)?;
            m = m.with_texture(tex);
        }
        Ok(m)
    }
}

impl Kerabit {
    /// Apply a loaded [`Scene`] (clear, ambient, camera, light, entities).
    ///
    /// Later [`Kerabit::spawn`] calls still append entities.
    pub fn scene(mut self, scene: Scene) -> Result<Self, SceneError> {
        self = self
            .clear_color(scene.clear_color)
            .ambient(scene.ambient)
            .camera(scene.to_camera())
            .light(scene.to_light());
        if let Some(env) = &scene.environment {
            self = self.environment(env.hdr.clone(), env.intensity);
        }
        for entity in scene.build_entities()? {
            self = self.spawn(entity);
        }
        Ok(self)
    }

    /// Load `.kerabit.json` from `path` and apply it (see [`Kerabit::scene`]).
    ///
    /// Also compiles `extras.script` / `components.script` paths relative to
    /// the scene file (Juni, 3.0).
    pub fn load_scene(self, path: impl AsRef<Path>) -> Result<Self, SceneError> {
        let path = path.as_ref();
        let scene = Scene::load(path)?;
        let attachments = scene.script_attachments();
        let environment = scene
            .environment
            .as_ref()
            .map(|e| (resolve_relative(path.parent(), &e.hdr), e.intensity));
        let mut k = self.scene(scene)?;
        if let Some((hdr, intensity)) = environment {
            k = k.environment(hdr, intensity);
        }
        load_script_attachments(&mut k.scripts, attachments, path.parent())?;
        Ok(k)
    }
}

/// Join `rel` onto `base` unless it is already absolute.
pub(crate) fn resolve_relative(base: Option<&Path>, rel: &Path) -> PathBuf {
    if rel.is_absolute() {
        rel.to_path_buf()
    } else if let Some(dir) = base {
        dir.join(rel)
    } else {
        rel.to_path_buf()
    }
}

pub(crate) fn load_script_attachments(
    runtime: &mut ScriptRuntime,
    attachments: Vec<(String, Option<String>)>,
    base_dir: Option<&Path>,
) -> Result<(), SceneError> {
    if let Some(dir) = base_dir {
        runtime.set_base_dir(Some(dir.to_path_buf()));
    }
    runtime.clear();
    for (rel, entity) in attachments {
        let path = match runtime.base_dir() {
            Some(dir) => dir.join(&rel),
            None => PathBuf::from(&rel),
        };
        runtime
            .load_file(&path, entity)
            .map_err(|err| SceneError::Script(err.to_string()))?;
    }
    Ok(())
}

// --- Serde wire format -------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct PrefabFile {
    version: u32,
    #[serde(default)]
    entities: Vec<EntityFile>,
}

#[derive(Serialize, Deserialize)]
struct SceneFile {
    version: u32,
    #[serde(default = "default_clear")]
    clear_color: [f32; 3],
    #[serde(default = "default_ambient")]
    ambient: [f32; 3],
    camera: CameraFile,
    light: LightFile,
    /// Additive optional IBL environment (scene version 1); omitted when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    environment: Option<EnvironmentFile>,
    #[serde(default)]
    entities: Vec<EntityFile>,
    /// Additive reserved bag (scene version 1); omitted when empty.
    #[serde(default, skip_serializing_if = "SceneMap::is_empty")]
    components: SceneMap,
    /// Additive reserved bag (scene version 1); omitted when empty.
    #[serde(default, skip_serializing_if = "SceneMap::is_empty")]
    extras: SceneMap,
}

#[derive(Serialize, Deserialize)]
struct EnvironmentFile {
    hdr: String,
    #[serde(default = "default_env_intensity")]
    intensity: f32,
}

fn default_env_intensity() -> f32 {
    1.0
}

#[derive(Serialize, Deserialize)]
struct LodFile {
    mesh: MeshFile,
    distance: f32,
}

fn default_clear() -> [f32; 3] {
    [0.08, 0.09, 0.12]
}

fn default_ambient() -> [f32; 3] {
    [0.15, 0.16, 0.18]
}

#[derive(Serialize, Deserialize)]
struct CameraFile {
    fov_y: f32,
    eye: [f32; 3],
    target: [f32; 3],
    #[serde(default = "default_near")]
    near: f32,
    #[serde(default = "default_far")]
    far: f32,
}

fn default_near() -> f32 {
    0.1
}

fn default_far() -> f32 {
    100.0
}

#[derive(Serialize, Deserialize)]
struct LightFile {
    direction: [f32; 3],
    #[serde(default = "default_intensity")]
    intensity: f32,
    #[serde(default = "default_white")]
    color: [f32; 3],
}

fn default_intensity() -> f32 {
    1.0
}

fn default_white() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

#[derive(Serialize, Deserialize)]
struct EntityFile {
    name: String,
    /// Additive optional field (scene version 1); omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
    mesh: MeshFile,
    /// Additive optional LOD chain (scene version 1); omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    lods: Vec<LodFile>,
    #[serde(default)]
    material: MaterialFile,
    #[serde(default = "default_zero3")]
    at: [f32; 3],
    #[serde(default = "default_quat")]
    rotation: [f32; 4],
    #[serde(default = "default_one3")]
    scale: [f32; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parent: Option<String>,
    /// Additive reserved bag (scene version 1); omitted when empty.
    #[serde(default, skip_serializing_if = "SceneMap::is_empty")]
    components: SceneMap,
    /// Additive reserved bag (scene version 1); omitted when empty.
    #[serde(default, skip_serializing_if = "SceneMap::is_empty")]
    extras: SceneMap,
}

fn default_zero3() -> [f32; 3] {
    [0.0, 0.0, 0.0]
}

fn default_one3() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

fn default_quat() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum MeshFile {
    Cube,
    Plane { size: f32 },
    Obj { path: String },
    Gltf { path: String },
}

#[derive(Serialize, Deserialize)]
struct MaterialFile {
    #[serde(default = "default_white")]
    color: [f32; 3],
    #[serde(default = "default_roughness")]
    roughness: f32,
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    metallic: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    texture: Option<String>,
}

fn default_roughness() -> f32 {
    0.5
}

fn is_zero_f32(v: &f32) -> bool {
    *v == 0.0
}

impl Default for MaterialFile {
    fn default() -> Self {
        Self {
            color: default_white(),
            roughness: default_roughness(),
            metallic: 0.0,
            texture: None,
        }
    }
}

impl SceneFile {
    fn from_scene(scene: &Scene) -> Self {
        Self {
            version: SCENE_VERSION,
            clear_color: scene.clear_color.to_rgb_array(),
            ambient: scene.ambient.to_rgb_array(),
            camera: CameraFile {
                fov_y: scene.camera.fov_y,
                eye: vec3_to_arr(scene.camera.eye),
                target: vec3_to_arr(scene.camera.target),
                near: scene.camera.near,
                far: scene.camera.far,
            },
            light: LightFile {
                direction: vec3_to_arr(scene.light.direction),
                intensity: scene.light.intensity,
                color: scene.light.color.to_rgb_array(),
            },
            environment: scene.environment.as_ref().map(|e| EnvironmentFile {
                hdr: e.hdr.to_string_lossy().into_owned(),
                intensity: e.intensity,
            }),
            entities: scene.entities.iter().map(EntityFile::from_scene).collect(),
            components: scene.components.clone(),
            extras: scene.extras.clone(),
        }
    }

    fn into_scene(self) -> Scene {
        Scene {
            clear_color: color_from_rgb(self.clear_color),
            ambient: color_from_rgb(self.ambient),
            camera: SceneCamera {
                fov_y: self.camera.fov_y,
                eye: vec3_from_arr(self.camera.eye),
                target: vec3_from_arr(self.camera.target),
                near: self.camera.near,
                far: self.camera.far,
            },
            light: SceneLight {
                direction: vec3_from_arr(self.light.direction),
                intensity: self.light.intensity,
                color: color_from_rgb(self.light.color),
            },
            environment: self.environment.map(|e| SceneEnvironment {
                hdr: PathBuf::from(e.hdr),
                intensity: e.intensity,
            }),
            entities: self.entities.into_iter().map(EntityFile::into_scene).collect(),
            components: self.components,
            extras: self.extras,
        }
    }
}

impl EntityFile {
    fn from_scene(e: &SceneEntity) -> Self {
        Self {
            name: e.name.clone(),
            tags: e.tags.clone(),
            mesh: MeshFile::from_scene(&e.mesh),
            lods: e
                .lods
                .iter()
                .map(|l| LodFile {
                    mesh: MeshFile::from_scene(&l.mesh),
                    distance: l.distance,
                })
                .collect(),
            material: MaterialFile {
                color: e.material.color.to_rgb_array(),
                roughness: e.material.roughness,
                metallic: e.material.metallic,
                texture: e
                    .material
                    .texture
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned()),
            },
            at: vec3_to_arr(e.at),
            rotation: quat_to_arr(e.rotation),
            scale: vec3_to_arr(e.scale),
            parent: e.parent.clone(),
            components: e.components.clone(),
            extras: e.extras.clone(),
        }
    }

    fn into_scene(self) -> SceneEntity {
        SceneEntity {
            name: self.name,
            tags: self.tags,
            mesh: self.mesh.into_scene(),
            lods: self
                .lods
                .into_iter()
                .map(|l| SceneLod {
                    mesh: l.mesh.into_scene(),
                    distance: l.distance,
                })
                .collect(),
            material: SceneMaterial {
                color: color_from_rgb(self.material.color),
                roughness: self.material.roughness,
                metallic: self.material.metallic,
                texture: self.material.texture.map(PathBuf::from),
            },
            at: vec3_from_arr(self.at),
            rotation: quat_from_arr(self.rotation),
            scale: vec3_from_arr(self.scale),
            parent: self.parent,
            components: self.components,
            extras: self.extras,
        }
    }
}

impl MeshFile {
    fn from_scene(m: &SceneMesh) -> Self {
        match m {
            SceneMesh::Cube => MeshFile::Cube,
            SceneMesh::Plane { size } => MeshFile::Plane { size: *size },
            SceneMesh::Obj { path } => MeshFile::Obj {
                path: path.to_string_lossy().into_owned(),
            },
            SceneMesh::Gltf { path } => MeshFile::Gltf {
                path: path.to_string_lossy().into_owned(),
            },
        }
    }

    fn into_scene(self) -> SceneMesh {
        match self {
            MeshFile::Cube => SceneMesh::Cube,
            MeshFile::Plane { size } => SceneMesh::Plane { size },
            MeshFile::Obj { path } => SceneMesh::Obj {
                path: PathBuf::from(path),
            },
            MeshFile::Gltf { path } => SceneMesh::Gltf {
                path: PathBuf::from(path),
            },
        }
    }
}

#[inline]
fn color_from_rgb(rgb: [f32; 3]) -> Color {
    Color::rgb(rgb[0], rgb[1], rgb[2])
}

#[inline]
fn vec3_from_arr(v: [f32; 3]) -> Vec3 {
    Vec3::new(v[0], v[1], v[2])
}

#[inline]
fn vec3_to_arr(v: Vec3) -> [f32; 3] {
    [v.x, v.y, v.z]
}

#[inline]
fn quat_from_arr(q: [f32; 4]) -> Quat {
    Quat::from_xyzw(q[0], q[1], q[2], q[3])
}

#[inline]
fn quat_to_arr(q: Quat) -> [f32; 4] {
    [q.x, q.y, q.z, q.w]
}

/// Resolve a mesh / texture path against the scene or prefab directory.
fn resolve_asset_path(base: Option<&Path>, path: &Path) -> PathBuf {
    if path.as_os_str().is_empty() || path.is_absolute() {
        path.to_path_buf()
    } else if let Some(dir) = base {
        dir.join(path)
    } else {
        path.to_path_buf()
    }
}

fn rebase_mesh(mesh: &mut SceneMesh, base: Option<&Path>) {
    match mesh {
        SceneMesh::Obj { path } | SceneMesh::Gltf { path } => {
            *path = resolve_asset_path(base, path);
        }
        SceneMesh::Cube | SceneMesh::Plane { .. } => {}
    }
}

fn rebase_material(material: &mut SceneMaterial, base: Option<&Path>) {
    if let Some(path) = material.texture.take() {
        material.texture = Some(resolve_asset_path(base, &path));
    }
}

fn relativize_path(dir: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(dir).unwrap_or(path).to_path_buf()
}

fn relativize_mesh(mesh: &mut SceneMesh, dir: &Path) {
    match mesh {
        SceneMesh::Obj { path } | SceneMesh::Gltf { path } => {
            *path = relativize_path(dir, path);
        }
        SceneMesh::Cube | SceneMesh::Plane { .. } => {}
    }
}

fn relativize_material(material: &mut SceneMaterial, dir: &Path) {
    if let Some(path) = material.texture.take() {
        material.texture = Some(relativize_path(dir, &path));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_json_round_trip() {
        let scene = Scene {
            clear_color: Color::rgb(0.1, 0.2, 0.3),
            ambient: Color::rgb(0.2, 0.2, 0.25),
            camera: SceneCamera {
                fov_y: 55.0,
                eye: vec3(1.0, 2.0, 3.0),
                target: vec3(0.0, 0.5, 0.0),
                near: 0.05,
                far: 200.0,
            },
            light: SceneLight {
                direction: vec3(-0.2, -1.0, -0.1),
                intensity: 1.5,
                color: Color::rgb(1.0, 0.95, 0.9),
            },
            environment: None,
            entities: vec![
                SceneEntity {
                    name: "ground".into(),
                    tags: vec!["ground".into()],
                    mesh: SceneMesh::Plane { size: 20.0 },
                    lods: Vec::new(),
                    material: SceneMaterial {
                        color: Color::GRAY,
                        roughness: 0.9,
                        metallic: 0.0,
                        texture: None,
                    },
                    at: Vec3::ZERO,
                    rotation: Quat::IDENTITY,
                    scale: Vec3::ONE,
                    parent: None,
                    components: SceneMap::new(),
                    extras: SceneMap::new(),
                },
                SceneEntity {
                    name: "box".into(),
                    tags: vec!["wall".into()],
                    mesh: SceneMesh::Cube,
                    lods: Vec::new(),
                    material: SceneMaterial {
                        color: Color::ORANGE,
                        roughness: 0.35,
                        metallic: 0.0,
                        texture: None,
                    },
                    at: vec3(0.0, 0.5, 0.0),
                    rotation: Quat::IDENTITY,
                    scale: vec3(1.0, 2.0, 1.0),
                    parent: None,
                    components: SceneMap::new(),
                    extras: SceneMap::new(),
                },
                SceneEntity {
                    name: "child".into(),
                    tags: Vec::new(),
                    mesh: SceneMesh::Cube,
                    lods: Vec::new(),
                    material: SceneMaterial {
                        color: Color::WHITE,
                        roughness: 0.5,
                        metallic: 0.0,
                        texture: None,
                    },
                    at: vec3(1.0, 0.0, 0.0),
                    rotation: Quat::IDENTITY,
                    scale: Vec3::ONE,
                    parent: Some("box".into()),
                    components: SceneMap::new(),
                    extras: SceneMap::new(),
                },
            ],
            components: SceneMap::new(),
            extras: SceneMap::new(),
        };

        let json = scene.to_json().expect("serialize");
        assert!(json.contains("\"tags\""));
        let loaded = Scene::from_json(&json).expect("deserialize");
        assert_eq!(loaded, scene);
        assert!(loaded.entities[0].has_tag("ground"));
        assert!(loaded.entities[1].has_tag("wall"));
    }

    #[test]
    fn tags_default_when_omitted() {
        let scene = Scene::from_json(
            r#"{
              "version": 1,
              "camera": {"fov_y": 60, "eye": [0, 0, 5], "target": [0, 0, 0]},
              "light": {"direction": [0, -1, 0]},
              "entities": [
                {"name": "solo", "mesh": {"type": "cube"}}
              ]
            }"#,
        )
        .expect("load without tags");
        assert!(scene.entities[0].tags.is_empty());
        assert!(scene.entities[0].components.is_empty());
        assert!(scene.entities[0].extras.is_empty());
        assert!(scene.components.is_empty());
        assert!(scene.extras.is_empty());
        let out = scene.to_json().unwrap();
        assert!(!out.contains("\"tags\""));
        assert!(!out.contains("\"components\""));
        assert!(!out.contains("\"extras\""));
    }

    #[test]
    fn components_and_extras_round_trip() {
        let json = r#"{
          "version": 1,
          "camera": {"fov_y": 60, "eye": [0, 0, 5], "target": [0, 0, 0]},
          "light": {"direction": [0, -1, 0]},
          "extras": {"author": "summit-m0"},
          "components": {"future": true},
          "entities": [
            {
              "name": "solo",
              "mesh": {"type": "cube"},
              "components": {"rigid_body": {"mass": 1.0}},
              "extras": {"editor_locked": false}
            }
          ]
        }"#;
        let scene = Scene::from_json(json).expect("load with reserved bags");
        assert_eq!(
            scene.extras.get("author").and_then(|v| v.as_str()),
            Some("summit-m0")
        );
        assert_eq!(
            scene.components.get("future").and_then(|v| v.as_bool()),
            Some(true)
        );
        let e = &scene.entities[0];
        assert_eq!(
            e.components
                .get("rigid_body")
                .and_then(|v| v.get("mass"))
                .and_then(|v| v.as_f64()),
            Some(1.0)
        );
        assert_eq!(
            e.extras
                .get("editor_locked")
                .and_then(|v| v.as_bool()),
            Some(false)
        );
        let round = Scene::from_json(&scene.to_json().unwrap()).unwrap();
        assert_eq!(round, scene);
    }

    #[test]
    fn environment_and_lods_round_trip() {
        let json = r#"{
          "version": 1,
          "camera": {"fov_y": 60.0, "eye": [0,2,5], "target": [0,0,0], "near": 0.1, "far": 100.0},
          "light": {"direction": [0,-1,0], "intensity": 1.0, "color": [1,1,1]},
          "environment": {"hdr": "sky/dusk.hdr", "intensity": 1.5},
          "entities": [{
            "name": "rock",
            "mesh": {"type": "cube"},
            "lods": [
              {"mesh": {"type": "plane", "size": 1.0}, "distance": 25.0},
              {"mesh": {"type": "cube"}, "distance": 60.0}
            ]
          }, {"name": "plain", "mesh": {"type": "cube"}}]
        }"#;
        let scene = Scene::from_json(json).expect("parse");
        let env = scene.environment.as_ref().expect("environment");
        assert_eq!(env.hdr, PathBuf::from("sky/dusk.hdr"));
        assert!((env.intensity - 1.5).abs() < 1e-6);
        assert_eq!(scene.entities[0].lods.len(), 2);
        assert!((scene.entities[0].lods[0].distance - 25.0).abs() < 1e-6);
        assert!(scene.entities[1].lods.is_empty());

        let out = scene.to_json().expect("serialize");
        assert!(out.contains("\"environment\""));
        assert!(out.contains("\"lods\""));
        let again = Scene::from_json(&out).expect("reparse");
        assert_eq!(again, scene);

        // Omitted fields stay omitted so old files keep their shape.
        let plain = Scene::default().to_json().unwrap();
        assert!(!plain.contains("environment") && !plain.contains("lods"));

        // LODs become spawn descriptors with GPU-ready meshes.
        let entities = scene.build_entities().expect("entities");
        assert_eq!(entities[0].lods.len(), 2);
    }

    #[test]
    fn script_attachments_from_extras() {
        let json = r#"{
          "version": 1,
          "camera": {"fov_y": 60, "eye": [0, 0, 5], "target": [0, 0, 0]},
          "light": {"direction": [0, -1, 0]},
          "extras": {"script": "hello.juni"},
          "entities": [
            {
              "name": "cube",
              "mesh": {"type": "cube"},
              "extras": {"script": "spin.juni"}
            }
          ]
        }"#;
        let scene = Scene::from_json(json).expect("load scripts");
        let atts = scene.script_attachments();
        assert_eq!(atts.len(), 2);
        assert_eq!(atts[0], ("hello.juni".into(), None));
        assert_eq!(atts[1], ("spin.juni".into(), Some("cube".into())));
    }

    #[test]
    fn rejects_bad_version() {
        let err = Scene::from_json(r#"{"version":99,"camera":{"fov_y":60,"eye":[0,0,5],"target":[0,0,0]},"light":{"direction":[0,-1,0]},"entities":[]}"#)
            .unwrap_err();
        assert!(matches!(err, SceneError::UnsupportedVersion(99)));
    }

    #[test]
    fn loads_checked_in_mini_game_scene() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/scenes/mini_game.kerabit.json");
        let scene = Scene::load(&path).expect("load mini_game.kerabit.json");
        assert_eq!(scene.entities.len(), 7);
        assert!(scene.entities.iter().any(|e| e.name == "player"));
        assert!(scene.entities.iter().any(|e| e.name == "goal"));
        let round = Scene::from_json(&scene.to_json().unwrap()).unwrap();
        assert_eq!(round, scene);
        let entities = scene.build_entities().expect("build spawn descriptors");
        assert_eq!(entities.len(), 7);
    }

    #[test]
    fn load_resolves_obj_path_relative_to_scene_file() {
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../kerabit-assets/fixtures");
        let tmp = std::env::temp_dir().join(format!(
            "kerabit-scene-rel-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).expect("temp scene dir");
        fs::copy(fixtures.join("box.obj"), tmp.join("box.obj")).expect("copy obj");
        let scene_path = tmp.join("rel.kerabit.json");
        fs::write(
            &scene_path,
            r#"{
              "version": 1,
              "camera": {"fov_y": 60, "eye": [0, 0, 5], "target": [0, 0, 0]},
              "light": {"direction": [0, -1, 0]},
              "entities": [
                {"name": "box", "mesh": {"type": "obj", "path": "box.obj"}}
              ]
            }"#,
        )
        .expect("write scene");

        let scene = Scene::load(&scene_path).expect("load scene next to obj");
        match &scene.entities[0].mesh {
            SceneMesh::Obj { path } => {
                assert!(
                    path.is_absolute() && path.ends_with("box.obj"),
                    "expected scene-dir join, got {}",
                    path.display()
                );
                assert!(path.is_file(), "resolved obj missing: {}", path.display());
            }
            other => panic!("expected obj mesh, got {other:?}"),
        }
        scene
            .build_entities()
            .expect("relative obj path must resolve against the scene directory");
        let saved = tmp.join("saved.kerabit.json");
        scene.save(&saved).expect("save");
        let written = fs::read_to_string(&saved).expect("read saved scene");
        assert!(
            written.contains("box.obj") && !written.contains(tmp.to_string_lossy().as_ref()),
            "save should keep a relative mesh path, got:\n{written}"
        );
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn prefab_round_trip_and_instantiate() {
        let prefab = Prefab {
            entities: vec![
                SceneEntity {
                    name: "hazard".into(),
                    tags: vec!["hazard".into()],
                    mesh: SceneMesh::Cube,
                    lods: Vec::new(),
                    material: SceneMaterial {
                        color: Color::rgb(0.9, 0.1, 0.2),
                        roughness: 0.6,
                        metallic: 0.0,
                        texture: None,
                    },
                    at: vec3(0.0, 0.45, 0.0),
                    rotation: Quat::IDENTITY,
                    scale: Vec3::ONE,
                    parent: None,
                    components: SceneMap::new(),
                    extras: SceneMap::new(),
                },
                SceneEntity {
                    name: "marker".into(),
                    tags: Vec::new(),
                    mesh: SceneMesh::Cube,
                    lods: Vec::new(),
                    material: SceneMaterial {
                        color: Color::WHITE,
                        roughness: 0.5,
                        metallic: 0.0,
                        texture: None,
                    },
                    at: vec3(1.0, 0.0, 0.0),
                    rotation: Quat::IDENTITY,
                    scale: Vec3::ONE,
                    parent: Some("hazard".into()),
                    components: SceneMap::new(),
                    extras: SceneMap::new(),
                },
            ],
        };
        let json = prefab.to_json().expect("serialize prefab");
        let loaded = Prefab::from_json(&json).expect("deserialize prefab");
        assert_eq!(loaded, prefab);

        let mut scene = Scene::default();
        scene.entities.push(SceneEntity {
            name: "hazard".into(),
            tags: Vec::new(),
            mesh: SceneMesh::Cube,
            lods: Vec::new(),
            material: SceneMaterial {
                color: Color::WHITE,
                roughness: 0.5,
                metallic: 0.0,
                texture: None,
            },
            at: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
            parent: None,
            components: SceneMap::new(),
            extras: SceneMap::new(),
        });
        let idxs = prefab.instantiate(&mut scene, vec3(2.0, 0.0, 0.0));
        assert_eq!(idxs.len(), 2);
        assert_eq!(scene.entities.len(), 3);
        assert_eq!(scene.entities[1].name, "hazard_2");
        assert_eq!(scene.entities[2].parent.as_deref(), Some("hazard_2"));
        assert_eq!(scene.entities[1].at, vec3(2.0, 0.45, 0.0));
    }
}
