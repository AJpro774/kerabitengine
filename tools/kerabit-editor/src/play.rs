//! Play mode via the public Kerabit API (`Scene` → `Kerabit::run`).
//!
//! Invoked as `kerabit-editor --play <path.kerabit.json>` so the editor shell
//! (eframe) and the play window (winit) never share one event loop — true
//! in-viewport play is not feasible without merging event loops. Escape or
//! closing the window exits this process; the parent editor restores selection.

use std::path::Path;

use kerabit::{Color, Key, MouseButton, Scene, Vec3};

/// Load `path` and open a Kerabit play window until Escape / close.
pub fn run(path: &Path) {
    let scene = match Scene::load(path) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("kerabit-editor play: failed to load {}: {err}", path.display());
            std::process::exit(1);
        }
    };

    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("scene");
    let title = format!("Kerabit Play — {name}");

    let kerabit = match scene.into_kerabit(title) {
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

        ctx.ui().text(
            16.0,
            16.0,
            18.0,
            Color::WHITE,
            "Playing — Esc / close to return (selection kept in editor)",
        );
    });
}
