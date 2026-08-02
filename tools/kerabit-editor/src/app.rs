//! Editor application state and egui panels.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};

use egui::{Color32, RichText, Ui};
use kerabit::{
    Color, Prefab, Quat, Scene, SceneCamera, SceneEntity, SceneLight, SceneMaterial, SceneMesh,
    Vec3,
};

use crate::selection::Selection;
use crate::settings::EditorSettings;
use crate::undo::UndoStack;
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

#[derive(Clone, Copy)]
enum AlignAxis {
    X,
    Y,
    Z,
}

/// In-memory editor document: a [`Scene`] plus path / dirty / selection.
pub struct EditorApp {
    scene: Scene,
    path: Option<PathBuf>,
    dirty: bool,
    selection: Selection,
    status: String,
    rename_buf: String,
    viewport: Viewport,
    undo: UndoStack,
    settings: EditorSettings,
    /// Child `kerabit-editor --play <path>` process while play mode is active.
    play_child: Option<Child>,
    /// Selection names captured when Play starts (restored on return).
    play_selection_names: Vec<String>,
    /// Temp scene path used for dirty/unsaved Play (deleted when play ends).
    play_temp_path: Option<PathBuf>,
}

impl EditorApp {
    pub fn new() -> Self {
        let settings = EditorSettings::load();
        let mut viewport = Viewport::new();
        viewport.apply_snap_settings(settings.snap_enabled, settings.snap_size);
        Self {
            scene: Scene::default(),
            path: None,
            dirty: false,
            selection: Selection::default(),
            status: "New scene".into(),
            rename_buf: String::new(),
            viewport,
            undo: UndoStack::new(),
            settings,
            play_child: None,
            play_selection_names: Vec::new(),
            play_temp_path: None,
        }
    }

    fn is_playing(&self) -> bool {
        self.play_child.is_some()
    }

    fn sync_rename_buf(&mut self) {
        self.rename_buf = self
            .selection
            .primary()
            .and_then(|i| self.scene.entities.get(i))
            .map(|e| e.name.clone())
            .unwrap_or_default();
    }

    fn persist_snap_if_needed(&mut self) {
        if !self.viewport.snap_dirty {
            return;
        }
        self.viewport.snap_dirty = false;
        self.settings.snap_enabled = self.viewport.gizmo.snap;
        self.settings.snap_size = self.viewport.gizmo.snap_size;
        self.settings.save();
        self.status = format!(
            "Snap {} (size {:.2}) saved",
            if self.settings.snap_enabled {
                "on"
            } else {
                "off"
            },
            self.settings.snap_size
        );
    }

    /// Poll the play child; clear state when it exits (Escape / window close).
    fn poll_play_child(&mut self) {
        let Some(child) = self.play_child.as_mut() else {
            return;
        };
        match child.try_wait() {
            Ok(Some(status)) => {
                self.play_child = None;
                self.finish_play(status.success(), Some(status.to_string()));
            }
            Ok(None) => {}
            Err(err) => {
                self.play_child = None;
                self.finish_play(false, Some(format!("wait failed: {err}")));
            }
        }
    }

    fn finish_play(&mut self, success: bool, detail: Option<String>) {
        if let Some(temp) = self.play_temp_path.take() {
            let _ = std::fs::remove_file(&temp);
        }
        let names = std::mem::take(&mut self.play_selection_names);
        self.selection
            .restore_by_names(&names, &self.scene.entities);
        self.sync_rename_buf();
        self.status = if success {
            if names.is_empty() {
                "Play stopped — back to edit".into()
            } else {
                format!(
                    "Play stopped — selection restored ({} entit{})",
                    names.len(),
                    if names.len() == 1 { "y" } else { "ies" }
                )
            }
        } else {
            format!(
                "Play exited ({})",
                detail.unwrap_or_else(|| "error".into())
            )
        };
    }

    fn stop_play(&mut self) {
        if let Some(mut child) = self.play_child.take() {
            let _ = child.kill();
            let _ = child.wait();
            self.finish_play(true, None);
        }
    }

    /// Launch play via child process. Dirty/unsaved scenes use a temp file so
    /// selection and edit state stay intact in this window (eframe + winit
    /// cannot share one event loop for true in-viewport play).
    fn play_scene(&mut self) {
        if self.is_playing() {
            self.status = "Already playing — Stop first".into();
            return;
        }

        self.play_selection_names = self.selection.names(&self.scene.entities);

        let play_path = if self.dirty || self.path.is_none() {
            let temp = std::env::temp_dir().join(format!(
                "kerabit-play-{}-{}.kerabit.json",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0)
            ));
            if let Err(err) = self.scene.save(&temp) {
                self.status = format!("Play failed: write temp: {err}");
                return;
            }
            self.play_temp_path = Some(temp.clone());
            temp
        } else {
            self.play_temp_path = None;
            self.path.clone().expect("path checked")
        };

        let exe = match std::env::current_exe() {
            Ok(e) => e,
            Err(err) => {
                self.status = format!("Play failed: current_exe: {err}");
                if let Some(temp) = self.play_temp_path.take() {
                    let _ = std::fs::remove_file(temp);
                }
                return;
            }
        };

        match Command::new(&exe).arg("--play").arg(&play_path).spawn() {
            Ok(child) => {
                self.play_child = Some(child);
                let hint = if self.dirty || self.path.is_none() {
                    "unsaved snapshot"
                } else {
                    "saved scene"
                };
                self.status = format!(
                    "Playing ({hint}) — Esc in play window or Stop; selection kept"
                );
            }
            Err(err) => {
                if let Some(temp) = self.play_temp_path.take() {
                    let _ = std::fs::remove_file(temp);
                }
                self.status = format!("Play failed to launch: {err}");
            }
        }
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    fn push_undo(&mut self) {
        self.undo.push(&self.scene);
    }

    fn push_undo_if_needed(&mut self) {
        self.undo.push_if_needed(&self.scene);
    }

    fn do_undo(&mut self) {
        if let Some(prev) = self.undo.undo(&self.scene) {
            self.scene = prev;
            self.selection.retain_valid(self.scene.entities.len());
            self.sync_rename_buf();
            self.mark_dirty();
            self.status = "Undo".into();
        }
    }

    fn do_redo(&mut self) {
        if let Some(next) = self.undo.redo(&self.scene) {
            self.scene = next;
            self.selection.retain_valid(self.scene.entities.len());
            self.sync_rename_buf();
            self.mark_dirty();
            self.status = "Redo".into();
        }
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
        self.undo.clear();
        self.scene = Scene::default();
        self.path = None;
        self.dirty = false;
        self.selection.clear();
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
                    self.undo.clear();
                    self.scene = scene;
                    self.path = Some(path.clone());
                    self.dirty = false;
                    self.selection.clear();
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
        self.push_undo();
        let name = self.unique_name("entity");
        self.scene.entities.push(SceneEntity {
            name,
            tags: Vec::new(),
            mesh: SceneMesh::Cube,
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

    fn duplicate_selected(&mut self) {
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
            self.status = format!("Duplicated {} entit{}", indices.len(), if indices.len() == 1 { "y" } else { "ies" });
        }
    }

    fn delete_selected(&mut self) {
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

    fn align_selected(&mut self, axis: AlignAxis) {
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

    fn save_prefab(&mut self) {
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
                        if prefab.entities.len() == 1 { "y" } else { "ies" },
                        path.display()
                    );
                }
                Err(err) => {
                    self.status = format!("Save prefab failed: {err}");
                }
            }
        }
    }

    fn instance_prefab(&mut self) {
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

    fn apply_rename(&mut self) {
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
                ui.separator();
                if ui
                    .add_enabled(
                        !self.selection.is_empty(),
                        egui::Button::new("Save Prefab…"),
                    )
                    .clicked()
                {
                    self.save_prefab();
                    ui.close_menu();
                }
                if ui.button("Instance Prefab…").clicked() {
                    self.instance_prefab();
                    ui.close_menu();
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui
                    .add_enabled(
                        self.undo.can_undo(),
                        egui::Button::new("Undo").shortcut_text("Ctrl+Z"),
                    )
                    .clicked()
                {
                    self.do_undo();
                    ui.close_menu();
                }
                if ui
                    .add_enabled(
                        self.undo.can_redo(),
                        egui::Button::new("Redo").shortcut_text("Ctrl+Shift+Z"),
                    )
                    .clicked()
                {
                    self.do_redo();
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Add Entity").clicked() {
                    self.add_entity();
                    ui.close_menu();
                }
                let has_sel = !self.selection.is_empty();
                if ui
                    .add_enabled(has_sel, egui::Button::new("Duplicate").shortcut_text("Ctrl+D"))
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
                ui.separator();
                ui.label(RichText::new("Align to primary").weak().small());
                let can_align = self.selection.len() >= 2;
                if ui
                    .add_enabled(can_align, egui::Button::new("Align X"))
                    .clicked()
                {
                    self.align_selected(AlignAxis::X);
                    ui.close_menu();
                }
                if ui
                    .add_enabled(can_align, egui::Button::new("Align Y"))
                    .clicked()
                {
                    self.align_selected(AlignAxis::Y);
                    ui.close_menu();
                }
                if ui
                    .add_enabled(can_align, egui::Button::new("Align Z"))
                    .clicked()
                {
                    self.align_selected(AlignAxis::Z);
                    ui.close_menu();
                }
            });
            ui.menu_button("Play", |ui| {
                let playing = self.is_playing();
                if ui
                    .add_enabled(
                        !playing,
                        egui::Button::new("Play Scene").shortcut_text("Ctrl+P"),
                    )
                    .clicked()
                {
                    self.play_scene();
                    ui.close_menu();
                }
                if ui
                    .add_enabled(playing, egui::Button::new("Stop").shortcut_text("Ctrl+."))
                    .clicked()
                {
                    self.stop_play();
                    ui.close_menu();
                }
            });
            ui.separator();
            let playing = self.is_playing();
            if ui
                .add_enabled(!playing, egui::Button::new("▶ Play"))
                .on_hover_text("Play current scene (Ctrl+P) — dirty scenes use a temp snapshot")
                .clicked()
            {
                self.play_scene();
            }
            if ui
                .add_enabled(playing, egui::Button::new("■ Stop"))
                .on_hover_text("Stop play and return to edit with selection intact (Ctrl+.)")
                .clicked()
            {
                self.stop_play();
            }
        });
    }

    fn ui_hierarchy(&mut self, ui: &mut Ui) {
        ui.heading("Hierarchy");
        ui.horizontal(|ui| {
            if ui.button("+ Add").clicked() {
                self.add_entity();
            }
            if ui
                .add_enabled(!self.selection.is_empty(), egui::Button::new("Duplicate"))
                .clicked()
            {
                self.duplicate_selected();
            }
            if ui
                .add_enabled(!self.selection.is_empty(), egui::Button::new("Delete"))
                .clicked()
            {
                self.delete_selected();
            }
        });
        if self.selection.len() >= 2 {
            ui.horizontal(|ui| {
                ui.label("Align");
                if ui.button("X").clicked() {
                    self.align_selected(AlignAxis::X);
                }
                if ui.button("Y").clicked() {
                    self.align_selected(AlignAxis::Y);
                }
                if ui.button("Z").clicked() {
                    self.align_selected(AlignAxis::Z);
                }
            });
        }
        ui.separator();

        if self.selection.primary().is_some() {
            ui.label("Rename (primary)");
            let resp = ui.text_edit_singleline(&mut self.rename_buf);
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.apply_rename();
            }
            if ui.button("Apply rename").clicked() {
                self.apply_rename();
            }
        }

        ui.separator();
        if self.selection.len() > 1 {
            ui.label(format!("{} selected (Shift+click)", self.selection.len()));
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            let count = self.scene.entities.len();
            let mut clicked = None;
            let mut multi = false;
            for i in 0..count {
                let name = self.scene.entities[i].name.clone();
                let parent = self.scene.entities[i].parent.clone();
                let label = if let Some(p) = parent {
                    format!("{name}  → {p}")
                } else {
                    name
                };
                let selected = self.selection.contains(i);
                let resp = ui.selectable_label(selected, label);
                if resp.clicked() {
                    clicked = Some(i);
                    multi = ui.input(|inp| inp.modifiers.shift || inp.modifiers.command);
                }
            }
            if let Some(i) = clicked {
                self.undo.end_gesture();
                if multi {
                    self.selection.toggle(i);
                } else {
                    self.selection.set_one(i);
                }
                self.sync_rename_buf();
            }
        });
    }

    fn ui_inspector(&mut self, ui: &mut Ui) {
        ui.heading("Inspector");
        let Some(i) = self.selection.primary() else {
            ui.label("Select an entity in the hierarchy.");
            ui.label(RichText::new("Shift+click for multi-select.").weak().small());
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
            SceneMesh::Obj { path } | SceneMesh::Gltf { path } => {
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
        dirty |= ui
            .add(
                egui::DragValue::new(&mut metallic)
                    .speed(0.01)
                    .range(0.0..=1.0)
                    .prefix("metallic "),
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
            self.mark_dirty();
        }

        // End continuous-edit gesture when pointer is released.
        if ui.input(|inp| {
            inp.pointer.any_released() && !inp.pointer.any_down()
        }) {
            self.undo.end_gesture();
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
            self.push_undo_if_needed();
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
        let mut play = false;
        let mut stop = false;
        let mut undo = false;
        let mut redo = false;
        let mut duplicate = false;
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
            if i.modifiers.command && i.key_pressed(egui::Key::P) {
                play = true;
            }
            if i.modifiers.command && i.key_pressed(egui::Key::Period) {
                stop = true;
            }
            if i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::Z) {
                redo = true;
            } else if i.modifiers.command && i.key_pressed(egui::Key::Z) {
                undo = true;
            }
            if i.modifiers.command && i.key_pressed(egui::Key::Y) {
                redo = true;
            }
            if i.modifiers.command && i.key_pressed(egui::Key::D) {
                duplicate = true;
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
        if play {
            self.play_scene();
        }
        if stop {
            self.stop_play();
        }
        if undo {
            self.do_undo();
        }
        if redo {
            self.do_redo();
        }
        if duplicate {
            self.duplicate_selected();
        }
    }
}

impl eframe::App for EditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_play_child();
        self.handle_shortcuts(ctx);
        self.persist_snap_if_needed();

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
            let prev_primary = self.selection.primary();
            let mut dirty_flag = false;
            self.viewport.show(
                ui,
                &mut self.scene,
                &mut self.selection,
                scene_dir.as_deref(),
                &mut self.undo,
                &mut || dirty_flag = true,
                &mut self.status,
            );
            if dirty_flag {
                self.dirty = true;
            }
            if self.selection.primary() != prev_primary {
                self.sync_rename_buf();
                self.undo.end_gesture();
            }
        });

        // Keep polling while a play child is alive.
        if self.is_playing() {
            ctx.request_repaint();
        }
    }

    fn on_exit(&mut self) {
        self.stop_play();
        self.settings.snap_enabled = self.viewport.gizmo.snap;
        self.settings.snap_size = self.viewport.gizmo.snap_size;
        self.settings.save();
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

    #[test]
    fn hazard_prefab_loads() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../games/reach/prefabs/hazard_block.kerabit.prefab.json");
        let prefab = Prefab::load(&path).expect("load hazard prefab");
        assert_eq!(prefab.entities.len(), 1);
        assert!(prefab.entities[0].has_tag("hazard"));
    }
}
