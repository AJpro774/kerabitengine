//! Play mode via the public Kerabit API (`Scene` → `Kerabit::run`).
//!
//! Invoked as `kerabit-editor --play <path.kerabit.json>` so the editor shell
//! (eframe) and the play window (winit) never share one event loop. The play
//! process does **not** inject a fly camera or HUD — the scene camera and any
//! Juni `ui_text` / `set_camera` calls are the game. Escape or closing the
//! window exits; the parent editor restores selection.

use std::path::Path;

use kerabit::{Color, Kerabit, Key};

/// Load `path` and open a Kerabit play window until Escape / close.
pub fn run(path: &Path) {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("scene");
    let title = format!("Kerabit Play — {name}");

    let kerabit = match Kerabit::new(title).load_scene(path) {
        Ok(k) => k,
        Err(err) => {
            eprintln!("kerabit-editor play: {err}");
            std::process::exit(1);
        }
    };

    kerabit.run(|ctx| {
        if ctx.input().key_pressed(Key::Escape) {
            ctx.quit();
        }
        if let Some(err) = ctx.script_error().map(str::to_owned) {
            ctx.ui().text(0.02, 0.03, 0.022, Color::rgb(1.0, 0.35, 0.3), &err);
        }
    });
}
