//! FBX import via ufbx: first triangle mesh + optional diffuse / base color.

use std::path::Path;

use kerabit_color::Color;
use kerabit_render::{Mesh, Vertex};

use crate::error::AssetError;
use crate::gltf_lite::generate_smooth_normals;

/// Result of an FBX load: geometry plus a flat material color when present.
#[derive(Clone, Debug)]
pub struct FbxMesh {
    pub mesh: Mesh,
    pub albedo: Color,
}

/// Load the **first** triangulated mesh from an `.fbx` (ASCII or binary).
///
/// Positions, optional normals / UVs. Coordinate space is normalized to
/// right-handed Y-up, meters. No animation, skins, or embedded textures.
pub fn load_fbx(path: impl AsRef<Path>) -> Result<FbxMesh, AssetError> {
    let path = path.as_ref();
    let filename = path.to_str().ok_or_else(|| AssetError::Fbx {
        path: path.to_path_buf(),
        message: "path is not valid UTF-8".into(),
    })?;

    let opts = ufbx::LoadOpts {
        target_axes: ufbx::CoordinateAxes::right_handed_y_up(),
        target_unit_meters: 1.0,
        generate_missing_normals: true,
        ignore_animation: true,
        ..Default::default()
    };

    let scene = ufbx::load_file(filename, opts).map_err(|e| AssetError::Fbx {
        path: path.to_path_buf(),
        message: e.description.to_string(),
    })?;

    let src = scene
        .meshes
        .iter()
        .find(|m| m.num_triangles > 0)
        .ok_or_else(|| AssetError::EmptyMesh {
            path: path.to_path_buf(),
        })?;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut tri = Vec::new();
    let mut had_normals = true;

    for &face in src.faces.iter() {
        if face.num_indices < 3 {
            continue;
        }
        tri.clear();
        let ntris = ufbx::triangulate_face_vec(&mut tri, src, face);
        for i in 0..(ntris as usize * 3) {
            let corner = tri[i] as usize;
            let p = src.vertex_position[corner];
            let (normal, sourced) = if src.vertex_normal.exists {
                let n = src.vertex_normal[corner];
                ([n.x as f32, n.y as f32, n.z as f32], true)
            } else {
                ([0.0, 1.0, 0.0], false)
            };
            had_normals &= sourced;
            let uv = if src.vertex_uv.exists {
                let t = src.vertex_uv[corner];
                [t.x as f32, 1.0 - t.y as f32]
            } else {
                [0.0, 0.0]
            };
            let vi = vertices.len();
            if vi > u16::MAX as usize {
                return Err(AssetError::TooManyVertices {
                    path: path.to_path_buf(),
                    count: vi,
                });
            }
            vertices.push(Vertex {
                position: [p.x as f32, p.y as f32, p.z as f32],
                normal,
                uv,
            });
            indices.push(vi as u16);
        }
    }

    if vertices.is_empty() {
        return Err(AssetError::EmptyMesh {
            path: path.to_path_buf(),
        });
    }
    if !had_normals {
        generate_smooth_normals(&mut vertices, &indices);
    }

    let albedo = src
        .materials
        .iter()
        .next()
        .map(|m| albedo_from_material(m))
        .unwrap_or(Color::WHITE);

    Ok(FbxMesh {
        mesh: Mesh::from_vertices(vertices).with_indices(indices),
        albedo,
    })
}

fn albedo_from_material(mat: &ufbx::Material) -> Color {
    let pbr = &mat.pbr.base_color;
    if pbr.has_value {
        let c = pbr.value_vec4;
        return Color::rgb(c.x as f32, c.y as f32, c.z as f32);
    }
    let diff = &mat.fbx.diffuse_color;
    if diff.has_value {
        let c = diff.value_vec4;
        return Color::rgb(c.x as f32, c.y as f32, c.z as f32);
    }
    Color::WHITE
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join(name)
    }

    #[test]
    fn loads_box_fbx() {
        let loaded = load_fbx(fixture("box.fbx")).expect("box.fbx");
        assert!(loaded.mesh.vertices.len() >= 3);
        assert!(loaded.mesh.indices.len() >= 3);
        assert_eq!(loaded.mesh.indices.len() % 3, 0);
    }
}
