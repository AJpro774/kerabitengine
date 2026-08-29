//! Hello Rhai — spin a cube from a `.rhai` script (Kerabit 2.0).
//!
//! ```bash
//! cargo run -p kerabit --example hello_rhai
//! ```
//!
//! Escape quits (handled in `hello.rhai`).

use std::path::PathBuf;

use kerabit::prelude::*;

fn scene_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/scenes/hello_rhai.kerabit.json")
}

fn main() {
    Kerabit::new("Hello Rhai")
        .load_scene(scene_path())
        .expect("hello_rhai scene")
        .run(|ctx| {
            if let Some(err) = ctx.script_error().map(str::to_owned) {
                ctx.ui().text(0.02, 0.02, 0.022, Color::rgb(1.0, 0.35, 0.3), &err);
            }
        });
}
