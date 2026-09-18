//! Hello Juni — spin a cube from a `.juni` script (Kerabit 3.0).
//!
//! ```bash
//! cargo run -p kerabit --example hello_juni
//! ```
//!
//! Escape quits (handled in `hello.juni`).

use std::path::PathBuf;

use kerabit::prelude::*;

fn scene_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/scenes/hello_juni.kerabit.json")
}

fn main() {
    Kerabit::new("Hello Juni")
        .load_scene(scene_path())
        .expect("hello_juni scene")
        .run(|ctx| {
            if let Some(err) = ctx.script_error().map(str::to_owned) {
                ctx.ui().text(0.02, 0.02, 0.022, Color::rgb(1.0, 0.35, 0.3), &err);
            }
        });
}
