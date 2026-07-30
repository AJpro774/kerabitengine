//! `.kerabit.json` scene save/load mirroring the public spawn API.

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
}

/// Current `.kerabit.json` format version.
pub const SCENE_VERSION: u32 = 1;

/// Authoring scene: entities, camera, light, clear/ambient — mirrors [`Kerabit`] spawn.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub clear_color: Color,
    pub ambient: Color,
    pub camera: SceneCamera,
    pub light: SceneLight,
    pub entities: Vec<SceneEntity>,
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
    pub material: SceneMaterial,
    pub at: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
    pub parent: Option<String>,
}

/// Mesh primitive or asset path (mirrors [`Mesh`] builders).
#[derive(Clone, Debug, PartialEq)]
pub enum SceneMesh {
    Cube,
    Plane { size: f32 },
    Obj { path: PathBuf },
    Gltf { path: PathBuf },
}

/// Material tint / roughness / optional albedo texture path.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneMaterial {
    pub color: Color,
    pub roughness: f32,
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
            entities: Vec::new(),
        }
    }
}

impl Scene {
    /// Load a `.kerabit.json` file from disk.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, SceneError> {
        let text = fs::read_to_string(path)?;
        Self::from_json(&text)
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
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), SceneError> {
        fs::write(path, self.to_json()?)?;
        Ok(())
    }

    /// Build a [`Kerabit`] window titled `title` from this scene.
    pub fn into_kerabit(self, title: impl Into<String>) -> Result<Kerabit, SceneError> {
        Kerabit::new(title).scene(self)
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
            .scale(self.scale);
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
        let mut m = Material::color(self.color).roughness(self.roughness);
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
        for entity in scene.build_entities()? {
            self = self.spawn(entity);
        }
        Ok(self)
    }

    /// Load `.kerabit.json` from `path` and apply it (see [`Kerabit::scene`]).
    pub fn load_scene(self, path: impl AsRef<Path>) -> Result<Self, SceneError> {
        self.scene(Scene::load(path)?)
    }
}

// --- Serde wire format -------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct SceneFile {
    version: u32,
    #[serde(default = "default_clear")]
    clear_color: [f32; 3],
    #[serde(default = "default_ambient")]
    ambient: [f32; 3],
    camera: CameraFile,
    light: LightFile,
    #[serde(default)]
    entities: Vec<EntityFile>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    texture: Option<String>,
}

fn default_roughness() -> f32 {
    0.5
}

impl Default for MaterialFile {
    fn default() -> Self {
        Self {
            color: default_white(),
            roughness: default_roughness(),
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
            entities: scene.entities.iter().map(EntityFile::from_scene).collect(),
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
            entities: self.entities.into_iter().map(EntityFile::into_scene).collect(),
        }
    }
}

impl EntityFile {
    fn from_scene(e: &SceneEntity) -> Self {
        Self {
            name: e.name.clone(),
            tags: e.tags.clone(),
            mesh: MeshFile::from_scene(&e.mesh),
            material: MaterialFile {
                color: e.material.color.to_rgb_array(),
                roughness: e.material.roughness,
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
        }
    }

    fn into_scene(self) -> SceneEntity {
        SceneEntity {
            name: self.name,
            tags: self.tags,
            mesh: self.mesh.into_scene(),
            material: SceneMaterial {
                color: color_from_rgb(self.material.color),
                roughness: self.material.roughness,
                texture: self.material.texture.map(PathBuf::from),
            },
            at: vec3_from_arr(self.at),
            rotation: quat_from_arr(self.rotation),
            scale: vec3_from_arr(self.scale),
            parent: self.parent,
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
            entities: vec![
                SceneEntity {
                    name: "ground".into(),
                    tags: vec!["ground".into()],
                    mesh: SceneMesh::Plane { size: 20.0 },
                    material: SceneMaterial {
                        color: Color::GRAY,
                        roughness: 0.9,
                        texture: None,
                    },
                    at: Vec3::ZERO,
                    rotation: Quat::IDENTITY,
                    scale: Vec3::ONE,
                    parent: None,
                },
                SceneEntity {
                    name: "box".into(),
                    tags: vec!["wall".into()],
                    mesh: SceneMesh::Cube,
                    material: SceneMaterial {
                        color: Color::ORANGE,
                        roughness: 0.35,
                        texture: None,
                    },
                    at: vec3(0.0, 0.5, 0.0),
                    rotation: Quat::IDENTITY,
                    scale: vec3(1.0, 2.0, 1.0),
                    parent: None,
                },
                SceneEntity {
                    name: "child".into(),
                    tags: Vec::new(),
                    mesh: SceneMesh::Cube,
                    material: SceneMaterial {
                        color: Color::WHITE,
                        roughness: 0.5,
                        texture: None,
                    },
                    at: vec3(1.0, 0.0, 0.0),
                    rotation: Quat::IDENTITY,
                    scale: Vec3::ONE,
                    parent: Some("box".into()),
                },
            ],
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
        let out = scene.to_json().unwrap();
        assert!(!out.contains("\"tags\""));
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
}
