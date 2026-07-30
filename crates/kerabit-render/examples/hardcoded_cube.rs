//! P1 smoke: window + shaded hardcoded cube.
//!
//! ```bash
//! cargo run -p kerabit-render --example hardcoded_cube
//! ```
//!
//! Escape or close the window to quit. Resize should not crash.

use kerabit_color::Color;
use kerabit_render::run_hardcoded_cube;

fn main() {
    let clear = Color::rgb(0.08, 0.09, 0.12);
    if let Err(err) = run_hardcoded_cube("Kerabit — hardcoded cube", clear) {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}
