//! Discovered mod packs: enable, scaffold, play, or open.

use std::path::PathBuf;

use egui::RichText;
use kerabit::{ModIndex, ModPack};

use super::EditorApp;

impl EditorApp {
    pub(super) fn ui_mods_window(&mut self, ctx: &egui::Context) {
        // Call every frame so egui can fade the window in and out (defaults on).
        let mut open = self.mods_open;
        egui::Window::new("Mods")
            .open(&mut open)
            .default_width(420.0)
            .default_height(360.0)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(
                        "Folders: ./mods  ·  ~/.kerabit/mods (%USERPROFILE%\\.kerabit\\mods)  ·  KERABIT_MODS (; on Windows)",
                    )
                    .small()
                    .weak(),
                );
                ui.horizontal(|ui| {
                    if ui.button("Rescan").clicked() {
                        self.mods = ModIndex::discover();
                    }
                    if ui.button("New pack…").clicked() {
                        let parent = std::env::current_dir()
                            .ok()
                            .map(|c| c.join("mods"))
                            .unwrap_or_else(|| PathBuf::from("mods"));
                        match ModIndex::scaffold(&parent, "my-mod", "My Mod") {
                            Ok(dir) => {
                                self.mods = ModIndex::discover();
                                self.status = format!("Created {}", dir.display());
                            }
                            Err(err) => self.status = format!("Scaffold failed: {err}"),
                        }
                    }
                });
                ui.separator();
                if self.mods.packs().is_empty() {
                    ui.label("No packs found. Clone a repo into ./mods or click New pack.");
                    return;
                }
                let packs: Vec<ModPack> = self.mods.packs().to_vec();
                let mut toggled = None;
                let mut play = None;
                let mut open_scene = None;
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for pack in &packs {
                        let id = pack.manifest.id.clone();
                        let mut on = self.mods.is_enabled(&id);
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                if ui.checkbox(&mut on, "").changed() {
                                    toggled = Some((id.clone(), on));
                                }
                                ui.strong(&pack.manifest.name);
                                ui.label(RichText::new(&id).weak().small());
                            });
                            if !pack.manifest.description.is_empty() {
                                ui.label(RichText::new(&pack.manifest.description).small());
                            }
                            ui.label(
                                RichText::new(format!(
                                    "{}  ·  game {}",
                                    pack.manifest.version, pack.manifest.game
                                ))
                                .small()
                                .weak(),
                            );
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled(
                                        pack.entry_scene().is_some(),
                                        egui::Button::new("Play"),
                                    )
                                    .clicked()
                                {
                                    play = pack.entry_scene();
                                }
                                if ui
                                    .add_enabled(
                                        pack.entry_scene().is_some(),
                                        egui::Button::new("Open"),
                                    )
                                    .clicked()
                                {
                                    open_scene = pack.entry_scene();
                                }
                            });
                        });
                    }
                });
                if let Some((id, on)) = toggled {
                    self.mods.set_enabled(&id, on);
                    if let Err(err) = self.mods.save_prefs() {
                        self.status = format!("Could not save mod prefs: {err}");
                    }
                }
                if let Some(path) = play {
                    self.play_file(path);
                }
                if let Some(path) = open_scene {
                    self.load_scene_file(path);
                }
            });
        self.mods_open = open;
    }
}
