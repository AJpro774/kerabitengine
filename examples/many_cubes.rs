//! Kerabit stress — ~10k instanced cubes + hierarchy child + 200 clustered
//! point lights (the 3.0 render-tier perf gate).
//!
//! ```bash
//! cargo run -p kerabit --example many_cubes --release
//! ```
//!
//! Escape quits. Right-drag orbits; WASD + Q/E move. `L` toggles the light
//! swarm. Average frame time prints every 2 seconds.

use kerabit::prelude::*;

const GRID: i32 = 22; // 22×22×22 = 10_648 cubes
const POINT_LIGHTS: usize = 200;

fn swarm_lights(t: f32) -> Vec<Light> {
    let mut lights = vec![Light::sun(vec3(-0.4, -1.0, -0.2)).intensity(1.15)];
    for i in 0..POINT_LIGHTS {
        let f = i as f32;
        let ring = 6.0 + (f * 0.37).sin() * 3.0 + (i % 5) as f32 * 2.2;
        let angle = f * 0.7 + t * (0.15 + (i % 3) as f32 * 0.07);
        let y = 2.0 + ((f * 0.53 + t * 0.6).sin() * 0.5 + 0.5) * 20.0;
        let color = Color::rgb(
            0.5 + 0.5 * (f * 0.9).sin(),
            0.5 + 0.5 * (f * 1.3 + 2.0).sin(),
            0.5 + 0.5 * (f * 0.6 + 4.0).sin(),
        );
        lights.push(
            Light::point(vec3(angle.cos() * ring, y, angle.sin() * ring))
                .color(color)
                .intensity(6.0)
                .range(5.5),
        );
    }
    lights
}

fn main() {
    let mut builder = Kerabit::new("Kerabit — many cubes")
        .clear_color(Color::rgb(0.06, 0.07, 0.09))
        .spawn(
            Entity::new("ground")
                .mesh(Mesh::plane(60.0))
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
        .camera(Camera::perspective(60.0).look_at(vec3(28.0, 20.0, 36.0), Vec3::ZERO))
        .light(Light::sun(vec3(-0.4, -1.0, -0.2)).intensity(1.15))
        .ambient(Color::rgb(0.12, 0.13, 0.15));

    let spacing = 1.05;
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

    let mut elapsed = 0.0f32;
    let mut swarm = true;
    let mut frame_acc = (0.0f32, 0u32);
    builder.run(move |ctx| {
        let dt = ctx.dt();
        elapsed += dt;

        if ctx.input().key_pressed(Key::Escape) {
            ctx.quit();
        }
        if ctx.input().key_pressed(Key::L) {
            swarm = !swarm;
        }
        if swarm {
            ctx.set_lights(swarm_lights(elapsed));
        } else {
            ctx.set_lights([Light::sun(vec3(-0.4, -1.0, -0.2)).intensity(1.15)]);
        }

        frame_acc.0 += dt;
        frame_acc.1 += 1;
        if frame_acc.0 >= 2.0 {
            let ms = frame_acc.0 / frame_acc.1 as f32 * 1000.0;
            eprintln!(
                "many_cubes: {:.2} ms/frame ({:.0} fps), lights={}",
                ms,
                1000.0 / ms,
                if swarm { POINT_LIGHTS + 1 } else { 1 }
            );
            frame_acc = (0.0, 0);
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

        let speed = 18.0 * dt;
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
