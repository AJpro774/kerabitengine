//! Entity list, rename, and align.

use egui::Ui;

use crate::theme::{self, Palette};

use super::document::AlignAxis;
use super::EditorApp;

impl EditorApp {
    pub(super) fn ui_hierarchy(&mut self, ui: &mut Ui) {
        theme::title(ui, "Hierarchy");
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
        let show_align = self.selection.len() >= 2;
        let align_t =
            ui.ctx()
                .animate_bool_with_time(egui::Id::new("hierarchy_align"), show_align, 0.16);
        if align_t > 0.01 {
            ui.scope(|ui| {
                ui.set_opacity(align_t);
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
                let glow = self.motion.selection_glow();
                if selected && glow > 0.02 {
                    ui.painter().rect_stroke(
                        resp.rect,
                        4.0,
                        egui::Stroke::new(1.0_f32, Palette::SUN.gamma_multiply(glow)),
                        egui::StrokeKind::Inside,
                    );
                }
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
}
