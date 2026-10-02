//! Top menu bar.

use egui::{RichText, Ui};
use kerabit::ModIndex;

use crate::theme::{self, Palette};

use super::document::AlignAxis;
use super::EditorApp;

impl EditorApp {
    pub(super) fn ui_menu(&mut self, ui: &mut Ui) {
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
            ui.menu_button("Script", |ui| {
                if ui.checkbox(&mut self.script_open, "Show panel").changed() {
                    ui.close_menu();
                }
                if ui.button("Open .juni…").clicked() {
                    self.open_script_dialog();
                    ui.close_menu();
                }
                if ui.button("Save script").clicked() {
                    self.save_script();
                    ui.close_menu();
                }
                if ui.button("Save script as…").clicked() {
                    self.save_script_as();
                    ui.close_menu();
                }
                if ui.button("New scene script").clicked() {
                    self.new_scene_script();
                    ui.close_menu();
                }
                if ui.button("Check syntax").clicked() {
                    self.check_script_syntax();
                    ui.close_menu();
                }
                if ui.button("Reload from disk").clicked() {
                    self.reload_script_from_disk();
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
                    .add_enabled(
                        has_sel,
                        egui::Button::new("Duplicate").shortcut_text("Ctrl+D"),
                    )
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
            ui.menu_button("Mods", |ui| {
                if ui.checkbox(&mut self.mods_open, "Show window").changed() {
                    if self.mods_open {
                        self.mods = ModIndex::discover();
                    }
                    ui.close_menu();
                }
                if ui.button("Rescan folders").clicked() {
                    self.mods = ModIndex::discover();
                    self.status = format!("{} pack(s) found", self.mods.packs().len());
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
            if playing {
                ui.colored_label(Palette::SUN, "▶ live");
            }
            if ui
                .add_enabled(!playing, egui::Button::new("▶ Play"))
                .on_hover_text("Play the scene as a game (Ctrl+P) — no builtin HUD or fly camera")
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
        theme::edge_rule(ui, true);
    }
}
