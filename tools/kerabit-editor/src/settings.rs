//! Persist editor preferences (gizmo snap) across sessions.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditorSettings {
    #[serde(default = "default_snap")]
    pub snap_enabled: bool,
    #[serde(default = "default_snap_size")]
    pub snap_size: f32,
}

fn default_snap() -> bool {
    false
}

fn default_snap_size() -> f32 {
    0.5
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            snap_enabled: default_snap(),
            snap_size: default_snap_size(),
        }
    }
}

impl EditorSettings {
    pub fn load() -> Self {
        let Some(path) = settings_path() else {
            return Self::default();
        };
        match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        let Some(path) = settings_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, text);
        }
    }
}

fn settings_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".kerabit").join("editor.json"))
}
