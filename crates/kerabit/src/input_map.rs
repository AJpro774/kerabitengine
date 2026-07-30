//! Map winit logical keys / mouse buttons → [`kerabit_input`] enums.
//!
//! Kept private to the facade so game code never sees winit types.

use kerabit_input::{Key, MouseButton};
use winit::event::MouseButton as WinitMouseButton;
use winit::keyboard::{Key as WinitKey, NamedKey};

pub fn map_key(key: &WinitKey) -> Option<Key> {
    match key {
        WinitKey::Named(named) => map_named(*named),
        WinitKey::Character(c) => map_character(c.as_str()),
        _ => None,
    }
}

fn map_named(named: NamedKey) -> Option<Key> {
    Some(match named {
        NamedKey::Escape => Key::Escape,
        NamedKey::Space => Key::Space,
        NamedKey::Enter => Key::Enter,
        NamedKey::Tab => Key::Tab,
        NamedKey::Backspace => Key::Backspace,
        NamedKey::Shift => Key::Shift,
        NamedKey::Control => Key::Control,
        NamedKey::Alt => Key::Alt,
        NamedKey::ArrowLeft => Key::Left,
        NamedKey::ArrowRight => Key::Right,
        NamedKey::ArrowUp => Key::Up,
        NamedKey::ArrowDown => Key::Down,
        _ => return None,
    })
}

fn map_character(s: &str) -> Option<Key> {
    let mut chars = s.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    Some(match c.to_ascii_lowercase() {
        'a' => Key::A,
        'b' => Key::B,
        'c' => Key::C,
        'd' => Key::D,
        'e' => Key::E,
        'f' => Key::F,
        'g' => Key::G,
        'h' => Key::H,
        'i' => Key::I,
        'j' => Key::J,
        'k' => Key::K,
        'l' => Key::L,
        'm' => Key::M,
        'n' => Key::N,
        'o' => Key::O,
        'p' => Key::P,
        'q' => Key::Q,
        'r' => Key::R,
        's' => Key::S,
        't' => Key::T,
        'u' => Key::U,
        'v' => Key::V,
        'w' => Key::W,
        'x' => Key::X,
        'y' => Key::Y,
        'z' => Key::Z,
        '0' => Key::Digit0,
        '1' => Key::Digit1,
        '2' => Key::Digit2,
        '3' => Key::Digit3,
        '4' => Key::Digit4,
        '5' => Key::Digit5,
        '6' => Key::Digit6,
        '7' => Key::Digit7,
        '8' => Key::Digit8,
        '9' => Key::Digit9,
        _ => return None,
    })
}

pub fn map_mouse_button(button: WinitMouseButton) -> Option<MouseButton> {
    Some(match button {
        WinitMouseButton::Left => MouseButton::Left,
        WinitMouseButton::Right => MouseButton::Right,
        WinitMouseButton::Middle => MouseButton::Middle,
        _ => return None,
    })
}
