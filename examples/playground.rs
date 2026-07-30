//! Kerabit playground — lit ground + rotating cube, WASD / orbit camera, Escape to quit.
//!
//! ```bash
//! cargo run -p kerabit --example playground
//! ```

use kerabit::prelude::*;

fn main() {
    Kerabit::new("Kerabit Playground")
        .clear_color(Color::rgb(0.08, 0.09, 0.12))
        .spawn(
            Entity::new("cube")
                .mesh(Mesh::cube())
                .material(Material::color(Color::ORANGE).roughness(0.35))
                .at(Vec3::new(0.0, 0.5, 0.0)),
        )
        // Child offset follows the rotating cube (hierarchy smoke).
        .spawn(
            Entity::new("satellite")
                .mesh(Mesh::cube())
                .material(Material::color(Color::rgb(0.4, 0.7, 1.0)).roughness(0.2))
                .at(Vec3::new(1.25, 0.0, 0.0))
                .parent("cube"),
        )
        .spawn(
            Entity::new("ground")
                .mesh(Mesh::plane(40.0))
                .material(Material::color(Color::GRAY).roughness(0.9))
                .at(Vec3::ZERO),
        )
        .camera(Camera::perspective(60.0).look_at(vec3(5.0, 3.0, 7.0), Vec3::ZERO))
        .light(Light::sun(vec3(-0.35, -1.0, -0.25)).intensity(1.2))
        .ambient(Color::rgb(0.15, 0.16, 0.18))
        .run(|ctx| {
            let dt = ctx.dt();

            if ctx.input().key_pressed(Key::Escape) {
                ctx.quit();
            }

            if let Some(cube) = ctx.world_mut().get_mut("cube") {
                cube.rotate_y(1.1 * dt);
            }

            // Orbit with right-mouse drag; pan/dolly with WASD + Q/E.
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
        });
}
