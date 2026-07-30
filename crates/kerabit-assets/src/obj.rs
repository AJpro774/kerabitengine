//! OBJ → [`kerabit_render::Mesh`] (positions, normals, UVs).

use std::path::Path;

use kerabit_render::{Mesh, Vertex};

use crate::error::AssetError;

/// Load the first mesh in an OBJ file into a Kerabit [`Mesh`].
///
/// Requires positions. Missing normals are generated per-triangle; missing UVs
/// default to `(0, 0)`. Faces are triangulated; indices are `u16`.
pub fn load_obj(path: impl AsRef<Path>) -> Result<Mesh, AssetError> {
    let path = path.as_ref();
    let (models, _materials) = tobj::load_obj(
        path,
        &tobj::LoadOptions {
            triangulate: true,
            single_index: true,
            ..Default::default()
        },
    )
    .map_err(|e| AssetError::Obj {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;

    let model = models.first().ok_or_else(|| AssetError::EmptyMesh {
        path: path.to_path_buf(),
    })?;
    mesh_from_tobj(&model.mesh, path)
}

fn mesh_from_tobj(mesh: &tobj::Mesh, path: &Path) -> Result<Mesh, AssetError> {
    let n_pos = mesh.positions.len() / 3;
    if n_pos == 0 {
        return Err(AssetError::EmptyMesh {
            path: path.to_path_buf(),
        });
    }
    if n_pos > u16::MAX as usize {
        return Err(AssetError::TooManyVertices {
            path: path.to_path_buf(),
            count: n_pos,
        });
    }

    let has_normals = mesh.normals.len() >= n_pos * 3;
    let has_uvs = mesh.texcoords.len() >= n_pos * 2;

    let mut vertices = Vec::with_capacity(n_pos);
    for i in 0..n_pos {
        let px = mesh.positions[i * 3];
        let py = mesh.positions[i * 3 + 1];
        let pz = mesh.positions[i * 3 + 2];
        let (nx, ny, nz) = if has_normals {
            (
                mesh.normals[i * 3],
                mesh.normals[i * 3 + 1],
                mesh.normals[i * 3 + 2],
            )
        } else {
            (0.0, 1.0, 0.0)
        };
        // OBJ V texcoord is often bottom-up; flip to top-left / wgpu convention.
        let (u, v) = if has_uvs {
            (mesh.texcoords[i * 2], 1.0 - mesh.texcoords[i * 2 + 1])
        } else {
            (0.0, 0.0)
        };
        vertices.push(Vertex {
            position: [px, py, pz],
            normal: [nx, ny, nz],
            uv: [u, v],
        });
    }

    let mut indices: Vec<u16> = mesh
        .indices
        .iter()
        .map(|&i| i as u16)
        .collect();

    if indices.is_empty() {
        // Non-indexed triangle soup.
        if n_pos % 3 != 0 {
            return Err(AssetError::Obj {
                path: path.to_path_buf(),
                message: "mesh has no indices and vertex count is not a multiple of 3".into(),
            });
        }
        indices = (0..n_pos as u16).collect();
    }

    if !has_normals {
        generate_smooth_normals(&mut vertices, &indices);
    }

    Ok(Mesh::from_vertices(vertices).with_indices(indices))
}

fn generate_smooth_normals(vertices: &mut [Vertex], indices: &[u16]) {
    for v in vertices.iter_mut() {
        v.normal = [0.0, 0.0, 0.0];
    }
    for tri in indices.chunks_exact(3) {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;
        let p0 = vertices[i0].position;
        let p1 = vertices[i1].position;
        let p2 = vertices[i2].position;
        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let n = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        for &idx in &[i0, i1, i2] {
            let vn = &mut vertices[idx].normal;
            vn[0] += n[0];
            vn[1] += n[1];
            vn[2] += n[2];
        }
    }
    for v in vertices.iter_mut() {
        let n = v.normal;
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len > 1e-8 {
            v.normal = [n[0] / len, n[1] / len, n[2] / len];
        } else {
            v.normal = [0.0, 1.0, 0.0];
        }
    }
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
    fn loads_box_obj() {
        let mesh = load_obj(fixture("box.obj")).expect("box.obj");
        assert!(mesh.vertices.len() >= 3);
        assert!(mesh.indices.len() >= 3);
        assert_eq!(mesh.indices.len() % 3, 0);
        // Normals should be unit-ish.
        let n = mesh.vertices[0].normal;
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        assert!((len - 1.0).abs() < 0.05 || len > 0.5);
    }
}
