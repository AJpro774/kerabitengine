//! Project-rooted file browser for meshes, textures, scripts, and HDR.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use egui::{Color32, RichText, Ui};

const SKIP_DIRS: &[&str] = &[
    "target",
    ".git",
    "node_modules",
    ".cursor",
    ".vercel",
    ".github",
];

const MAX_CHILDREN: usize = 400;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetKind {
    Directory,
    Mesh,
    Texture,
    Script,
    Hdr,
    Other,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    Meshes,
    Textures,
    Scripts,
}

impl Filter {
    fn label(self) -> &'static str {
        match self {
            Filter::All => "all",
            Filter::Meshes => "meshes",
            Filter::Textures => "textures",
            Filter::Scripts => "scripts",
        }
    }

    fn allows(self, kind: AssetKind) -> bool {
        match (self, kind) {
            (Filter::All, AssetKind::Other) => false,
            (Filter::All, _) => true,
            (Filter::Meshes, AssetKind::Mesh | AssetKind::Directory) => true,
            (Filter::Textures, AssetKind::Texture | AssetKind::Directory) => true,
            (Filter::Scripts, AssetKind::Script | AssetKind::Directory) => true,
            _ => false,
        }
    }
}

/// Action produced by a double-click or Apply in the browser.
#[derive(Clone, Debug)]
pub enum AssetAction {
    OpenScript(PathBuf),
    AssignMesh(PathBuf),
    AssignTexture(PathBuf),
    AssignHdr(PathBuf),
}

/// Cached directory tree rooted at the Kerabit project (or the scene folder).
pub struct AssetBrowser {
    root: Option<PathBuf>,
    filter: Filter,
    selected: Option<PathBuf>,
    expanded: HashSet<PathBuf>,
    children: HashMap<PathBuf, Vec<PathBuf>>,
}

impl AssetBrowser {
    pub fn new() -> Self {
        Self {
            root: None,
            filter: Filter::All,
            selected: None,
            expanded: HashSet::new(),
            children: HashMap::new(),
        }
    }

    pub fn sync_root(&mut self, root: Option<PathBuf>) {
        if self.root == root {
            return;
        }
        self.root = root;
        self.children.clear();
        self.expanded.clear();
        self.selected = None;
        if let Some(root) = self.root.clone() {
            self.expanded.insert(root.clone());
            self.refresh(&root);
        }
    }

    fn refresh(&mut self, dir: &Path) {
        let mut entries = Vec::new();
        if let Ok(read) = std::fs::read_dir(dir) {
            for ent in read.flatten() {
                let path = ent.path();
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.starts_with('.') || SKIP_DIRS.contains(&name) {
                    continue;
                }
                entries.push(path);
                if entries.len() >= MAX_CHILDREN {
                    break;
                }
            }
        }
        entries.sort_by(|a, b| {
            let da = a.is_dir();
            let db = b.is_dir();
            db.cmp(&da).then_with(|| a.file_name().cmp(&b.file_name()))
        });
        self.children.insert(dir.to_path_buf(), entries);
    }

    /// Draw the tree. Returns an action when the user applies a file.
    pub fn ui(&mut self, ui: &mut Ui) -> Option<AssetAction> {
        let mut action = None;
        ui.horizontal(|ui| {
            ui.heading("Assets");
            if ui.button("Refresh").clicked() {
                if let Some(root) = self.root.clone() {
                    self.children.clear();
                    self.refresh(&root);
                }
            }
        });
        egui::ComboBox::from_id_salt("asset_filter")
            .selected_text(self.filter.label())
            .show_ui(ui, |ui| {
                for f in [Filter::All, Filter::Meshes, Filter::Textures, Filter::Scripts] {
                    ui.selectable_value(&mut self.filter, f, f.label());
                }
            });
        let Some(root) = self.root.clone() else {
            ui.label(
                RichText::new("Save the scene to browse the project tree.")
                    .small()
                    .weak(),
            );
            return None;
        };
        ui.label(
            RichText::new(root.display().to_string())
                .small()
                .weak(),
        );
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("asset_tree")
            .show(ui, |ui| {
                action = self.draw_dir(ui, &root, 0);
            });
        if let Some(sel) = self.selected.clone() {
            let kind = classify(&sel);
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(
                        sel.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("—"),
                    )
                    .small(),
                );
                match kind {
                    AssetKind::Mesh => {
                        if ui.button("Apply mesh").clicked() {
                            action = Some(AssetAction::AssignMesh(sel));
                        }
                    }
                    AssetKind::Texture => {
                        if ui.button("Apply texture").clicked() {
                            action = Some(AssetAction::AssignTexture(sel));
                        }
                    }
                    AssetKind::Script => {
                        if ui.button("Open").clicked() {
                            action = Some(AssetAction::OpenScript(sel));
                        }
                    }
                    AssetKind::Hdr => {
                        if ui.button("Apply HDR").clicked() {
                            action = Some(AssetAction::AssignHdr(sel));
                        }
                    }
                    _ => {}
                }
            });
        }
        action
    }

    fn draw_dir(&mut self, ui: &mut Ui, dir: &Path, depth: u32) -> Option<AssetAction> {
        if !self.children.contains_key(dir) {
            self.refresh(dir);
        }
        let children = self.children.get(dir).cloned().unwrap_or_default();
        let mut action = None;
        for path in children {
            let kind = classify(&path);
            if !self.filter.allows(kind) {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string();
            let selected = self.selected.as_deref() == Some(path.as_path());
            ui.horizontal(|ui| {
                ui.add_space(depth as f32 * 12.0);
                if kind == AssetKind::Directory {
                    let open = self.expanded.contains(&path);
                    let arrow = if open { "▾" } else { "▸" };
                    let resp = ui.selectable_label(selected, format!("{arrow} {name}/"));
                    if resp.clicked() {
                        if open {
                            self.expanded.remove(&path);
                        } else {
                            self.expanded.insert(path.clone());
                            self.refresh(&path);
                        }
                        self.selected = Some(path.clone());
                    }
                } else {
                    let mark = match kind {
                        AssetKind::Mesh => "mesh",
                        AssetKind::Texture => "tex",
                        AssetKind::Script => "juni",
                        AssetKind::Hdr => "hdr",
                        _ => "file",
                    };
                    let resp = ui.selectable_label(selected, format!("{name}  {mark}"));
                    if resp.clicked() {
                        self.selected = Some(path.clone());
                    }
                    if resp.double_clicked() {
                        action = match kind {
                            AssetKind::Mesh => Some(AssetAction::AssignMesh(path.clone())),
                            AssetKind::Texture => Some(AssetAction::AssignTexture(path.clone())),
                            AssetKind::Script => Some(AssetAction::OpenScript(path.clone())),
                            AssetKind::Hdr => Some(AssetAction::AssignHdr(path.clone())),
                            _ => None,
                        };
                    }
                }
            });
            if kind == AssetKind::Directory && self.expanded.contains(&path) {
                if let Some(child) = self.draw_dir(ui, &path, depth + 1) {
                    action = Some(child);
                }
            }
        }
        action
    }
}

pub fn classify(path: &Path) -> AssetKind {
    if path.is_dir() {
        return AssetKind::Directory;
    }
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("obj" | "glb" | "gltf" | "fbx") => AssetKind::Mesh,
        Some("png" | "jpg" | "jpeg") => AssetKind::Texture,
        Some("juni") => AssetKind::Script,
        Some("hdr") => AssetKind::Hdr,
        _ => AssetKind::Other,
    }
}

/// Walk toward the filesystem root until `Cargo.toml` or `.git` appears.
pub fn find_project_root(start: &Path) -> PathBuf {
    let mut cur = start;
    loop {
        if cur.join("Cargo.toml").is_file()
            || cur.join(".git").exists()
            || cur.join("games").join("reach").is_dir()
        {
            return cur.to_path_buf();
        }
        match cur.parent() {
            Some(parent) => cur = parent,
            None => return start.to_path_buf(),
        }
    }
}

pub fn project_root_from(scene_dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(dir) = scene_dir {
        return Some(find_project_root(dir));
    }
    std::env::current_dir().ok().map(|d| find_project_root(&d))
}

/// Path of `file` relative to `base_dir`, using `..` when needed.
pub fn rel_to(base_dir: &Path, file: &Path) -> String {
    let base = abs_path(base_dir);
    let file = abs_path(file);
    if let Ok(rel) = file.strip_prefix(&base) {
        return rel.to_string_lossy().replace('\\', "/");
    }
    let mut up = PathBuf::new();
    let mut cur = base.as_path();
    while let Some(parent) = cur.parent() {
        up.push("..");
        if let Ok(rel) = file.strip_prefix(parent) {
            return up.join(rel).to_string_lossy().replace('\\', "/");
        }
        cur = parent;
    }
    file.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| file.to_string_lossy().into_owned())
}

fn abs_path(path: &Path) -> PathBuf {
    if let Ok(c) = path.canonicalize() {
        return c;
    }
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

/// Display name for a stored scene-relative path (last component).
pub fn file_label(stored: &str) -> String {
    if stored.trim().is_empty() {
        return "(none)".into();
    }
    Path::new(stored)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| stored.to_string())
}

pub fn mesh_kind_from_path(path: &Path) -> Option<MeshExt> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("obj") => Some(MeshExt::Obj),
        Some("glb" | "gltf") => Some(MeshExt::Gltf),
        Some("fbx") => Some(MeshExt::Fbx),
        _ => None,
    }
}

#[derive(Clone, Copy)]
pub enum MeshExt {
    Obj,
    Gltf,
    Fbx,
}

/// One inspector row: current file name + Browse / Clear (no free-typed path).
pub fn file_row(ui: &mut Ui, current: &str, can_open: bool) -> FileRowEvent {
    let mut event = FileRowEvent::None;
    ui.horizontal(|ui| {
        let empty = current.trim().is_empty();
        let color = if empty {
            Color32::from_rgb(0xb7, 0xa9, 0x9a)
        } else {
            Color32::from_rgb(0xf3, 0xeb, 0xe1)
        };
        ui.label(RichText::new(file_label(current)).color(color));
        if ui.button("Browse…").clicked() {
            event = FileRowEvent::Browse;
        }
        if can_open && ui.add_enabled(!empty, egui::Button::new("Edit")).clicked() {
            event = FileRowEvent::Open;
        }
        if ui.add_enabled(!empty, egui::Button::new("Clear")).clicked() {
            event = FileRowEvent::Clear;
        }
    });
    event
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FileRowEvent {
    None,
    Browse,
    Open,
    Clear,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_extensions() {
        assert_eq!(classify(Path::new("pistol.glb")), AssetKind::Mesh);
        assert_eq!(classify(Path::new("hero.fbx")), AssetKind::Mesh);
        assert_eq!(classify(Path::new("box.obj")), AssetKind::Mesh);
        assert_eq!(classify(Path::new("brick.png")), AssetKind::Texture);
        assert_eq!(classify(Path::new("spin.juni")), AssetKind::Script);
        assert_eq!(classify(Path::new("sky.hdr")), AssetKind::Hdr);
    }

    #[test]
    fn rel_to_walks_up_from_scene_dir() {
        let tmp = std::env::temp_dir().join(format!("kerabit-rel-{}", std::process::id()));
        let scenes = tmp.join("scenes");
        let assets = tmp.join("assets");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&scenes).unwrap();
        std::fs::create_dir_all(&assets).unwrap();
        let file = assets.join("pistol.glb");
        std::fs::write(&file, []).unwrap();
        let rel = rel_to(&scenes, &file);
        assert_eq!(rel, "../assets/pistol.glb");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn file_label_uses_basename() {
        assert_eq!(file_label("../assets/pistol.glb"), "pistol.glb");
        assert_eq!(file_label(""), "(none)");
    }
}
