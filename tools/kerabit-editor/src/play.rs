//! Play mode via the public Kerabit API (`Scene` → `Kerabit::run`).
//!
//! Invoked as `kerabit-editor --play <path.kerabit.json>` so the editor shell
//! (eframe) and the play window (winit) never share one event loop — true
//! in-viewport play is not feasible without merging event loops. Escape or
//! closing the window exits this process; the parent editor restores selection.

use std::path::Path;

use kerabit::{Color, Kerabit, Key, MouseButton, Vec3};

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
            return;
        }

        if let Some(err) = ctx.script_error().map(str::to_owned) {
            ctx.ui().text(
                0.02,
                0.09,
                0.024,
                Color::rgb(1.0, 0.35, 0.3),
                &err,
            );
        }

        // Light orbit / pan so authors can inspect the lit scene.
        let dt = ctx.dt();
        let (mdx, mdy) = ctx.input().mouse_delta();
        let orbit = ctx.input().mouse_button_down(MouseButton::Right);
        let move_w = ctx.input().key_down(Key::W);
        let move_s = ctx.input().key_down(Key::S);
        let move_d = ctx.input().key_down(Key::D);
        let move_a = ctx.input().key_down(Key::A);
        let move_e = ctx.input().key_down(Key::E);
        let move_q = ctx.input().key_down(Key::Q);

        let cam = ctx.camera_mut();
        if orbit {
            let sens = 0.005;
            let offset = cam.eye - cam.target;
            let radius = offset.length().max(0.5);
            let mut yaw = offset.x.atan2(offset.z);
            let mut pitch = (offset.y / radius).asin();
            yaw -= mdx * sens;
            pitch = (pitch + mdy * sens).clamp(-1.4, 1.4);
            let cp = pitch.cos();
            cam.eye = cam.target
                + Vec3::new(yaw.sin() * cp, pitch.sin(), yaw.cos() * cp) * radius;
        }

        let speed = 6.0 * dt;
        let forward = (cam.target - cam.eye).normalize_or_zero();
        let right = forward.cross(cam.up).normalize_or_zero();
        let mut move_dir = Vec3::ZERO;
        if move_w {
            move_dir += forward;
        }
        if move_s {
            move_dir -= forward;
        }
        if move_d {
            move_dir += right;
        }
        if move_a {
            move_dir -= right;
        }
        if move_e {
            move_dir += cam.up;
        }
        if move_q {
            move_dir -= cam.up;
        }
        if move_dir.length_squared() > 0.0 {
            let delta = move_dir.normalize() * speed;
            cam.eye += delta;
            cam.target += delta;
        }

        // Corner HUD — site lime accent (normalized 0–1 coords).
        ctx.ui().rect(0.0, 0.0, 0.48, 0.07, Color::rgba(0.10, 0.09, 0.08, 0.72));
        ctx.ui().text(
            0.02,
            0.022,
            0.028,
            Color::rgb(0.91, 1.0, 0.29),
            "PLAY  -  Esc returns to editor (scripts auto-reload on save)",
        );
    });
}
