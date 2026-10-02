//! Scene commands: new, open, save, entities, prefabs, and asset assignment.

use std::path::PathBuf;

use kerabit::{
    Color, Prefab, Quat, Scene, SceneEntity, SceneEnvironment, SceneMaterial, SceneMesh, Vec3,
};

use crate::assets::{self, mesh_kind_from_path, AssetAction, MeshExt};

use super::EditorApp;

#[derive(Clone, Copy)]
pub(super) enum AlignAxis {
    X,
    Y,
    Z,
}

impl EditorApp {
    pub(super) fn apply_mesh_path(&mut self, path: PathBuf) {
        let Some(i) = self.selection.primary() else {
            self.status = "Select an entity to apply a mesh".into();
            return;
        };
        let Some(ext) = mesh_kind_from_path(&path) else {
            self.status = format!("Not a mesh: {}", path.display());
            return;
        };
        let rel = PathBuf::from(self.stored_rel(&path));
        self.push_undo_if_needed();
        self.scene.entities[i].mesh = match ext {
            MeshExt::Obj => SceneMesh::Obj { path: rel },
            MeshExt::Gltf => SceneMesh::Gltf { path: rel },
            MeshExt::Fbx => SceneMesh::Fbx { path: rel },
        };
        self.undo.end_gesture();
        self.mark_dirty();
        self.status = format!("Mesh → {}", assets::file_label(&self.stored_rel(&path)));
    }

    pub(super) fn apply_texture_path(&mut self, path: PathBuf) {
        let Some(i) = self.selection.primary() else {
            self.status = "Select an entity to apply a texture".into();
            return;
        };
        let rel = PathBuf::from(self.stored_rel(&path));
        self.push_undo_if_needed();
        self.scene.entities[i].material.texture = Some(rel);
        self.undo.end_gesture();
        self.mark_dirty();
        self.status = format!("Texture → {}", assets::file_label(&self.stored_rel(&path)));
    }

    pub(super) fn apply_hdr_path(&mut self, path: PathBuf) {
        let rel = PathBuf::from(self.stored_rel(&path));
        let intensity = self
            .scene
            .environment
            .as_ref()
            .map(|e| e.intensity)
            .unwrap_or(1.0);
        self.push_undo_if_needed();
        self.scene.environment = Some(SceneEnvironment {
            hdr: rel,
            intensity,
        });
        self.undo.end_gesture();
        self.mark_dirty();
        self.status = format!(
            "Environment → {}",
            assets::file_label(&self.stored_rel(&path))
        );
    }

    pub(super) fn apply_asset(&mut self, action: AssetAction) {
        match action {
            AssetAction::OpenScript(path) => {
                self.script_bind = self.infer_script_bind(&path);
                self.load_script_path(path);
            }
            AssetAction::AssignMesh(path) => self.apply_mesh_path(path),
            AssetAction::AssignTexture(path) => self.apply_texture_path(path),
            AssetAction::AssignHdr(path) => self.apply_hdr_path(path),
        }
    }

    pub(super) fn new_scene(&mut self) {
        self.undo.clear();
        self.scene = Scene::default();
        self.path = None;
        self.dirty = false;
        self.selection.clear();
        self.rename_buf.clear();
        self.clear_script_buffer();
        self.status = "New scene".into();
    }

    pub(super) fn open_scene(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Kerabit scene", &["json"])
            .set_title("Open .kerabit.json");
        if let Some(dir) = default_levels_dir() {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.pick_file() {
            self.load_scene_file(path);
        }
    }

    /// Replace the open document with the scene at `path`.
    pub(super) fn load_scene_file(&mut self, path: PathBuf) {
        match Scene::load(&path) {
            Ok(scene) => {
                self.undo.clear();
                self.scene = scene;
                self.path = Some(path.clone());
                self.dirty = false;
                self.selection.clear();
                self.rename_buf.clear();
                self.sync_script_from_scene();
                self.status = format!("Opened {}", path.display());
            }
            Err(err) => {
                self.status = format!("Open failed: {err}");
            }
        }
    }

    pub(super) fn save_scene(&mut self) {
        if self.script_dirty {
            self.save_script();
        }
        if self.path.is_some() {
            self.write_current_path();
        } else {
            self.save_scene_as();
        }
    }

    pub(super) fn save_scene_as(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Kerabit scene", &["json"])
            .set_file_name("untitled.kerabit.json")
            .set_title("Save As .kerabit.json");
        if let Some(dir) = default_levels_dir() {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.save_file() {
            self.path = Some(path);
            self.write_current_path();
        }
    }

    fn write_current_path(&mut self) {
        let Some(path) = self.path.clone() else {
            return;
        };
        match self.scene.save(&path) {
            Ok(()) => {
                self.dirty = false;
                self.status = format!("Saved {}", path.display());
            }
            Err(err) => {
                self.status = format!("Save failed: {err}");
            }
        }
    }

    fn unique_name(&self, base: &str) -> String {
        let existing: Vec<&str> = self
            .scene
            .entities
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        if !existing.contains(&base) {
            return base.to_string();
        }
        for i in 2..10_000 {
            let candidate = format!("{base}_{i}");
            if !existing.contains(&candidate.as_str()) {
                return candidate;
            }
        }
        format!("{base}_{}", existing.len() + 1)
    }

    pub(super) fn add_entity(&mut self) {
        self.push_undo();
        let name = self.unique_name("entity");
        self.scene.entities.push(SceneEntity {
            name,
            tags: Vec::new(),
            mesh: SceneMesh::Cube,
            lods: Vec::new(),
            material: SceneMaterial {
                color: Color::ORANGE,
                roughness: 0.5,
                metallic: 0.0,
                texture: None,
            },
            at: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
            parent: None,
            components: Default::default(),
            extras: Default::default(),
        });
        self.selection.set_one(self.scene.entities.len() - 1);
        self.sync_rename_buf();
        self.mark_dirty();
        self.status = "Added entity".into();
    }

    pub(super) fn duplicate_selected(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        self.push_undo();
        let indices: Vec<usize> = self.selection.as_slice().to_vec();
        let mut new_indices = Vec::new();
        for &i in &indices {
            let Some(src) = self.scene.entities.get(i).cloned() else {
                continue;
            };
            let mut dup = src;
            dup.name = self.unique_name(&format!("{}_copy", dup.name));
            // Offset slightly so copies are visible.
            dup.at.x += 0.5;
            self.scene.entities.push(dup);
            new_indices.push(self.scene.entities.len() - 1);
        }
        if !new_indices.is_empty() {
            self.selection.set_many(new_indices);
            self.sync_rename_buf();
            self.mark_dirty();
            self.status = format!(
                "Duplicated {} entit{}",
                indices.len(),
                if indices.len() == 1 { "y" } else { "ies" }
            );
        }
    }

    pub(super) fn delete_selected(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        self.push_undo();
        let to_remove = self.selection.sorted_desc();
        let mut removed_names = Vec::new();
        for i in to_remove {
            if i >= self.scene.entities.len() {
                continue;
            }
            let removed = self.scene.entities.remove(i);
            removed_names.push(removed.name.clone());
            for e in &mut self.scene.entities {
                if e.parent.as_deref() == Some(removed.name.as_str()) {
                    e.parent = None;
                }
            }
        }
        self.selection.clear();
        self.rename_buf.clear();
        self.mark_dirty();
        self.status = format!("Deleted {}", removed_names.join(", "));
    }

    pub(super) fn align_selected(&mut self, axis: AlignAxis) {
        let Some(primary) = self.selection.primary() else {
            return;
        };
        if self.selection.len() < 2 {
            self.status = "Align needs 2+ selected entities".into();
            return;
        }
        let Some(ref_at) = self.scene.entities.get(primary).map(|e| e.at) else {
            return;
        };
        self.push_undo();
        for &i in self.selection.as_slice() {
            if i == primary || i >= self.scene.entities.len() {
                continue;
            }
            match axis {
                AlignAxis::X => self.scene.entities[i].at.x = ref_at.x,
                AlignAxis::Y => self.scene.entities[i].at.y = ref_at.y,
                AlignAxis::Z => self.scene.entities[i].at.z = ref_at.z,
            }
        }
        self.mark_dirty();
        let axis_name = match axis {
            AlignAxis::X => "X",
            AlignAxis::Y => "Y",
            AlignAxis::Z => "Z",
        };
        self.status = format!("Aligned to primary {axis_name}");
    }

    pub(super) fn save_prefab(&mut self) {
        if self.selection.is_empty() {
            self.status = "Select entities to save as prefab".into();
            return;
        }
        let prefab = self.scene.prefab_from_indices(self.selection.as_slice());
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Kerabit prefab", &["json"])
            .set_file_name("untitled.kerabit.prefab.json")
            .set_title("Save Prefab");
        if let Some(dir) = default_prefabs_dir() {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.save_file() {
            let path = ensure_prefab_ext(path);
            match prefab.save(&path) {
                Ok(()) => {
                    self.status = format!(
                        "Saved prefab ({} entit{}) → {}",
                        prefab.entities.len(),
                        if prefab.entities.len() == 1 {
                            "y"
                        } else {
                            "ies"
                        },
                        path.display()
                    );
                }
                Err(err) => {
                    self.status = format!("Save prefab failed: {err}");
                }
            }
        }
    }

    pub(super) fn instance_prefab(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Kerabit prefab", &["json"])
            .set_title("Instance Prefab");
        if let Some(dir) = default_prefabs_dir() {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.pick_file() {
            match Prefab::load(&path) {
                Ok(prefab) => {
                    if prefab.entities.is_empty() {
                        self.status = "Prefab has no entities".into();
                        return;
                    }
                    self.push_undo();
                    let offset = self
                        .selection
                        .primary()
                        .and_then(|i| self.scene.entities.get(i))
                        .map(|e| e.at + Vec3::new(1.0, 0.0, 0.0))
                        .unwrap_or(Vec3::ZERO);
                    let idxs = prefab.instantiate(&mut self.scene, offset);
                    self.selection.set_many(idxs);
                    self.sync_rename_buf();
                    self.mark_dirty();
                    self.status = format!(
                        "Instanced {} from {}",
                        path.file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("prefab"),
                        path.display()
                    );
                }
                Err(err) => {
                    self.status = format!("Instance prefab failed: {err}");
                }
            }
        }
    }

    pub(super) fn apply_rename(&mut self) {
        let Some(i) = self.selection.primary() else {
            return;
        };
        let new_name = self.rename_buf.trim().to_string();
        if new_name.is_empty() {
            self.status = "Rename failed: empty name".into();
            return;
        }
        let old_name = self.scene.entities[i].name.clone();
        if new_name == old_name {
            return;
        }
        if self
            .scene
            .entities
            .iter()
            .enumerate()
            .any(|(j, e)| j != i && e.name == new_name)
        {
            self.status = format!("Rename failed: \"{new_name}\" already exists");
            return;
        }
        self.push_undo();
        self.scene.entities[i].name = new_name.clone();
        for e in &mut self.scene.entities {
            if e.parent.as_deref() == Some(old_name.as_str()) {
                e.parent = Some(new_name.clone());
            }
        }
        self.mark_dirty();
        self.status = format!("Renamed \"{old_name}\" → \"{new_name}\"");
    }
}

fn default_levels_dir() -> Option<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let reach = manifest.join("../../games/reach/levels");
    if reach.is_dir() {
        Some(reach.canonicalize().unwrap_or(reach))
    } else {
        None
    }
}

fn default_prefabs_dir() -> Option<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let prefabs = manifest.join("../../games/reach/prefabs");
    if prefabs.is_dir() {
        Some(prefabs.canonicalize().unwrap_or(prefabs))
    } else {
        default_levels_dir()
    }
}

fn ensure_prefab_ext(path: PathBuf) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled.kerabit.prefab.json");
    if name.ends_with(".kerabit.prefab.json") {
        return path;
    }
    if name.ends_with(".json") {
        let stem = name.trim_end_matches(".json");
        return path.with_file_name(format!("{stem}.kerabit.prefab.json"));
    }
    path.with_extension("kerabit.prefab.json")
}
