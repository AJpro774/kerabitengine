//! Clear color, camera, sun, image-based lighting, and the scene Juni path.

use egui::Ui;
use kerabit::{map_script_path, Color, SceneCamera, SceneLight, Vec3};

use crate::assets::{file_row, FileRowEvent};
use crate::theme;

use super::script::ScriptBind;
use super::widgets::{color_to_rgb, vec3_drag};
use super::EditorApp;

impl EditorApp {
    pub(super) fn ui_environment(&mut self, ui: &mut Ui) {
        theme::title(ui, "Environment");
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
        theme::section(ui, "Camera");
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
        theme::section(ui, "Sun");
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

        ui.separator();
        theme::section(ui, "IBL environment");
        let hdr_label = self
            .scene
            .environment
            .as_ref()
            .map(|e| e.hdr.to_string_lossy().into_owned())
            .unwrap_or_default();
        match file_row(ui, &hdr_label, false) {
            FileRowEvent::Browse => {
                if let Some(path) = self.pick_in_project("HDR environment", "HDR", &["hdr"]) {
                    self.apply_hdr_path(path);
                    return;
                }
            }
            FileRowEvent::Clear => {
                self.push_undo_if_needed();
                self.scene.environment = None;
                self.undo.end_gesture();
                self.mark_dirty();
                return;
            }
            _ => {}
        }
        if self.scene.environment.is_some() {
            let mut intensity = self
                .scene
                .environment
                .as_ref()
                .map(|e| e.intensity)
                .unwrap_or(1.0);
            if ui
                .add(
                    egui::DragValue::new(&mut intensity)
                        .speed(0.05)
                        .range(0.0..=8.0)
                        .prefix("intensity "),
                )
                .changed()
            {
                if let Some(env) = &mut self.scene.environment {
                    env.intensity = intensity;
                }
                dirty = true;
            }
        }

        ui.separator();
        theme::section(ui, "Scene Juni script");
        let mut scene_script = map_script_path(&self.scene.extras).unwrap_or_default();
        match file_row(ui, &scene_script, true) {
            FileRowEvent::Browse => {
                if let Some(path) = self.pick_in_project("Juni script", "Juni", &["juni"]) {
                    self.script_bind = ScriptBind::Scene;
                    self.load_script_path(path);
                    self.bind_open_script();
                    return;
                }
            }
            FileRowEvent::Open => {
                self.script_bind = ScriptBind::Scene;
                self.load_script_path(self.resolve_stored(scene_script.trim()));
                return;
            }
            FileRowEvent::Clear => {
                scene_script.clear();
                dirty = true;
            }
            FileRowEvent::None => {}
        }
        if ui.button("New scene script…").clicked() {
            self.new_scene_script();
            return;
        }

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
            if scene_script.trim().is_empty() {
                self.scene.extras.remove("script");
            } else {
                self.scene.extras.insert(
                    "script".into(),
                    serde_json::Value::String(scene_script.trim().to_string()),
                );
            }
            self.mark_dirty();
        }
    }
}
