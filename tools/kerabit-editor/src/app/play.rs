//! Play as a child of this binary (`--play`), so this window keeps its selection.

use std::path::PathBuf;
use std::process::Command;

use super::EditorApp;

impl EditorApp {
    pub(super) fn is_playing(&self) -> bool {
        self.play_child.is_some()
    }

    /// Poll the play child; clear state when it exits (Escape / window close).
    pub(super) fn poll_play_child(&mut self) {
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
            format!("Play exited ({})", detail.unwrap_or_else(|| "error".into()))
        };
    }

    pub(super) fn stop_play(&mut self) {
        if let Some(mut child) = self.play_child.take() {
            let _ = child.kill();
            let _ = child.wait();
            self.finish_play(true, None);
        }
    }

    /// Launch play via child process. Dirty/unsaved scenes use a temp file so
    /// selection and edit state stay intact in this window (eframe + winit
    /// cannot share one event loop for true in-viewport play).
    pub(super) fn play_scene(&mut self) {
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

        self.spawn_play(exe, play_path, self.dirty || self.path.is_none());
    }

    pub(super) fn play_file(&mut self, path: PathBuf) {
        if self.is_playing() {
            self.status = "Already playing — Stop first".into();
            return;
        }
        let exe = match std::env::current_exe() {
            Ok(e) => e,
            Err(err) => {
                self.status = format!("Play failed: current_exe: {err}");
                return;
            }
        };
        self.play_temp_path = None;
        self.spawn_play(exe, path, false);
    }

    fn spawn_play(&mut self, exe: PathBuf, play_path: PathBuf, snapshot: bool) {
        match Command::new(&exe).arg("--play").arg(&play_path).spawn() {
            Ok(child) => {
                self.play_child = Some(child);
                let hint = if snapshot { "snapshot" } else { "saved scene" };
                self.status =
                    format!("Playing ({hint}) — Esc in play window or Stop; selection kept");
            }
            Err(err) => {
                if let Some(temp) = self.play_temp_path.take() {
                    let _ = std::fs::remove_file(temp);
                }
                self.status = format!("Play failed to launch: {err}");
            }
        }
    }
}
