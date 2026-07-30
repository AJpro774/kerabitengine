//! Immediate-mode screen overlay (`ctx.ui()`).
//!
//! # Coordinate system
//!
//! **Normalized 0–1, origin top-left.**
//! - `(0.0, 0.0)` — top-left of the window
//! - `(1.0, 1.0)` — bottom-right
//! - Width / height / text `size` are fractions of the framebuffer (independent axes)
//!
//! Drawn after the 3D pass each frame. The draw list is cleared at the start of
//! every frame (immediate mode).

use kerabit_color::Color;
use kerabit_render::OverlayCommands;

/// Screen-space UI overlay for the current frame.
///
/// Use via [`crate::Context::ui`]. Do not hold across frames — the engine clears
/// the list every tick before your update closure runs.
pub struct Ui {
    pub(crate) cmds: OverlayCommands,
}

impl Ui {
    pub(crate) fn new() -> Self {
        Self {
            cmds: OverlayCommands::new(),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.cmds.clear();
    }

    pub(crate) fn commands(&self) -> &OverlayCommands {
        &self.cmds
    }

    /// Solid rectangle. `x,y` = top-left; `w,h` = size (normalized 0–1).
    ///
    /// ```ignore
    /// ctx.ui().rect(0.0, 0.0, 1.0, 1.0, Color::rgba(0.0, 0.0, 0.0, 0.5));
    /// ```
    #[inline]
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        self.cmds.rect(x, y, w, h, color);
    }

    /// Bitmap text (embedded 8×8 ASCII atlas). `size` is glyph height in
    /// normalized units; glyph width equals `size`. Advances one `size` per
    /// character. `\n` moves to the next line.
    ///
    /// ```ignore
    /// ctx.ui().text(0.05, 0.05, 0.04, Color::WHITE, "REACH");
    /// ```
    #[inline]
    pub fn text(&mut self, x: f32, y: f32, size: f32, color: Color, s: &str) {
        self.cmds.text(x, y, size, color, s);
    }
}
