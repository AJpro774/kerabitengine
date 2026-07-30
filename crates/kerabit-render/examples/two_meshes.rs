//! P2 harness: lit plane + cube with Camera / Light / Mesh builders.
//!
//! ```bash
//! cargo run -p kerabit-render --example two_meshes
//! ```
//!
//! Escape or close the window to quit. Resize should not crash.

use kerabit_color::Color;
use kerabit_render::run_two_meshes;

fn main() {
    let clear = Color::rgb(0.08, 0.09, 0.12);
    if let Err(err) = run_two_meshes("Kerabit — two meshes", clear) {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}
