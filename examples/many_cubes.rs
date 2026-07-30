//! Kerabit P4 stress — ~1000 instanced cubes + hierarchy child.
//!
//! ```bash
//! cargo run -p kerabit --example many_cubes --release
//! ```
//!
//! Escape quits. Right-drag orbits; WASD + Q/E move.

use kerabit::prelude::*;

const GRID: i32 = 10; // 10×10×10 = 1000 cubes

fn main() {
    let mut builder = Kerabit::new("Kerabit — many cubes")
        .clear_color(Color::rgb(0.06, 0.07, 0.09))
        .spawn(
            Entity::new("ground")
                .mesh(Mesh::plane(40.0))
                .material(Material::color(Color::GRAY).roughness(0.95))
                .at(Vec3::ZERO),
        )
        .spawn(
            Entity::new("pivot")
                .mesh(Mesh::cube())
                .material(Material::color(Color::ORANGE).roughness(0.25))
                .at(Vec3::new(0.0, 0.5, 0.0)),
        )
        // Child follows the rotating pivot (local offset on +X).
        .spawn(
            Entity::new("orbiter")
                .mesh(Mesh::cube())
                .material(Material::color(Color::rgb(0.35, 0.75, 1.0)).roughness(0.15))
                .at(Vec3::new(1.4, 0.0, 0.0))
                .parent("pivot"),
        )
        .camera(Camera::perspective(60.0).look_at(vec3(14.0, 10.0, 18.0), Vec3::ZERO))
        .light(Light::sun(vec3(-0.4, -1.0, -0.2)).intensity(1.15))
        .ambient(Color::rgb(0.12, 0.13, 0.15));

    let spacing = 1.15;
    let origin = -((GRID - 1) as f32) * spacing * 0.5;
    for z in 0..GRID {
        for y in 0..GRID {
            for x in 0..GRID {
                let i = x + y * GRID + z * GRID * GRID;
                let px = origin + x as f32 * spacing;
                let py = 1.2 + y as f32 * spacing;
                let pz = origin + z as f32 * spacing;
                let t = i as f32 * 0.017;
                let color = Color::rgb(
                    0.35 + 0.55 * (t * 0.7).sin() * 0.5 + 0.5,
                    0.35 + 0.55 * (t * 1.1 + 1.0).sin() * 0.5 + 0.5,
                    0.40 + 0.45 * (t * 0.9 + 2.0).cos() * 0.5 + 0.5,
                );
                let roughness = 0.2 + 0.7 * ((x + y + z) % 5) as f32 / 4.0;
                builder = builder.spawn(
                    Entity::new(format!("c{i}"))
                        .mesh(Mesh::cube())
                        .material(Material::color(color).roughness(roughness))
                        .at(Vec3::new(px, py, pz)),
                );
            }
        }
    }

    builder.run(|ctx| {
        let dt = ctx.dt();

        if ctx.input().key_pressed(Key::Escape) {
            ctx.quit();
        }

        if let Some(pivot) = ctx.world_mut().get_mut("pivot") {
            pivot.rotate_y(0.9 * dt);
        }

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
            cam.eye = cam.target + Vec3::new(yaw.sin() * cp, pitch.sin(), yaw.cos() * cp) * radius;
        }

        let speed = 12.0 * dt;
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
    });
}
