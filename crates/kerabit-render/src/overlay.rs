//! Immediate-mode screen overlay draw list (no wgpu types).
//!
//! Coordinates are **normalized 0–1 with origin at the top-left**:
//! - `(0, 0)` = top-left of the framebuffer
//! - `(1, 1)` = bottom-right
//! - `size` / `w` / `h` are fractions of the screen (width and height independently)

use kerabit_color::Color;

use crate::font8x8::{glyph_uv, solid_uv};

/// Soft cap so a runaway UI loop cannot explode GPU uploads.
const MAX_QUADS: usize = 4096;

/// One textured colored quad in normalized screen space.
#[derive(Clone, Copy, Debug)]
pub struct OverlayQuad {
    /// Top-left x in `0..=1`.
    pub x: f32,
    /// Top-left y in `0..=1`.
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub color: [f32; 4],
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
}

/// Collected overlay quads for one frame (cleared each frame by the engine).
#[derive(Clone, Debug, Default)]
pub struct OverlayCommands {
    quads: Vec<OverlayQuad>,
}

impl OverlayCommands {
    pub fn new() -> Self {
        Self {
            quads: Vec::with_capacity(64),
        }
    }

    /// Drop all queued draws (immediate-mode start-of-frame).
    pub fn clear(&mut self) {
        self.quads.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.quads.is_empty()
    }

    pub fn quads(&self) -> &[OverlayQuad] {
        &self.quads
    }

    /// Solid rectangle. `x,y` = top-left; `w,h` = size (normalized 0–1).
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        if self.quads.len() >= MAX_QUADS {
            return;
        }
        let (uv_min, uv_max) = solid_uv();
        self.quads.push(OverlayQuad {
            x,
            y,
            w,
            h,
            color: color.to_array(),
            uv_min,
            uv_max,
        });
    }

    /// Bitmap text. `size` is glyph height in normalized screen units;
    /// glyph width matches height (8×8 cells). Advances one `size` per character.
    /// Non-ASCII / unsupported glyphs render as `'?'`.
    pub fn text(&mut self, x: f32, y: f32, size: f32, color: Color, s: &str) {
        let mut cx = x;
        let mut cy = y;
        let color = color.to_array();
        for ch in s.chars() {
            if self.quads.len() >= MAX_QUADS {
                return;
            }
            if ch == '\n' {
                cx = x;
                cy += size;
                continue;
            }
            let (uv_min, uv_max) = glyph_uv(ch);
            self.quads.push(OverlayQuad {
                x: cx,
                y: cy,
                w: size,
                h: size,
                color,
                uv_min,
                uv_max,
            });
            cx += size;
        }
    }
}

/// GPU vertex for the overlay pass (NDC position + UV + color).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct OverlayVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

impl OverlayVertex {
    pub const ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x4,
    ];

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// Convert normalized top-left rect → six NDC triangle vertices.
pub fn quad_to_vertices(q: &OverlayQuad, out: &mut Vec<OverlayVertex>) {
    // Normalized top-left → NDC: x' = 2x-1, y' = 1-2y (flip Y).
    let x0 = q.x * 2.0 - 1.0;
    let x1 = (q.x + q.w) * 2.0 - 1.0;
    let y0 = 1.0 - q.y * 2.0;
    let y1 = 1.0 - (q.y + q.h) * 2.0;
    let u0 = q.uv_min[0];
    let v0 = q.uv_min[1];
    let u1 = q.uv_max[0];
    let v1 = q.uv_max[1];
    let c = q.color;

    // Two triangles: TL, TR, BR + TL, BR, BL
    let verts = [
        OverlayVertex {
            position: [x0, y0],
            uv: [u0, v0],
            color: c,
        },
        OverlayVertex {
            position: [x1, y0],
            uv: [u1, v0],
            color: c,
        },
        OverlayVertex {
            position: [x1, y1],
            uv: [u1, v1],
            color: c,
        },
        OverlayVertex {
            position: [x0, y0],
            uv: [u0, v0],
            color: c,
        },
        OverlayVertex {
            position: [x1, y1],
            uv: [u1, v1],
            color: c,
        },
        OverlayVertex {
            position: [x0, y1],
            uv: [u0, v1],
            color: c,
        },
    ];
    out.extend_from_slice(&verts);
}

/// Max overlay vertices uploaded per frame.
pub const MAX_OVERLAY_VERTICES: usize = MAX_QUADS * 6;

pub use crate::font8x8::{bake_atlas_rgba, ATLAS_HEIGHT, ATLAS_WIDTH};
