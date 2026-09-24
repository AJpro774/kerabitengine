//! Community mods: a folder + [`mod.kerabit.json`](ModManifest).
//!
//! Discovery (later roots override earlier when resolving files):
//! 1. `./mods` (usually the repo root)
//! 2. `user_data_dir()/mods` (`~/.kerabit` or `%USERPROFILE%\.kerabit`)
//! 3. `KERABIT_MODS` (path-separator list)
//!
//! Enable/disable is stored in `~/.kerabit/mods.json`. A pack is on unless its
//! id is listed in `disabled`. Share mods by git clone into one of those roots.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Filename every pack must contain at its root.
pub const MOD_MANIFEST: &str = "mod.kerabit.json";

/// On-disk pack description (additive JSON, version 1).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    /// Target game id (`reach`, `surge`, `spark`, `*`).
    #[serde(default = "star")]
    pub game: String,
    /// Scene paths relative to the pack root. First is the Play entry.
    #[serde(default)]
    pub scenes: Vec<String>,
    #[serde(default)]
    pub homepage: String,
}

fn star() -> String {
    "*".into()
}

impl ModManifest {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let m: Self = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        if !valid_id(&m.id) {
            return Err(format!("{}: id `{}` must be kebab-case [a-z0-9-]", path.display(), m.id));
        }
        Ok(m)
    }
}

/// A discovered pack on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModPack {
    pub dir: PathBuf,
    pub manifest: ModManifest,
}

impl ModPack {
    pub fn entry_scene(&self) -> Option<PathBuf> {
        self.manifest.scenes.first().map(|rel| self.dir.join(rel))
    }

    pub fn matches_game(&self, game: &str) -> bool {
        self.manifest.game == "*" || self.manifest.game == game
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct ModPrefs {
    #[serde(default)]
    disabled: Vec<String>,
}

/// All packs Kerabit can see, plus the user's enable list.
#[derive(Clone, Debug)]
pub struct ModIndex {
    packs: Vec<ModPack>,
    disabled: HashSet<String>,
}

impl ModIndex {
    /// Scan default roots and load `~/.kerabit/mods.json`.
    pub fn discover() -> Self {
        let mut roots = Vec::new();
        if let Ok(cwd) = std::env::current_dir() {
            roots.push(cwd.join("mods"));
        }
        if let Some(home) = user_mods_dir() {
            roots.push(home);
        }
        if let Ok(extra) = std::env::var("KERABIT_MODS") {
            for part in extra.split(path_list_sep()) {
                if !part.is_empty() {
                    roots.push(PathBuf::from(part));
                }
            }
        }
        Self::discover_in(&roots)
    }

    pub fn discover_in(roots: &[PathBuf]) -> Self {
        let mut packs = Vec::new();
        let mut seen = HashSet::new();
        for root in roots {
            if !root.is_dir() {
                continue;
            }
            let Ok(read) = fs::read_dir(root) else {
                continue;
            };
            for ent in read.flatten() {
                let dir = ent.path();
                if !dir.is_dir() {
                    continue;
                }
                let manifest_path = dir.join(MOD_MANIFEST);
                let Ok(manifest) = ModManifest::load(&manifest_path) else {
                    continue;
                };
                if !seen.insert(manifest.id.clone()) {
                    // Later root wins (user / KERABIT_MODS overrides repo).
                    packs.retain(|p: &ModPack| p.manifest.id != manifest.id);
                }
                packs.push(ModPack { dir, manifest });
            }
        }
        packs.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
        let disabled = load_prefs().disabled.into_iter().collect();
        Self { packs, disabled }
    }

    pub fn packs(&self) -> &[ModPack] {
        &self.packs
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        !self.disabled.contains(id)
    }

    pub fn set_enabled(&mut self, id: &str, on: bool) {
        if on {
            self.disabled.remove(id);
        } else {
            self.disabled.insert(id.to_string());
        }
    }

    pub fn save_prefs(&self) -> Result<(), String> {
        let path = prefs_path().ok_or_else(|| "no home directory".to_string())?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let prefs = ModPrefs {
            disabled: {
                let mut v: Vec<String> = self.disabled.iter().cloned().collect();
                v.sort();
                v
            },
        };
        let text = serde_json::to_string_pretty(&prefs).map_err(|e| e.to_string())?;
        fs::write(path, text).map_err(|e| e.to_string())
    }

    pub fn enabled(&self) -> impl Iterator<Item = &ModPack> {
        self.packs.iter().filter(|p| self.is_enabled(&p.manifest.id))
    }

    /// Entry scenes from enabled packs for `game` (and `*`).
    pub fn extra_scenes(&self, game: &str) -> Vec<PathBuf> {
        self.enabled()
            .filter(|p| p.matches_game(game))
            .flat_map(|p| {
                p.manifest
                    .scenes
                    .iter()
                    .map(|rel| p.dir.join(rel))
                    .filter(|path| path.is_file())
            })
            .collect()
    }

    /// First enabled pack (search last-to-first) that contains `rel`.
    pub fn resolve(&self, rel: impl AsRef<Path>) -> Option<PathBuf> {
        let rel = rel.as_ref();
        for pack in self.enabled().collect::<Vec<_>>().into_iter().rev() {
            let candidate = pack.dir.join(rel);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }

    /// Create a starter pack under `parent / id`.
    pub fn scaffold(parent: impl AsRef<Path>, id: &str, name: &str) -> Result<PathBuf, String> {
        if !valid_id(id) {
            return Err("id must be kebab-case [a-z0-9-]".into());
        }
        let dir = parent.as_ref().join(id);
        if dir.exists() {
            return Err(format!("{} already exists", dir.display()));
        }
        fs::create_dir_all(dir.join("scenes")).map_err(|e| e.to_string())?;
        let manifest = ModManifest {
            id: id.to_string(),
            name: name.to_string(),
            version: "0.1.0".into(),
            author: String::new(),
            description: "A Kerabit community mod.".into(),
            game: "*".into(),
            scenes: vec!["scenes/entry.kerabit.json".into()],
            homepage: String::new(),
        };
        let text = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
        fs::write(dir.join(MOD_MANIFEST), text).map_err(|e| e.to_string())?;
        fs::write(dir.join("scenes/entry.kerabit.json"), SAMPLE_SCENE).map_err(|e| e.to_string())?;
        fs::write(dir.join("scenes/entry.juni"), SAMPLE_SCRIPT).map_err(|e| e.to_string())?;
        Ok(dir)
    }
}

const SAMPLE_SCENE: &str = r#"{
  "version": 1,
  "clear_color": [0.08, 0.09, 0.12],
  "ambient": [0.15, 0.16, 0.18],
  "camera": {
    "fov_y": 60.0,
    "eye": [5.0, 3.0, 7.0],
    "target": [0.0, 0.0, 0.0],
    "near": 0.1,
    "far": 100.0
  },
  "light": {
    "direction": [-0.35, -1.0, -0.25],
    "intensity": 1.2,
    "color": [1.0, 1.0, 1.0]
  },
  "extras": { "script": "entry.juni" },
  "entities": [
    {
      "name": "cube",
      "mesh": { "type": "cube" },
      "material": { "color": [0.91, 1.0, 0.29], "roughness": 0.35 },
      "at": [0.0, 0.5, 0.0]
    },
    {
      "name": "ground",
      "mesh": { "type": "plane", "size": 40.0 },
      "material": { "color": [0.5, 0.5, 0.5], "roughness": 0.9 },
      "at": [0.0, 0.0, 0.0]
    }
  ]
}
"#;

const SAMPLE_SCRIPT: &str = concat!(
    "# Community mod — `main` once, `frame` every Play frame.\n",
    "state:\n",
    "    cube: i32 = 0\n",
    "    t: f32 = 0.0\n",
    "\n",
    "fn main() -> i32:\n",
    "    cube = entity(\"cube\")\n",
    "    return 0\n",
    "\n",
    "fn frame(dt: f32) -> i32:\n",
    "    t = t + dt\n",
    "    rotate_y(cube, 1.1 * dt)\n",
    "    if key_pressed(\"Escape\"):\n",
    "        quit()\n",
    "    return 0\n",
);

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !id.starts_with('-')
        && !id.ends_with('-')
}

fn path_list_sep() -> char {
    if cfg!(windows) {
        ';'
    } else {
        ':'
    }
}

/// `~/.kerabit/mods`
pub fn user_mods_dir() -> Option<PathBuf> {
    kerabit_home().map(|h| h.join("mods"))
}

fn prefs_path() -> Option<PathBuf> {
    kerabit_home().map(|h| h.join("mods.json"))
}

fn kerabit_home() -> Option<PathBuf> {
    crate::user_data_dir()
}

fn load_prefs() -> ModPrefs {
    let Some(path) = prefs_path() else {
        return ModPrefs::default();
    };
    fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_ids() {
        assert!(!valid_id(""));
        assert!(!valid_id("Hello"));
        assert!(!valid_id("-x"));
        assert!(valid_id("hello-cube"));
    }

    #[test]
    fn discover_and_filter_game() {
        let tmp = std::env::temp_dir().join(format!("kerabit-mods-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        ModIndex::scaffold(&tmp, "hello-cube", "Hello Cube").expect("scaffold");
        let idx = ModIndex::discover_in(&[tmp.clone()]);
        assert_eq!(idx.packs().len(), 1);
        assert_eq!(idx.packs()[0].manifest.id, "hello-cube");
        assert_eq!(idx.extra_scenes("reach").len(), 1);
        assert!(idx.extra_scenes("reach")[0].ends_with("entry.kerabit.json"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn later_root_overrides_id() {
        let tmp = std::env::temp_dir().join(format!("kerabit-mods-ov-{}", std::process::id()));
        let a = tmp.join("a");
        let b = tmp.join("b");
        let _ = fs::remove_dir_all(&tmp);
        ModIndex::scaffold(&a, "pack", "A").unwrap();
        ModIndex::scaffold(&b, "pack", "B").unwrap();
        let idx = ModIndex::discover_in(&[a, b]);
        assert_eq!(idx.packs().len(), 1);
        assert_eq!(idx.packs()[0].manifest.name, "B");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn shipped_hello_cube_loads() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mods/hello-cube");
        let m = ModManifest::load(dir.join(MOD_MANIFEST)).expect("sample manifest");
        assert_eq!(m.id, "hello-cube");
        let scene = crate::Scene::load(dir.join(&m.scenes[0])).expect("sample scene");
        assert!(scene.entities.iter().any(|e| e.name == "cube"));
    }

    #[test]
    fn resolve_finds_file_in_enabled_pack() {
        let tmp = std::env::temp_dir().join(format!("kerabit-mods-rs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let dir = ModIndex::scaffold(&tmp, "hello-cube", "Hello").unwrap();
        let idx = ModIndex::discover_in(&[tmp.clone()]);
        let found = idx.resolve("scenes/entry.juni").expect("resolve");
        assert_eq!(found, dir.join("scenes/entry.juni"));
        let _ = fs::remove_dir_all(&tmp);
    }
}
