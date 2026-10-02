//! Selected entity: tags, transform, mesh, material, parent, and Juni path.

use std::path::PathBuf;

use egui::{RichText, Ui};
use kerabit::{map_script_path, Color, Quat, SceneMesh, Vec3};

use crate::assets::{file_row, FileRowEvent};
use crate::theme::{self, Palette};

use super::script::ScriptBind;
use super::widgets::color_to_rgb;
use super::EditorApp;

/// Mesh kind for the inspector combo box.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MeshKind {
    Cube,
    Plane,
    Obj,
    Gltf,
    Fbx,
}

impl MeshKind {
    fn label(self) -> &'static str {
        match self {
            MeshKind::Cube => "cube",
            MeshKind::Plane => "plane",
            MeshKind::Obj => "obj",
            MeshKind::Gltf => "gltf",
            MeshKind::Fbx => "fbx",
        }
    }

    fn from_mesh(mesh: &SceneMesh) -> Self {
        match mesh {
            SceneMesh::Cube => MeshKind::Cube,
            SceneMesh::Plane { .. } => MeshKind::Plane,
            SceneMesh::Obj { .. } => MeshKind::Obj,
            SceneMesh::Gltf { .. } => MeshKind::Gltf,
            SceneMesh::Fbx { .. } => MeshKind::Fbx,
        }
    }
}

impl EditorApp {
    pub(super) fn ui_inspector(&mut self, ui: &mut Ui) {
        theme::title(ui, "Inspector");
        let Some(i) = self.selection.primary() else {
            ui.label("Select an entity in the hierarchy.");
            ui.label(
                RichText::new("Shift+click for multi-select.")
                    .weak()
                    .small(),
            );
            return;
        };
        if i >= self.scene.entities.len() {
            self.selection.clear();
            return;
        }

        if self.selection.len() > 1 {
            ui.label(
                RichText::new(format!(
                    "Editing primary of {} selected",
                    self.selection.len()
                ))
                .weak(),
            );
        }

        let mut at = [
            self.scene.entities[i].at.x,
            self.scene.entities[i].at.y,
            self.scene.entities[i].at.z,
        ];
        let q = self.scene.entities[i].rotation;
        let mut rot = [q.x, q.y, q.z, q.w];
        let mut scale = [
            self.scene.entities[i].scale.x,
            self.scene.entities[i].scale.y,
            self.scene.entities[i].scale.z,
        ];
        let mut mesh_kind = MeshKind::from_mesh(&self.scene.entities[i].mesh);
        let mut plane_size = match &self.scene.entities[i].mesh {
            SceneMesh::Plane { size } => *size,
            _ => 10.0,
        };
        let mut mesh_path = match &self.scene.entities[i].mesh {
            SceneMesh::Obj { path } | SceneMesh::Gltf { path } | SceneMesh::Fbx { path } => {
                path.to_string_lossy().into_owned()
            }
            _ => String::new(),
        };
        let mut color = color_to_rgb(self.scene.entities[i].material.color);
        let mut roughness = self.scene.entities[i].material.roughness;
        let mut metallic = self.scene.entities[i].material.metallic;
        let mut texture = self.scene.entities[i]
            .material
            .texture
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut parent = self.scene.entities[i].parent.clone();
        let mut tags = self.scene.entities[i].tags.clone();
        let mut script_rel = map_script_path(&self.scene.entities[i].extras).unwrap_or_default();

        let mut dirty = false;

        ui.label(
            RichText::new(&self.scene.entities[i].name)
                .strong()
                .color(Palette::SUN),
        );
        ui.separator();

        theme::section(ui, "Tags / roles");
        ui.horizontal_wrapped(|ui| {
            for role in ["player", "goal", "ground", "wall", "hazard"] {
                let mut on = tags.iter().any(|t| t == role);
                if ui.checkbox(&mut on, role).changed() {
                    dirty = true;
                    if on {
                        if !tags.iter().any(|t| t == role) {
                            tags.push(role.to_string());
                        }
                    } else {
                        tags.retain(|t| t != role);
                    }
                }
            }
        });
        ui.label(
            RichText::new("Reach roles; names stay labels only.")
                .small()
                .weak(),
        );

        ui.separator();
        theme::section(ui, "Transform");
        dirty |= ui
            .horizontal(|ui| {
                ui.label("Position");
                ui.add(egui::DragValue::new(&mut at[0]).speed(0.05).prefix("X "))
                    .changed()
                    || ui
                        .add(egui::DragValue::new(&mut at[1]).speed(0.05).prefix("Y "))
                        .changed()
                    || ui
                        .add(egui::DragValue::new(&mut at[2]).speed(0.05).prefix("Z "))
                        .changed()
            })
            .inner;
        dirty |= ui
            .horizontal(|ui| {
                ui.label("Rotation (xyzw)");
                ui.add(egui::DragValue::new(&mut rot[0]).speed(0.01).prefix("x "))
                    .changed()
                    || ui
                        .add(egui::DragValue::new(&mut rot[1]).speed(0.01).prefix("y "))
                        .changed()
                    || ui
                        .add(egui::DragValue::new(&mut rot[2]).speed(0.01).prefix("z "))
                        .changed()
                    || ui
                        .add(egui::DragValue::new(&mut rot[3]).speed(0.01).prefix("w "))
                        .changed()
            })
            .inner;
        dirty |= ui
            .horizontal(|ui| {
                ui.label("Scale");
                ui.add(egui::DragValue::new(&mut scale[0]).speed(0.05).prefix("X "))
                    .changed()
                    || ui
                        .add(egui::DragValue::new(&mut scale[1]).speed(0.05).prefix("Y "))
                        .changed()
                    || ui
                        .add(egui::DragValue::new(&mut scale[2]).speed(0.05).prefix("Z "))
                        .changed()
            })
            .inner;

        ui.separator();
        theme::section(ui, "Mesh");
        egui::ComboBox::from_id_salt("mesh_kind")
            .selected_text(mesh_kind.label())
            .show_ui(ui, |ui| {
                for kind in [
                    MeshKind::Cube,
                    MeshKind::Plane,
                    MeshKind::Obj,
                    MeshKind::Gltf,
                    MeshKind::Fbx,
                ] {
                    if ui
                        .selectable_value(&mut mesh_kind, kind, kind.label())
                        .changed()
                    {
                        dirty = true;
                    }
                }
            });
        match mesh_kind {
            MeshKind::Plane => {
                dirty |= ui
                    .add(
                        egui::DragValue::new(&mut plane_size)
                            .speed(0.1)
                            .prefix("size "),
                    )
                    .changed();
            }
            MeshKind::Obj | MeshKind::Gltf | MeshKind::Fbx => match file_row(ui, &mesh_path, false)
            {
                FileRowEvent::Browse => {
                    if let Some(path) =
                        self.pick_in_project("Mesh", "Mesh", &["obj", "glb", "gltf", "fbx"])
                    {
                        self.apply_mesh_path(path);
                        return;
                    }
                }
                FileRowEvent::Clear => {
                    mesh_kind = MeshKind::Cube;
                    mesh_path.clear();
                    dirty = true;
                }
                _ => {}
            },
            MeshKind::Cube => {
                if ui.button("Browse mesh…").clicked() {
                    if let Some(path) =
                        self.pick_in_project("Mesh", "Mesh", &["obj", "glb", "gltf", "fbx"])
                    {
                        self.apply_mesh_path(path);
                        return;
                    }
                }
            }
        }

        ui.separator();
        theme::section(ui, "Material");
        dirty |= ui.color_edit_button_rgb(&mut color).changed();
        dirty |= ui
            .add(
                egui::DragValue::new(&mut roughness)
                    .speed(0.01)
                    .range(0.0..=1.0)
                    .prefix("roughness "),
            )
            .changed();
        dirty |= ui
            .add(
                egui::DragValue::new(&mut metallic)
                    .speed(0.01)
                    .range(0.0..=1.0)
                    .prefix("metallic "),
            )
            .changed();
        ui.label("Texture");
        match file_row(ui, &texture, false) {
            FileRowEvent::Browse => {
                if let Some(path) =
                    self.pick_in_project("Texture", "Image", &["png", "jpg", "jpeg"])
                {
                    self.apply_texture_path(path);
                    return;
                }
            }
            FileRowEvent::Clear => {
                texture.clear();
                dirty = true;
            }
            _ => {}
        }

        ui.separator();
        theme::section(ui, "Parent");
        let entity_names: Vec<String> = self
            .scene
            .entities
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, e)| e.name.clone())
            .collect();
        let parent_label = parent.as_deref().unwrap_or("(none)");
        egui::ComboBox::from_id_salt("parent")
            .selected_text(parent_label)
            .show_ui(ui, |ui| {
                if ui.selectable_label(parent.is_none(), "(none)").clicked() {
                    parent = None;
                    dirty = true;
                }
                for name in &entity_names {
                    let selected = parent.as_deref() == Some(name.as_str());
                    if ui.selectable_label(selected, name).clicked() {
                        parent = Some(name.clone());
                        dirty = true;
                    }
                }
            });

        ui.separator();
        theme::section(ui, "Juni script");
        match file_row(ui, &script_rel, true) {
            FileRowEvent::Browse => {
                if let Some(path) = self.pick_in_project("Juni script", "Juni", &["juni"]) {
                    let name = self.scene.entities[i].name.clone();
                    self.script_bind = ScriptBind::Entity(name);
                    self.load_script_path(path);
                    self.bind_open_script();
                    return;
                }
            }
            FileRowEvent::Open => {
                let name = self.scene.entities[i].name.clone();
                self.script_bind = ScriptBind::Entity(name);
                self.load_script_path(self.resolve_stored(script_rel.trim()));
                return;
            }
            FileRowEvent::Clear => {
                script_rel.clear();
                dirty = true;
            }
            FileRowEvent::None => {}
        }
        if ui.button("New script…").clicked() {
            let name = self.scene.entities[i].name.clone();
            self.new_entity_script(&name);
            return;
        }
        ui.label(
            RichText::new("Opens in the Juni panel. Paths stay relative to the scene file.")
                .small()
                .weak(),
        );

        if dirty {
            self.push_undo_if_needed();
            self.scene.entities[i].at = Vec3::new(at[0], at[1], at[2]);
            let mut q = Quat::from_xyzw(rot[0], rot[1], rot[2], rot[3]);
            if q.length_squared() > 1e-8 {
                q = q.normalize();
            } else {
                q = Quat::IDENTITY;
            }
            self.scene.entities[i].rotation = q;
            self.scene.entities[i].scale = Vec3::new(scale[0], scale[1], scale[2]);
            self.scene.entities[i].mesh = match mesh_kind {
                MeshKind::Cube => SceneMesh::Cube,
                MeshKind::Plane => SceneMesh::Plane { size: plane_size },
                MeshKind::Obj => SceneMesh::Obj {
                    path: PathBuf::from(mesh_path.trim()),
                },
                MeshKind::Gltf => SceneMesh::Gltf {
                    path: PathBuf::from(mesh_path.trim()),
                },
                MeshKind::Fbx => SceneMesh::Fbx {
                    path: PathBuf::from(mesh_path.trim()),
                },
            };
            self.scene.entities[i].material.color = Color::rgb(color[0], color[1], color[2]);
            self.scene.entities[i].material.roughness = roughness;
            self.scene.entities[i].material.metallic = metallic;
            self.scene.entities[i].material.texture = if texture.trim().is_empty() {
                None
            } else {
                Some(PathBuf::from(texture.trim()))
            };
            self.scene.entities[i].parent = parent;
            self.scene.entities[i].tags = tags;
            if script_rel.trim().is_empty() {
                self.scene.entities[i].extras.remove("script");
            } else {
                self.scene.entities[i].extras.insert(
                    "script".into(),
                    serde_json::Value::String(script_rel.trim().to_string()),
                );
            }
            self.mark_dirty();
        }

        // End continuous-edit gesture when pointer is released.
        if ui.input(|inp| inp.pointer.any_released() && !inp.pointer.any_down()) {
            self.undo.end_gesture();
        }
    }
}
