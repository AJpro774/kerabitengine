//! Editor application state and egui panels.

use std::path::{Path, PathBuf};

use egui::{Color32, RichText, Ui};
use kerabit::{
    Color, Quat, Scene, SceneCamera, SceneEntity, SceneLight, SceneMaterial, SceneMesh, Vec3,
};

use crate::validation;
use crate::viewport::Viewport;

/// Mesh kind for the inspector combo box.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MeshKind {
    Cube,
    Plane,
    Obj,
    Gltf,
}

impl MeshKind {
    fn label(self) -> &'static str {
        match self {
            MeshKind::Cube => "cube",
            MeshKind::Plane => "plane",
            MeshKind::Obj => "obj",
            MeshKind::Gltf => "gltf",
        }
    }

    fn from_mesh(mesh: &SceneMesh) -> Self {
        match mesh {
            SceneMesh::Cube => MeshKind::Cube,
            SceneMesh::Plane { .. } => MeshKind::Plane,
            SceneMesh::Obj { .. } => MeshKind::Obj,
            SceneMesh::Gltf { .. } => MeshKind::Gltf,
        }
    }
}

/// In-memory editor document: a [`Scene`] plus path / dirty / selection.
pub struct EditorApp {
    scene: Scene,
    path: Option<PathBuf>,
    dirty: bool,
    selected: Option<usize>,
    status: String,
    rename_buf: String,
    viewport: Viewport,
}

impl EditorApp {
    pub fn new() -> Self {
        Self {
            scene: Scene::default(),
            path: None,
            dirty: false,
            selected: None,
            status: "New scene".into(),
            rename_buf: String::new(),
            viewport: Viewport::new(),
        }
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    fn scene_dir(&self) -> Option<&Path> {
        self.path.as_ref().and_then(|p| p.parent())
    }

    fn window_title(&self) -> String {
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("untitled");
        let star = if self.dirty { "*" } else { "" };
        format!("Kerabit Editor — {name}{star}")
    }

    fn new_scene(&mut self) {
        self.scene = Scene::default();
        self.path = None;
        self.dirty = false;
        self.selected = None;
        self.rename_buf.clear();
        self.status = "New scene".into();
    }

    fn open_scene(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Kerabit scene", &["json"])
            .set_title("Open .kerabit.json");
        if let Some(dir) = default_levels_dir() {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.pick_file() {
            match Scene::load(&path) {
                Ok(scene) => {
                    self.scene = scene;
                    self.path = Some(path.clone());
                    self.dirty = false;
                    self.selected = None;
                    self.rename_buf.clear();
                    self.status = format!("Opened {}", path.display());
                }
                Err(err) => {
                    self.status = format!("Open failed: {err}");
                }
            }
        }
    }

    fn save_scene(&mut self) {
        if self.path.is_some() {
            self.write_current_path();
        } else {
            self.save_scene_as();
        }
    }

    fn save_scene_as(&mut self) {
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
        let existing: Vec<&str> = self.scene.entities.iter().map(|e| e.name.as_str()).collect();
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

    fn add_entity(&mut self) {
        let name = self.unique_name("entity");
        self.scene.entities.push(SceneEntity {
            name,
            tags: Vec::new(),
            mesh: SceneMesh::Cube,
            material: SceneMaterial {
                color: Color::ORANGE,
                roughness: 0.5,
                texture: None,
            },
            at: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
            parent: None,
        });
        self.selected = Some(self.scene.entities.len() - 1);
        if let Some(i) = self.selected {
            self.rename_buf = self.scene.entities[i].name.clone();
        }
        self.mark_dirty();
        self.status = "Added entity".into();
    }

    fn duplicate_selected(&mut self) {
        let Some(i) = self.selected else {
            return;
        };
        let Some(src) = self.scene.entities.get(i).cloned() else {
            return;
        };
        let mut dup = src;
        dup.name = self.unique_name(&format!("{}_copy", dup.name));
        self.scene.entities.push(dup);
        self.selected = Some(self.scene.entities.len() - 1);
        if let Some(j) = self.selected {
            self.rename_buf = self.scene.entities[j].name.clone();
        }
        self.mark_dirty();
        self.status = "Duplicated entity".into();
    }

    fn delete_selected(&mut self) {
        let Some(i) = self.selected else {
            return;
        };
        if i >= self.scene.entities.len() {
            self.selected = None;
            return;
        }
        let removed = self.scene.entities.remove(i);
        for e in &mut self.scene.entities {
            if e.parent.as_deref() == Some(removed.name.as_str()) {
                e.parent = None;
            }
        }
        self.selected = None;
        self.rename_buf.clear();
        self.mark_dirty();
        self.status = format!("Deleted \"{}\"", removed.name);
    }

    fn select(&mut self, index: Option<usize>) {
        self.selected = index;
        self.rename_buf = index
            .and_then(|i| self.scene.entities.get(i))
            .map(|e| e.name.clone())
            .unwrap_or_default();
    }

    fn apply_rename(&mut self) {
        let Some(i) = self.selected else {
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
        self.scene.entities[i].name = new_name.clone();
        for e in &mut self.scene.entities {
            if e.parent.as_deref() == Some(old_name.as_str()) {
                e.parent = Some(new_name.clone());
            }
        }
        self.mark_dirty();
        self.status = format!("Renamed \"{old_name}\" → \"{new_name}\"");
    }

    fn ui_menu(&mut self, ui: &mut Ui) {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui
                    .add(egui::Button::new("New").shortcut_text("Ctrl+N"))
                    .clicked()
                {
                    self.new_scene();
                    ui.close_menu();
                }
                if ui
                    .add(egui::Button::new("Open…").shortcut_text("Ctrl+O"))
                    .clicked()
                {
                    self.open_scene();
                    ui.close_menu();
                }
                if ui
                    .add(egui::Button::new("Save").shortcut_text("Ctrl+S"))
                    .clicked()
                {
                    self.save_scene();
                    ui.close_menu();
                }
                if ui
                    .add(egui::Button::new("Save As…").shortcut_text("Ctrl+Shift+S"))
                    .clicked()
                {
                    self.save_scene_as();
                    ui.close_menu();
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui.button("Add Entity").clicked() {
                    self.add_entity();
                    ui.close_menu();
                }
                let has_sel = self.selected.is_some();
                if ui
                    .add_enabled(has_sel, egui::Button::new("Duplicate"))
                    .clicked()
                {
                    self.duplicate_selected();
                    ui.close_menu();
                }
                if ui
                    .add_enabled(has_sel, egui::Button::new("Delete"))
                    .clicked()
                {
                    self.delete_selected();
                    ui.close_menu();
                }
            });
        });
    }

    fn ui_hierarchy(&mut self, ui: &mut Ui) {
        ui.heading("Hierarchy");
        ui.horizontal(|ui| {
            if ui.button("+ Add").clicked() {
                self.add_entity();
            }
            if ui
                .add_enabled(self.selected.is_some(), egui::Button::new("Duplicate"))
                .clicked()
            {
                self.duplicate_selected();
            }
            if ui
                .add_enabled(self.selected.is_some(), egui::Button::new("Delete"))
                .clicked()
            {
                self.delete_selected();
            }
        });
        ui.separator();

        if self.selected.is_some() {
            ui.label("Rename");
            let resp = ui.text_edit_singleline(&mut self.rename_buf);
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.apply_rename();
            }
            if ui.button("Apply rename").clicked() {
                self.apply_rename();
            }
        }

        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            let count = self.scene.entities.len();
            let mut clicked = None;
            for i in 0..count {
                let name = self.scene.entities[i].name.clone();
                let parent = self.scene.entities[i].parent.clone();
                let label = if let Some(p) = parent {
                    format!("{name}  → {p}")
                } else {
                    name
                };
                let selected = self.selected == Some(i);
                if ui.selectable_label(selected, label).clicked() {
                    clicked = Some(i);
                }
            }
            if let Some(i) = clicked {
                self.select(Some(i));
            }
        });
    }

    fn ui_inspector(&mut self, ui: &mut Ui) {
        ui.heading("Inspector");
        let Some(i) = self.selected else {
            ui.label("Select an entity in the hierarchy.");
            return;
        };
        if i >= self.scene.entities.len() {
            self.selected = None;
            return;
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
            SceneMesh::Obj { path } | SceneMesh::Gltf { path } => {
                path.to_string_lossy().into_owned()
            }
            _ => String::new(),
        };
        let mut color = color_to_rgb(self.scene.entities[i].material.color);
        let mut roughness = self.scene.entities[i].material.roughness;
        let mut texture = self.scene.entities[i]
            .material
            .texture
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut parent = self.scene.entities[i].parent.clone();
        let mut tags = self.scene.entities[i].tags.clone();

        let mut dirty = false;

        ui.label(RichText::new(&self.scene.entities[i].name).strong());
        ui.separator();

        ui.label("Tags / roles");
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
        ui.label("Transform");
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
        ui.label("Mesh");
        egui::ComboBox::from_id_salt("mesh_kind")
            .selected_text(mesh_kind.label())
            .show_ui(ui, |ui| {
                for kind in [
                    MeshKind::Cube,
                    MeshKind::Plane,
                    MeshKind::Obj,
                    MeshKind::Gltf,
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
                    .add(egui::DragValue::new(&mut plane_size).speed(0.1).prefix("size "))
                    .changed();
            }
            MeshKind::Obj | MeshKind::Gltf => {
                dirty |= ui.text_edit_singleline(&mut mesh_path).changed();
            }
            MeshKind::Cube => {}
        }

        ui.separator();
        ui.label("Material");
        dirty |= ui.color_edit_button_rgb(&mut color).changed();
        dirty |= ui
            .add(
                egui::DragValue::new(&mut roughness)
                    .speed(0.01)
                    .range(0.0..=1.0)
                    .prefix("roughness "),
            )
            .changed();
        ui.horizontal(|ui| {
            ui.label("Texture");
            dirty |= ui.text_edit_singleline(&mut texture).changed();
        });

        ui.separator();
        ui.label("Parent");
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

        if dirty {
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
            };
            self.scene.entities[i].material.color = Color::rgb(color[0], color[1], color[2]);
            self.scene.entities[i].material.roughness = roughness;
            self.scene.entities[i].material.texture = if texture.trim().is_empty() {
                None
            } else {
                Some(PathBuf::from(texture.trim()))
            };
            self.scene.entities[i].parent = parent;
            self.scene.entities[i].tags = tags;
            self.mark_dirty();
        }
    }

    fn ui_environment(&mut self, ui: &mut Ui) {
        ui.heading("Environment");
        let mut dirty = false;

        let mut clear = color_to_rgb(self.scene.clear_color);
        let mut ambient = color_to_rgb(self.scene.ambient);
        ui.horizontal(|ui| {
            ui.label("Clear");
            dirty |= ui.color_edit_button_rgb(&mut clear).changed();
            ui.label("Ambient");
            dirty |= ui.color_edit_button_rgb(&mut ambient).changed();
        });

        ui.separator();
        ui.label("Camera");
        let mut eye = [
            self.scene.camera.eye.x,
            self.scene.camera.eye.y,
            self.scene.camera.eye.z,
        ];
        let mut target = [
            self.scene.camera.target.x,
            self.scene.camera.target.y,
            self.scene.camera.target.z,
        ];
        let mut fov = self.scene.camera.fov_y;
        let mut near = self.scene.camera.near;
        let mut far = self.scene.camera.far;
        dirty |= vec3_drag(ui, "Eye", &mut eye);
        dirty |= vec3_drag(ui, "Target", &mut target);
        dirty |= ui
            .add(
                egui::DragValue::new(&mut fov)
                    .speed(0.5)
                    .range(1.0..=179.0)
                    .prefix("FOV° "),
            )
            .changed();
        ui.horizontal(|ui| {
            dirty |= ui
                .add(egui::DragValue::new(&mut near).speed(0.01).prefix("near "))
                .changed();
            dirty |= ui
                .add(egui::DragValue::new(&mut far).speed(1.0).prefix("far "))
                .changed();
        });

        ui.separator();
        ui.label("Sun");
        let mut dir = [
            self.scene.light.direction.x,
            self.scene.light.direction.y,
            self.scene.light.direction.z,
        ];
        let mut intensity = self.scene.light.intensity;
        let mut light_color = color_to_rgb(self.scene.light.color);
        dirty |= vec3_drag(ui, "Direction", &mut dir);
        dirty |= ui
            .add(
                egui::DragValue::new(&mut intensity)
                    .speed(0.05)
                    .range(0.0..=10.0)
                    .prefix("intensity "),
            )
            .changed();
        dirty |= ui.color_edit_button_rgb(&mut light_color).changed();

        if dirty {
            self.scene.clear_color = Color::rgb(clear[0], clear[1], clear[2]);
            self.scene.ambient = Color::rgb(ambient[0], ambient[1], ambient[2]);
            self.scene.camera = SceneCamera {
                fov_y: fov,
                eye: Vec3::new(eye[0], eye[1], eye[2]),
                target: Vec3::new(target[0], target[1], target[2]),
                near,
                far,
            };
            self.scene.light = SceneLight {
                direction: Vec3::new(dir[0], dir[1], dir[2]),
                intensity,
                color: Color::rgb(light_color[0], light_color[1], light_color[2]),
            };
            self.mark_dirty();
        }
    }

    fn ui_status(&self, ui: &mut Ui, errors: &[String]) {
        let path = self
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(unsaved)".into());
        let dirty = if self.dirty { "modified" } else { "clean" };
        ui.horizontal(|ui| {
            ui.label(format!("{dirty}  ·  {path}"));
            ui.separator();
            ui.label(&self.status);
            if !errors.is_empty() {
                ui.separator();
                ui.colored_label(
                    Color32::from_rgb(220, 80, 80),
                    format!("{} issue(s)", errors.len()),
                );
            }
        });
        if !errors.is_empty() {
            ui.separator();
            for err in errors.iter().take(6) {
                ui.colored_label(Color32::from_rgb(220, 80, 80), err);
            }
            if errors.len() > 6 {
                ui.label(format!("…and {} more", errors.len() - 6));
            }
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let mut open = false;
        let mut save = false;
        let mut save_as = false;
        let mut new = false;
        ctx.input(|i| {
            if i.modifiers.command && i.key_pressed(egui::Key::O) {
                open = true;
            }
            if i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::S) {
                save_as = true;
            } else if i.modifiers.command && i.key_pressed(egui::Key::S) {
                save = true;
            }
            if i.modifiers.command && i.key_pressed(egui::Key::N) {
                new = true;
            }
        });
        if open {
            self.open_scene();
        }
        if save_as {
            self.save_scene_as();
        } else if save {
            self.save_scene();
        }
        if new {
            self.new_scene();
        }
    }
}

impl eframe::App for EditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_shortcuts(ctx);

        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));

        let errors = validation::validate(&self.scene, self.scene_dir());

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            self.ui_menu(ui);
        });

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            self.ui_status(ui, &errors);
        });

        egui::SidePanel::left("hierarchy")
            .default_width(260.0)
            .show(ctx, |ui| {
                self.ui_hierarchy(ui);
            });

        egui::SidePanel::right("inspector")
            .default_width(320.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.ui_inspector(ui);
                    ui.add_space(12.0);
                    ui.separator();
                    self.ui_environment(ui);
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let scene_dir = self.scene_dir().map(|p| p.to_path_buf());
            let prev_selected = self.selected;
            let mut dirty_flag = false;
            self.viewport.show(
                ui,
                &mut self.scene,
                &mut self.selected,
                scene_dir.as_deref(),
                &mut || dirty_flag = true,
                &mut self.status,
            );
            if dirty_flag {
                self.mark_dirty();
            }
            if self.selected != prev_selected {
                self.rename_buf = self
                    .selected
                    .and_then(|i| self.scene.entities.get(i))
                    .map(|e| e.name.clone())
                    .unwrap_or_default();
            }
        });
    }
}

fn color_to_rgb(c: Color) -> [f32; 3] {
    [c.r, c.g, c.b]
}

fn vec3_drag(ui: &mut Ui, label: &str, v: &mut [f32; 3]) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(&mut v[0]).speed(0.05).prefix("X "))
            .changed()
            || ui
                .add(egui::DragValue::new(&mut v[1]).speed(0.05).prefix("Y "))
                .changed()
            || ui
                .add(egui::DragValue::new(&mut v[2]).speed(0.05).prefix("Z "))
                .changed()
    })
    .inner
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

#[cfg(test)]
mod tests {
    use super::*;
    use kerabit::Scene;

    #[test]
    fn reach_intro_round_trips_via_scene_api() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../games/reach/levels/01_intro.kerabit.json");
        let scene = Scene::load(&path).expect("load Reach intro");
        assert!(!scene.entities.is_empty());
        let json = scene.to_json().expect("serialize");
        let again = Scene::from_json(&json).expect("deserialize");
        assert_eq!(again, scene);
        let errors = validation::validate(&scene, path.parent());
        assert!(
            errors.is_empty(),
            "intro level should validate clean: {errors:?}"
        );
    }
}
