//! Juni buffer: which bag Save writes back to, and the bottom panel that edits it.

use std::path::{Path, PathBuf};

use egui::{RichText, Ui};
use kerabit::{map_script_path, ScriptRuntime};

use crate::theme::{self, Palette};

use super::EditorApp;

/// Which extras bag the open Juni buffer writes back to on Save.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ScriptBind {
    Scene,
    Entity(String),
    Loose,
}

impl EditorApp {
    pub(super) fn clear_script_buffer(&mut self) {
        self.script_path = None;
        self.script_text.clear();
        self.script_dirty = false;
        self.script_error = None;
        self.script_bind = ScriptBind::Scene;
    }

    pub(super) fn infer_script_bind(&self, path: &Path) -> ScriptBind {
        let rel = self.stored_rel(path);
        if map_script_path(&self.scene.extras).as_deref() == Some(rel.as_str())
            || map_script_path(&self.scene.components).as_deref() == Some(rel.as_str())
        {
            return ScriptBind::Scene;
        }
        for e in &self.scene.entities {
            if map_script_path(&e.extras).as_deref() == Some(rel.as_str())
                || map_script_path(&e.components).as_deref() == Some(rel.as_str())
            {
                return ScriptBind::Entity(e.name.clone());
            }
        }
        ScriptBind::Loose
    }

    pub(super) fn sync_script_from_scene(&mut self) {
        let rel =
            map_script_path(&self.scene.extras).or_else(|| map_script_path(&self.scene.components));
        let Some(rel) = rel else {
            self.clear_script_buffer();
            return;
        };
        self.script_bind = ScriptBind::Scene;
        self.load_script_path(self.resolve_stored(&rel));
    }

    pub(super) fn load_script_path(&mut self, path: PathBuf) {
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                self.script_path = Some(path.clone());
                self.script_text = text;
                self.script_dirty = false;
                self.script_error = ScriptRuntime::check_source(&self.script_text)
                    .err()
                    .map(|e| e.to_string());
                self.script_open = true;
                self.status = format!("Opened script {}", path.display());
            }
            Err(err) => {
                self.script_path = Some(path.clone());
                self.script_text.clear();
                self.script_dirty = false;
                self.script_error = Some(format!("read failed: {err}"));
                self.script_open = true;
                self.status = format!("Script read failed: {err}");
            }
        }
    }

    pub(super) fn open_script_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Juni script", &["juni"])
            .set_title("Open .juni");
        if let Some(dir) = self.scene_dir() {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.pick_file() {
            self.script_bind = self.infer_script_bind(&path);
            self.load_script_path(path);
        }
    }

    pub(super) fn save_script(&mut self) {
        if self.script_path.is_some() {
            self.write_script_path();
        } else {
            self.save_script_as();
        }
    }

    pub(super) fn check_script_syntax(&mut self) {
        match ScriptRuntime::check_source(&self.script_text) {
            Ok(()) => {
                self.script_error = None;
                self.status = "Script checks OK (types + host API)".into();
            }
            Err(err) => {
                self.script_error = Some(err.to_string());
                self.status = format!("Script error: {err}");
            }
        }
    }

    pub(super) fn reload_script_from_disk(&mut self) {
        let Some(path) = self.script_path.clone() else {
            self.status = "No script path to reload".into();
            return;
        };
        self.load_script_path(path);
        self.status = "Reloaded script from disk".into();
    }

    pub(super) fn save_script_as(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Juni script", &["juni"])
            .set_file_name("scene.juni")
            .set_title("Save .juni");
        if let Some(dir) = self.scene_dir() {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.save_file() {
            self.script_path = Some(path);
            self.write_script_path();
            self.bind_open_script();
        }
    }

    fn write_script_path(&mut self) {
        let Some(path) = self.script_path.clone() else {
            return;
        };
        match std::fs::write(&path, &self.script_text) {
            Ok(()) => {
                self.script_dirty = false;
                self.script_error = ScriptRuntime::check_source(&self.script_text)
                    .err()
                    .map(|e| e.to_string());
                self.bind_open_script();
                self.status = format!("Saved script {}", path.display());
            }
            Err(err) => {
                self.status = format!("Script save failed: {err}");
            }
        }
    }

    pub(super) fn bind_open_script(&mut self) {
        let Some(script_path) = self.script_path.clone() else {
            return;
        };
        let rel = self.stored_rel(&script_path);
        match &self.script_bind {
            ScriptBind::Entity(name) => {
                let name = name.clone();
                if let Some(e) = self.scene.entities.iter_mut().find(|e| e.name == name) {
                    e.extras
                        .insert("script".into(), serde_json::Value::String(rel));
                    self.mark_dirty();
                }
            }
            ScriptBind::Scene => {
                self.scene
                    .extras
                    .insert("script".into(), serde_json::Value::String(rel));
                self.mark_dirty();
            }
            ScriptBind::Loose => {}
        }
    }

    pub(super) fn new_scene_script(&mut self) {
        self.script_text = concat!(
            "# `main` runs once when Play starts; `frame` runs every Play frame.\n",
            "state:\n",
            "    t: f32 = 0.0\n",
            "\n",
            "fn main() -> i32:\n",
            "    return 0\n",
            "\n",
            "fn frame(dt: f32) -> i32:\n",
            "    t = t + dt\n",
            "    if key_pressed(\"Escape\"):\n",
            "        quit()\n",
            "    return 0\n",
        )
        .into();
        self.script_path = None;
        self.script_dirty = true;
        self.script_error = None;
        self.script_bind = ScriptBind::Scene;
        self.script_open = true;
        self.status = "New scene script — Save to attach".into();
    }

    pub(super) fn new_entity_script(&mut self, name: &str) {
        self.script_text = format!(
            "# Entity `{name}` — `self_entity()` is this handle.\nfn main() -> i32:\n    return 0\n\nfn frame(dt: f32) -> i32:\n    return 0\n"
        );
        self.script_dirty = true;
        self.script_error = None;
        self.script_bind = ScriptBind::Entity(name.to_string());
        self.script_open = true;
        if let Some(dir) = self.scene_dir() {
            let mut path = dir.join(format!("{name}.juni"));
            let mut n = 2;
            while path.exists() {
                path = dir.join(format!("{name}_{n}.juni"));
                n += 1;
            }
            self.script_path = Some(path);
            self.status = format!("New script for `{name}` — Save to write the file");
        } else {
            self.script_path = None;
            self.status = format!("New script for `{name}` — Save the scene, then the script");
        }
    }

    /// Slide the Juni panel open and closed without dropping the user's resized height.
    ///
    /// The tween uses a separate panel id so the real panel's saved size stays put.
    pub(super) fn show_script_panel(&mut self, ctx: &egui::Context) {
        let t = ctx.animate_bool_with_time(
            egui::Id::new("script_panel_reveal"),
            self.script_open,
            0.18,
        );
        if t <= 0.0 {
            return;
        }
        let height_id = egui::Id::new("script_panel_full_height");
        if t >= 1.0 {
            let shown = egui::TopBottomPanel::bottom("script_panel")
                .resizable(true)
                .default_height(280.0)
                .min_height(160.0)
                .show(ctx, |ui| {
                    self.ui_script_panel(ui);
                });
            let height = shown.response.rect.height();
            ctx.data_mut(|d| d.insert_temp(height_id, height));
        } else {
            let full = ctx.data(|d| d.get_temp::<f32>(height_id)).unwrap_or(280.0);
            egui::TopBottomPanel::bottom("script_panel_tween")
                .resizable(false)
                .exact_height((full * t).max(1.0))
                .show(ctx, |ui| {
                    ui.set_opacity(t);
                    self.ui_script_panel(ui);
                });
        }
    }

    fn ui_script_panel(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            theme::inline_title(ui, "Juni");
            let bind = match &self.script_bind {
                ScriptBind::Scene => "scene".to_string(),
                ScriptBind::Entity(name) => format!("entity `{name}`"),
                ScriptBind::Loose => "unattached".to_string(),
            };
            ui.label(RichText::new(bind).weak());
            let label = self
                .script_path
                .as_ref()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .unwrap_or("(unsaved)");
            ui.label(RichText::new(label).strong());
            let lines = self.script_text.lines().count().max(1);
            ui.label(RichText::new(format!("{lines} lines")).weak().small());
            if self.script_dirty {
                ui.colored_label(Palette::CORAL, "modified");
            }
            if ui.button("New").clicked() {
                self.new_scene_script();
            }
            if ui.button("Open…").clicked() {
                self.open_script_dialog();
            }
            if ui.button("Save").clicked() {
                self.save_script();
            }
            if ui.button("Check").clicked() {
                self.check_script_syntax();
            }
            if ui.button("Reload").clicked() {
                self.reload_script_from_disk();
            }
            if self.script_bind == ScriptBind::Loose {
                if ui.button("Attach to scene").clicked() {
                    self.script_bind = ScriptBind::Scene;
                    self.bind_open_script();
                }
                if ui
                    .add_enabled(
                        self.selection.primary().is_some(),
                        egui::Button::new("Attach to entity"),
                    )
                    .clicked()
                {
                    if let Some(i) = self.selection.primary() {
                        if let Some(name) = self.scene.entities.get(i).map(|e| e.name.clone()) {
                            self.script_bind = ScriptBind::Entity(name);
                            self.bind_open_script();
                        }
                    }
                }
            }
            if ui.button("Hide").clicked() {
                self.script_open = false;
            }
        });
        if let Some(err) = &self.script_error {
            ui.colored_label(Palette::ERROR, err);
        }
        let mut text = std::mem::take(&mut self.script_text);
        let response = ui.add(
            egui::TextEdit::multiline(&mut text)
                .id_salt("juni_editor")
                .code_editor()
                .font(egui::FontId::monospace(14.0))
                .desired_width(f32::INFINITY)
                .desired_rows(16)
                .lock_focus(true),
        );
        self.script_text = text;
        if response.changed() {
            self.script_dirty = true;
            self.script_error = ScriptRuntime::check_source(&self.script_text)
                .err()
                .map(|e| e.to_string());
        }
    }
}
