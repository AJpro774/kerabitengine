//! Paths stored on the scene, resolved against the scene file's directory.

use std::path::{Path, PathBuf};

use crate::assets::{project_root_from, rel_to};

use super::EditorApp;

impl EditorApp {
    pub(super) fn scene_dir(&self) -> Option<&Path> {
        self.path.as_ref().and_then(|p| p.parent())
    }

    pub(super) fn project_root(&self) -> Option<PathBuf> {
        project_root_from(self.scene_dir())
    }

    pub(super) fn stored_rel(&self, path: &Path) -> String {
        self.scene_dir()
            .map(|dir| rel_to(dir, path))
            .unwrap_or_else(|| {
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string_lossy().into_owned())
            })
    }

    pub(super) fn resolve_stored(&self, rel: &str) -> PathBuf {
        match self.scene_dir() {
            Some(dir) => dir.join(rel),
            None => PathBuf::from(rel),
        }
    }

    pub(super) fn pick_in_project(
        &self,
        title: &str,
        filter: &str,
        exts: &[&str],
    ) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new()
            .add_filter(filter, exts)
            .set_title(title);
        if let Some(dir) = self
            .project_root()
            .or_else(|| self.scene_dir().map(Path::to_path_buf))
        {
            dialog = dialog.set_directory(dir);
        }
        dialog.pick_file()
    }
}
