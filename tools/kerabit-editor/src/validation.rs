//! Scene validation for the editor status bar.

use std::collections::HashMap;
use std::path::Path;

use kerabit::{Scene, SceneMesh};

/// Collect human-readable validation issues for a scene.
pub fn validate(scene: &Scene, scene_dir: Option<&Path>) -> Vec<String> {
    let mut errors = Vec::new();

    let mut counts: HashMap<&str, usize> = HashMap::new();
    for e in &scene.entities {
        *counts.entry(e.name.as_str()).or_insert(0) += 1;
    }
    for (name, count) in &counts {
        if *count > 1 {
            errors.push(format!("duplicate entity name \"{name}\" ({count}×)"));
        }
    }

    let names: Vec<&str> = scene.entities.iter().map(|e| e.name.as_str()).collect();
    for e in &scene.entities {
        if let Some(parent) = &e.parent {
            if !names.contains(&parent.as_str()) {
                errors.push(format!(
                    "entity \"{}\" parent \"{}\" not found",
                    e.name, parent
                ));
            }
            if parent == &e.name {
                errors.push(format!("entity \"{}\" parents itself", e.name));
            }
        }

        match &e.mesh {
            SceneMesh::Obj { path } | SceneMesh::Gltf { path } => {
                if !asset_exists(path, scene_dir) {
                    errors.push(format!(
                        "entity \"{}\": missing mesh asset {}",
                        e.name,
                        path.display()
                    ));
                }
            }
            SceneMesh::Cube | SceneMesh::Plane { .. } => {}
        }

        if let Some(tex) = &e.material.texture {
            if !asset_exists(tex, scene_dir) {
                errors.push(format!(
                    "entity \"{}\": missing texture {}",
                    e.name,
                    tex.display()
                ));
            }
        }
    }

    errors
}

fn asset_exists(path: &Path, scene_dir: Option<&Path>) -> bool {
    if path.as_os_str().is_empty() {
        return false;
    }
    if path.is_file() {
        return true;
    }
    if let Some(dir) = scene_dir {
        if dir.join(path).is_file() {
            return true;
        }
    }
    false
}
