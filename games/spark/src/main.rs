//! Spark — Kerabit 3.0 proof: almost all gameplay lives in `spark.juni`.
//!
//! ```bash
//! cargo run -p spark
//! ```
//!
//! Rust only bootstraps the window and shows script errors. WASD move, reach
//! the cyan pad, avoid orange hazards. Space retries after win/fail. Esc quits.

use std::path::PathBuf;

use kerabit::prelude::*;

fn scene_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scenes/spark.kerabit.json")
}

fn main() {
    Kerabit::new("Spark")
        .load_scene(scene_path())
        .expect("spark scene")
        .run(|ctx| {
            if let Some(err) = ctx.script_error().map(str::to_owned) {
                ctx.ui().text(0.02, 0.02, 0.022, Color::rgb(1.0, 0.35, 0.3), &err);
            }
        });
}
