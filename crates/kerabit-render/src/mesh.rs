//! CPU mesh types and builders (cube / plane / custom).

use crate::vertex::Vertex;

/// Indexed triangle mesh on the CPU. Upload via [`crate::MeshCache`] for drawing.
#[derive(Clone, Debug)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
}

impl Mesh {
    /// Unit cube centered at the origin (edge length 1, −0.5…0.5). Face-unique vertices.
    pub fn cube() -> Self {
        let (vertices, indices) = cube_geometry();
        Self { vertices, indices }
    }

    /// Axis-aligned plane on XZ, centered at origin, edge length `size`, normal +Y.
    pub fn plane(size: f32) -> Self {
        let h = size * 0.5;
        let vertices = vec![
            Vertex {
                position: [-h, 0.0, h],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0, 0.0],
            },
            Vertex {
                position: [h, 0.0, h],
                normal: [0.0, 1.0, 0.0],
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [h, 0.0, -h],
                normal: [0.0, 1.0, 0.0],
                uv: [1.0, 1.0],
            },
            Vertex {
                position: [-h, 0.0, -h],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0, 1.0],
            },
        ];
        let indices = vec![0, 1, 2, 0, 2, 3];
        Self { vertices, indices }
    }

    /// Start a custom mesh from vertex data. Call [`MeshBuilder::with_indices`] to finish.
    pub fn from_vertices(vertices: Vec<Vertex>) -> MeshBuilder {
        MeshBuilder { vertices }
    }

    /// Number of indices (triangle list).
    pub fn index_count(&self) -> u32 {
        self.indices.len() as u32
    }
}

/// Builder for custom meshes: vertices first, then indices.
pub struct MeshBuilder {
    vertices: Vec<Vertex>,
}

impl MeshBuilder {
    /// Attach an index buffer and produce a [`Mesh`].
    pub fn with_indices(self, indices: Vec<u16>) -> Mesh {
        Mesh {
            vertices: self.vertices,
            indices,
        }
    }
}

fn cube_geometry() -> (Vec<Vertex>, Vec<u16>) {
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);

    // (normal, four corners in CCW order when viewed along +normal)
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        (
            [0.0, 0.0, 1.0],
            [
                [-0.5, -0.5, 0.5],
                [0.5, -0.5, 0.5],
                [0.5, 0.5, 0.5],
                [-0.5, 0.5, 0.5],
            ],
        ),
        (
            [0.0, 0.0, -1.0],
            [
                [0.5, -0.5, -0.5],
                [-0.5, -0.5, -0.5],
                [-0.5, 0.5, -0.5],
                [0.5, 0.5, -0.5],
            ],
        ),
        (
            [0.0, 1.0, 0.0],
            [
                [-0.5, 0.5, 0.5],
                [0.5, 0.5, 0.5],
                [0.5, 0.5, -0.5],
                [-0.5, 0.5, -0.5],
            ],
        ),
        (
            [0.0, -1.0, 0.0],
            [
                [-0.5, -0.5, -0.5],
                [0.5, -0.5, -0.5],
                [0.5, -0.5, 0.5],
                [-0.5, -0.5, 0.5],
            ],
        ),
        (
            [1.0, 0.0, 0.0],
            [
                [0.5, -0.5, 0.5],
                [0.5, -0.5, -0.5],
                [0.5, 0.5, -0.5],
                [0.5, 0.5, 0.5],
            ],
        ),
        (
            [-1.0, 0.0, 0.0],
            [
                [-0.5, -0.5, -0.5],
                [-0.5, -0.5, 0.5],
                [-0.5, 0.5, 0.5],
                [-0.5, 0.5, -0.5],
            ],
        ),
    ];

    let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

    for (normal, corners) in faces {
        let base = vertices.len() as u16;
        for (i, position) in corners.into_iter().enumerate() {
            vertices.push(Vertex {
                position,
                normal,
                uv: uvs[i],
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_has_expected_topology() {
        let m = Mesh::cube();
        assert_eq!(m.vertices.len(), 24);
        assert_eq!(m.indices.len(), 36);
    }

    #[test]
    fn plane_has_four_verts() {
        let m = Mesh::plane(10.0);
        assert_eq!(m.vertices.len(), 4);
        assert_eq!(m.indices.len(), 6);
        assert!((m.vertices[1].position[0] - 5.0).abs() < 1e-5);
    }

    #[test]
    fn from_vertices_with_indices() {
        let m = Mesh::from_vertices(vec![
            Vertex {
                position: [0.0, 0.0, 0.0],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0, 0.0],
            },
            Vertex {
                position: [1.0, 0.0, 0.0],
                normal: [0.0, 1.0, 0.0],
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [0.0, 0.0, 1.0],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0, 1.0],
            },
        ])
        .with_indices(vec![0, 1, 2]);
        assert_eq!(m.index_count(), 3);
    }
}
