//! Frozen GPU vertex layout for Kerabit.
//!
//! ```text
//! location 0: position  f32x3
//! location 1: normal    f32x3
//! location 2: uv        f32x2
//! ```
//!
//! Changing this is a cross-crate breaking change — update ARCHITECTURE.md first.

use wgpu::vertex_attr_array;

/// Interleaved vertex matching WGSL `VertexInput` and the frozen layout.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

impl Vertex {
    pub const ATTRIBUTES: [wgpu::VertexAttribute; 3] = vertex_attr_array![
        0 => Float32x3,
        1 => Float32x3,
        2 => Float32x2,
    ];

    /// Vertex buffer layout for render pipelines.
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}
