//! Load an external mesh (OBJ or glTF) and render it lit.
//!
//! ```bash
//! cargo run -p kerabit --example load_mesh
//! ```
//!
//! Escape quits. Right-drag orbits; WASD + Q/E move.

use std::path::PathBuf;

use kerabit::prelude::*;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../kerabit-assets/fixtures")
}

fn main() {
    let fixtures = fixtures_dir();

    // Prefer glTF (mesh + base color texture). Fall back to OBJ + PNG.
    let (mesh, material) = match load_gltf(fixtures.join("box.gltf")) {
        Ok(pair) => pair,
        Err(err) => {
            eprintln!("kerabit: glTF load failed ({err}); falling back to OBJ+PNG");
            let mesh = Mesh::load_obj(fixtures.join("box.obj")).expect("load box.obj");
            let material = Material::load_png(fixtures.join("checker.png"))
                .expect("load checker.png")
                .roughness(0.45);
            (mesh, material)
        }
    };

    Kerabit::new("Kerabit Load Mesh")
        .clear_color(Color::rgb(0.08, 0.09, 0.12))
        .spawn(
            Entity::new("model")
                .mesh(mesh)
                .material(material.roughness(0.4))
                .at(Vec3::new(0.0, 0.5, 0.0)),
        )
        .spawn(
            Entity::new("ground")
                .mesh(Mesh::plane(20.0))
                .material(Material::color(Color::GRAY).roughness(0.9))
                .at(Vec3::ZERO),
        )
        .camera(Camera::perspective(60.0).look_at(vec3(4.0, 2.5, 5.5), Vec3::ZERO))
        .light(Light::sun(vec3(-0.35, -1.0, -0.25)).intensity(1.2))
        .ambient(Color::rgb(0.15, 0.16, 0.18))
        .run(|ctx| {
            let dt = ctx.dt();

            if ctx.input().key_pressed(Key::Escape) {
                ctx.quit();
            }

            if let Some(model) = ctx.world_mut().get_mut("model") {
                model.rotate_y(0.7 * dt);
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
