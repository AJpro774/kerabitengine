//! Bottom bar: dirty flag, path, play indicator, and scene issues.

use egui::Ui;

use crate::feedback::{self, Cue};
use crate::theme::{self, Palette};

use super::EditorApp;

impl EditorApp {
    pub(super) fn ui_status(&self, ui: &mut Ui, errors: &[String]) {
        theme::edge_rule(ui, false);
        let path = self
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(unsaved)".into());
        let dirty = if self.dirty { "modified" } else { "clean" };
        let playing = self.is_playing();
        let glow = self.motion.status_glow();
        let error_status = feedback::cue_for_status(&self.status) == Some(Cue::Error);
        ui.horizontal(|ui| {
            if playing {
                ui.colored_label(Palette::SUN, "● PLAYING");
                ui.separator();
            }
            ui.label(format!("{dirty}  ·  {path}"));
            ui.separator();
            ui.add_space(glow * 6.0);
            let rest = if error_status {
                Palette::ERROR
            } else if playing {
                Palette::SUN
            } else {
                Palette::INK
            };
            let hot = if error_status {
                Palette::ERROR
            } else {
                Palette::SUN
            };
            ui.colored_label(theme::mix(rest, hot, glow), &self.status);
            if !errors.is_empty() {
                ui.separator();
                ui.colored_label(Palette::ERROR, format!("{} issue(s)", errors.len()));
            }
        });
        if !errors.is_empty() {
            ui.separator();
            for err in errors.iter().take(6) {
                ui.colored_label(Palette::ERROR, err);
            }
            if errors.len() > 6 {
                ui.label(format!("…and {} more", errors.len() - 6));
            }
        }
    }
}
