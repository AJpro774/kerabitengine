//! Per-frame input snapshot for Kerabit.
//!
//! Key/mouse enums are engine-owned. Mapping from `winit` happens inside the
//! `kerabit` facade — this crate has no window dependency.

use std::collections::HashSet;

/// Keyboard key used by game code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Escape,
    Space,
    Enter,
    Tab,
    Backspace,
    Shift,
    Control,
    Alt,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
}

/// Mouse button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Snapshot of keyboard/mouse state for one frame.
///
/// - [`Self::key_down`] / [`Self::mouse_button_down`]: held this frame
/// - [`Self::key_pressed`] / [`Self::mouse_button_pressed`]: edge (went down this frame)
#[derive(Debug, Clone, Default)]
pub struct InputState {
    keys_down: HashSet<Key>,
    keys_pressed: HashSet<Key>,
    keys_released: HashSet<Key>,
    buttons_down: HashSet<MouseButton>,
    buttons_pressed: HashSet<MouseButton>,
    buttons_released: HashSet<MouseButton>,
    mouse_x: f32,
    mouse_y: f32,
    mouse_dx: f32,
    mouse_dy: f32,
}

impl InputState {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// `true` while `key` is held.
    #[inline]
    pub fn key_down(&self, key: Key) -> bool {
        self.keys_down.contains(&key)
    }

    /// `true` only on the frame `key` transitioned to down.
    #[inline]
    pub fn key_pressed(&self, key: Key) -> bool {
        self.keys_pressed.contains(&key)
    }

    /// `true` only on the frame `key` was released.
    #[inline]
    pub fn key_released(&self, key: Key) -> bool {
        self.keys_released.contains(&key)
    }

    /// `true` while `button` is held.
    #[inline]
    pub fn mouse_button_down(&self, button: MouseButton) -> bool {
        self.buttons_down.contains(&button)
    }

    /// `true` only on the frame `button` transitioned to down.
    #[inline]
    pub fn mouse_button_pressed(&self, button: MouseButton) -> bool {
        self.buttons_pressed.contains(&button)
    }

    /// `true` only on the frame `button` was released.
    #[inline]
    pub fn mouse_button_released(&self, button: MouseButton) -> bool {
        self.buttons_released.contains(&button)
    }

    /// Cursor position in window pixels (origin top-left).
    #[inline]
    pub fn mouse_position(&self) -> (f32, f32) {
        (self.mouse_x, self.mouse_y)
    }

    /// Mouse movement since the last [`Self::end_frame`] (pixels).
    #[inline]
    pub fn mouse_delta(&self) -> (f32, f32) {
        (self.mouse_dx, self.mouse_dy)
    }

    /// Clear per-frame edges and mouse delta. Call after the game `run` closure.
    pub fn end_frame(&mut self) {
        self.keys_pressed.clear();
        self.keys_released.clear();
        self.buttons_pressed.clear();
        self.buttons_released.clear();
        self.mouse_dx = 0.0;
        self.mouse_dy = 0.0;
    }

    /// Engine: key went down or up.
    pub fn set_key(&mut self, key: Key, pressed: bool) {
        if pressed {
            if self.keys_down.insert(key) {
                self.keys_pressed.insert(key);
            }
        } else if self.keys_down.remove(&key) {
            self.keys_released.insert(key);
        }
    }

    /// Engine: mouse button went down or up.
    pub fn set_mouse_button(&mut self, button: MouseButton, pressed: bool) {
        if pressed {
            if self.buttons_down.insert(button) {
                self.buttons_pressed.insert(button);
            }
        } else if self.buttons_down.remove(&button) {
            self.buttons_released.insert(button);
        }
    }

    /// Engine: absolute cursor position (does not change delta).
    pub fn set_mouse_position(&mut self, x: f32, y: f32) {
        self.mouse_x = x;
        self.mouse_y = y;
    }

    /// Engine: accumulate pointer motion for this frame.
    pub fn add_mouse_delta(&mut self, dx: f32, dy: f32) {
        self.mouse_dx += dx;
        self.mouse_dy += dy;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_pressed_is_edge_only() {
        let mut input = InputState::new();
        input.set_key(Key::Escape, true);
        assert!(input.key_down(Key::Escape));
        assert!(input.key_pressed(Key::Escape));

        input.end_frame();
        assert!(input.key_down(Key::Escape));
        assert!(!input.key_pressed(Key::Escape));

        input.set_key(Key::Escape, false);
        assert!(!input.key_down(Key::Escape));
        assert!(input.key_released(Key::Escape));
    }

    #[test]
    fn mouse_delta_clears_each_frame() {
        let mut input = InputState::new();
        input.add_mouse_delta(3.0, -2.0);
        assert_eq!(input.mouse_delta(), (3.0, -2.0));
        input.end_frame();
        assert_eq!(input.mouse_delta(), (0.0, 0.0));
    }
}
