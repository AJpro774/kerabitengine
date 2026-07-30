//! Minimal glTF 2.0 import: first mesh primitive + base color factor/texture.

use std::path::Path;

use kerabit_color::Color;
use kerabit_render::{Mesh, Vertex};

use crate::error::AssetError;
use crate::texture::Texture;

/// Result of a lite glTF load: geometry plus a flat material description.
#[derive(Clone, Debug)]
pub struct GltfMesh {
    pub mesh: Mesh,
    pub albedo: Color,
    pub albedo_texture: Option<Texture>,
}

/// Load the **first** mesh / **first** primitive from a `.gltf` / `.glb`.
///
/// Supported: `POSITION`, optional `NORMAL` / `TEXCOORD_0`, triangle indices,
/// `pbrMetallicRoughness.baseColorFactor`, and optional `baseColorTexture`
/// (embedded or external image). No animation, skins, or morph targets.
pub fn load_gltf(path: impl AsRef<Path>) -> Result<GltfMesh, AssetError> {
    let path = path.as_ref();
    let (document, buffers, images) = gltf::import(path).map_err(|e| AssetError::Gltf {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;

    let mesh = document.meshes().next().ok_or_else(|| AssetError::EmptyMesh {
        path: path.to_path_buf(),
    })?;
    let primitive = mesh
        .primitives()
        .next()
        .ok_or_else(|| AssetError::EmptyMesh {
            path: path.to_path_buf(),
        })?;

    let reader = primitive.reader(|buffer| Some(buffers.get(buffer.index())?.0.as_slice()));

    let positions: Vec<[f32; 3]> = reader
        .read_positions()
        .ok_or_else(|| AssetError::Gltf {
            path: path.to_path_buf(),
            message: "primitive missing POSITION".into(),
        })?
        .collect();

    if positions.is_empty() {
        return Err(AssetError::EmptyMesh {
            path: path.to_path_buf(),
        });
    }
    if positions.len() > u16::MAX as usize {
        return Err(AssetError::TooManyVertices {
            path: path.to_path_buf(),
            count: positions.len(),
        });
    }

    let normals: Option<Vec<[f32; 3]>> = reader.read_normals().map(|iter| iter.collect());
    let tex_coords: Option<Vec<[f32; 2]>> = reader
        .read_tex_coords(0)
        .map(|tc| tc.into_f32().collect());

    let mut vertices = Vec::with_capacity(positions.len());
    for i in 0..positions.len() {
        let normal = normals
            .as_ref()
            .and_then(|n| n.get(i).copied())
            .unwrap_or([0.0, 1.0, 0.0]);
        let uv = tex_coords
            .as_ref()
            .and_then(|t| t.get(i).copied())
            .unwrap_or([0.0, 0.0]);
        vertices.push(Vertex {
            position: positions[i],
            normal,
            uv,
        });
    }

    let indices: Vec<u16> = if let Some(idx) = reader.read_indices() {
        idx.into_u32()
            .map(|i| i as u16)
            .collect()
    } else {
        (0..vertices.len() as u16).collect()
    };

    if normals.is_none() {
        generate_smooth_normals(&mut vertices, &indices);
    }

    let material = primitive.material();
    let pbr = material.pbr_metallic_roughness();
    let factor = pbr.base_color_factor();
    let albedo = Color::rgba(factor[0], factor[1], factor[2], factor[3]);

    let albedo_texture = match pbr.base_color_texture() {
        Some(info) => Some(texture_from_gltf_image(
            &images,
            info.texture().source().index(),
            path,
        )?),
        None => None,
    };

    Ok(GltfMesh {
        mesh: Mesh::from_vertices(vertices).with_indices(indices),
        albedo,
        albedo_texture,
    })
}

fn texture_from_gltf_image(
    images: &[gltf::image::Data],
    index: usize,
    path: &Path,
) -> Result<Texture, AssetError> {
    let image = images.get(index).ok_or_else(|| AssetError::Gltf {
        path: path.to_path_buf(),
        message: format!("missing image index {index}"),
    })?;

    let rgba = match image.format {
        gltf::image::Format::R8G8B8A8 => image.pixels.clone(),
        gltf::image::Format::R8G8B8 => {
            let mut out = Vec::with_capacity(image.pixels.len() / 3 * 4);
            for chunk in image.pixels.chunks_exact(3) {
                out.extend_from_slice(chunk);
                out.push(255);
            }
            out
        }
        gltf::image::Format::R8 => {
            let mut out = Vec::with_capacity(image.pixels.len() * 4);
            for &p in &image.pixels {
                out.extend_from_slice(&[p, p, p, 255]);
            }
            out
        }
        gltf::image::Format::R8G8 => {
            let mut out = Vec::with_capacity(image.pixels.len() / 2 * 4);
            for chunk in image.pixels.chunks_exact(2) {
                out.extend_from_slice(&[chunk[0], chunk[0], chunk[0], chunk[1]]);
            }
            out
        }
        other => {
            return Err(AssetError::Gltf {
                path: path.to_path_buf(),
                message: format!("unsupported image format: {other:?}"),
            });
        }
    };

    Ok(Texture::from_rgba8(image.width, image.height, rgba))
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
    fn loads_box_gltf() {
        let loaded = load_gltf(fixture("box.gltf")).expect("box.gltf");
        assert!(loaded.mesh.vertices.len() >= 3);
        assert!(loaded.mesh.indices.len() >= 3);
        assert!(loaded.albedo.r > 0.0);
    }
}
