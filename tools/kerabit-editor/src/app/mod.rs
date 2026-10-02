//! Editor shell: the in-memory document and the frame that lays the panels out.
//!
//! Scene commands live in `document`, paths in `paths`, the play child in `play`,
//! and the Juni buffer in `script`. Panels are `menu`, `hierarchy`, `inspector`,
//! `environment`, `mods`, and `status`. Theme, motion, and action sounds stay in
//! their own files.

mod document;
mod environment;
mod hierarchy;
mod inspector;
mod menu;
mod mods;
mod paths;
mod play;
mod script;
mod status;
mod widgets;

use std::path::PathBuf;
use std::process::Child;

use kerabit::{ModIndex, Scene};

use crate::assets::AssetBrowser;
use crate::feedback::{self, Cue, EditorSfx};
use crate::motion::Motion;
use crate::selection::Selection;
use crate::settings::EditorSettings;
use crate::undo::UndoStack;
use crate::validation;
use crate::viewport::Viewport;

use script::ScriptBind;

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
    /// Bottom Juni script panel visibility.
    script_open: bool,
    script_path: Option<PathBuf>,
    script_text: String,
    script_dirty: bool,
    script_error: Option<String>,
    script_bind: ScriptBind,
    assets: AssetBrowser,
    mods: ModIndex,
    mods_open: bool,
    motion: Motion,
    sfx: EditorSfx,
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
            script_open: true,
            script_path: None,
            script_text: String::new(),
            script_dirty: false,
            script_error: None,
            script_bind: ScriptBind::Scene,
            assets: AssetBrowser::new(),
            mods: ModIndex::discover(),
            mods_open: false,
            motion: Motion::default(),
            sfx: EditorSfx::new(),
        }
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
        let dt = ctx.input(|i| i.stable_dt);
        self.motion.tick(dt);
        self.poll_play_child();
        self.handle_shortcuts(ctx);
        self.persist_snap_if_needed();

        let status_before = self.status.clone();
        let selection_before = self.selection.as_slice().to_vec();

        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));

        let errors = validation::validate(&self.scene, self.scene_dir());

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            self.ui_menu(ui);
        });

        self.ui_mods_window(ctx);

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            self.ui_status(ui, &errors);
        });

        self.show_script_panel(ctx);

        let mut asset_action = None;
        self.assets.sync_root(self.project_root());
        egui::SidePanel::left("hierarchy")
            .default_width(280.0)
            .show(ctx, |ui| {
                ui.add_enabled_ui(!self.is_playing(), |ui| {
                    egui::TopBottomPanel::bottom("assets")
                        .resizable(true)
                        .default_height(240.0)
                        .min_height(120.0)
                        .show_inside(ui, |ui| {
                            asset_action = self.assets.ui(ui);
                        });
                    self.ui_hierarchy(ui);
                });
            });
        if let Some(action) = asset_action {
            self.apply_asset(action);
        }

        egui::SidePanel::right("inspector")
            .default_width(340.0)
            .show(ctx, |ui| {
                ui.add_enabled_ui(!self.is_playing(), |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        self.ui_inspector(ui);
                        ui.add_space(12.0);
                        ui.separator();
                        self.ui_environment(ui);
                    });
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
                self.motion.selection_glow(),
            );
            if dirty_flag {
                self.dirty = true;
            }
            if self.selection.primary() != prev_primary {
                self.sync_rename_buf();
                self.undo.end_gesture();
            }
        });

        let selection_changed = self.selection.as_slice() != selection_before.as_slice();
        if selection_changed {
            self.motion.nudge_selection();
        }
        let status_cue = if self.status != status_before {
            feedback::cue_for_status(&self.status)
        } else {
            None
        };
        if let Some(cue) = status_cue {
            self.sfx.play(cue);
            self.motion.nudge_status();
        } else if selection_changed && self.status == status_before {
            // Hierarchy picks do not rewrite the status line.
            self.sfx.play(Cue::Select);
        }
        self.sfx.maintain();

        // Play mode already polls. Flashes need extra frames; panel and window tweens repaint themselves.
        if self.is_playing() || self.motion.busy() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use kerabit::{Prefab, Scene};

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
