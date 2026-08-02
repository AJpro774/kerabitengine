//! Kerabit Summit showcase — non-game engine trailer.
//!
//! Visual proof of M1 render: PBR-lite materials, multi-light, tonemap/bloom,
//! and particle bursts. Orbit period is **20s** so marketing loops close cleanly.
//!
//! ```bash
//! cargo run -p showcase
//! KERABIT_SHOWCASE_RECORD=1 cargo run -p showcase --release   # no HUD; auto-quit ~22s
//! ```

use kerabit::prelude::*;
use std::env;
use std::f32::consts::TAU;

/// Marketing loop length — camera angle returns to start after this many seconds.
const LOOP_SECS: f32 = 20.0;

fn main() {
    let record = env::var_os("KERABIT_SHOWCASE_RECORD").is_some()
        || env::args().any(|a| a == "--record");

    let frames_dir = env::var_os("KERABIT_CAPTURE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("target/showcase-frames"));

    let mut app = Kerabit::new("Kerabit Showcase")
        .clear_color(Color::rgb(0.035, 0.038, 0.055))
        .ambient(Color::rgb(0.07, 0.08, 0.1))
        .camera(Camera::perspective(52.0).look_at(vec3(5.2, 2.8, 6.2), vec3(0.0, 0.7, 0.0)))
        .lights([
            Light::sun(vec3(-0.45, -1.0, -0.25))
                .intensity(1.4)
                .color(Color::rgb(1.0, 0.96, 0.9)),
            Light::point(vec3(1.8, 2.0, 0.6))
                .intensity(2.4)
                .color(Color::rgb(1.0, 0.55, 0.22))
                .range(9.0),
            Light::point(vec3(-2.0, 1.6, -0.8))
                .intensity(1.8)
                .color(Color::rgb(0.3, 0.5, 1.0))
                .range(8.0),
            Light::point(vec3(0.2, 1.2, 2.2))
                .intensity(1.2)
                .color(Color::rgb(0.55, 1.0, 0.7))
                .range(6.0),
        ]);

    if record {
        // 1280×720 keeps the encode under the site size budget while looking sharp.
        app = app
            .window_size(1280, 720)
            .capture_frames(&frames_dir);
        eprintln!(
            "showcase: recording ~20s → {} (30fps PNG sequence)",
            frames_dir.display()
        );
    }

    app.spawn(
            Entity::new("floor")
                .mesh(Mesh::plane(14.0))
                .material(Material::color(Color::rgb(0.18, 0.19, 0.22)).roughness(0.88))
                .at(Vec3::ZERO),
        )
        .spawn(
            Entity::new("back_wall")
                .mesh(Mesh::cube())
                .material(Material::color(Color::rgb(0.14, 0.15, 0.18)).roughness(0.95))
                .at(Vec3::new(0.0, 1.6, -3.4))
                .scale(Vec3::new(7.0, 3.2, 0.18)),
        )
        .spawn(
            Entity::new("side_wall")
                .mesh(Mesh::cube())
                .material(Material::color(Color::rgb(0.16, 0.17, 0.2)).roughness(0.92))
                .at(Vec3::new(-3.5, 1.4, -0.8))
                .scale(Vec3::new(0.18, 2.8, 5.0)),
        )
        .spawn(
            Entity::new("plinth")
                .mesh(Mesh::cube())
                .material(
                    Material::color(Color::rgb(0.28, 0.29, 0.32))
                        .roughness(0.55)
                        .metallic(0.15),
                )
                .at(Vec3::new(0.0, 0.2, 0.0))
                .scale(Vec3::new(4.2, 0.4, 2.4)),
        )
        .spawn(
            Entity::new("dielectric")
                .mesh(Mesh::cube())
                .material(
                    Material::color(Color::rgb(0.88, 0.18, 0.14))
                        .roughness(0.22)
                        .metallic(0.0),
                )
                .at(Vec3::new(-1.5, 0.85, 0.0)),
        )
        .spawn(
            Entity::new("brushed_metal")
                .mesh(Mesh::cube())
                .material(
                    Material::color(Color::rgb(0.92, 0.9, 0.85))
                        .roughness(0.32)
                        .metallic(1.0),
                )
                .at(Vec3::new(0.0, 0.85, 0.0)),
        )
        .spawn(
            Entity::new("chrome")
                .mesh(Mesh::cube())
                .material(
                    Material::color(Color::rgb(0.95, 0.96, 0.98))
                        .roughness(0.06)
                        .metallic(1.0),
                )
                .at(Vec3::new(1.5, 0.85, 0.0)),
        )
        .spawn(
            Entity::new("gloss_plastic")
                .mesh(Mesh::cube())
                .material(
                    Material::color(Color::rgb(0.12, 0.52, 0.95))
                        .roughness(0.1)
                        .metallic(0.0),
                )
                .at(Vec3::new(0.0, 0.85, 1.35)),
        )
        .spawn(
            Entity::new("accent_orb")
                .mesh(Mesh::cube())
                .material(
                    Material::color(Color::rgb(1.0, 0.75, 0.25))
                        .roughness(0.18)
                        .metallic(0.4),
                )
                .at(Vec3::new(1.8, 2.0, 0.6))
                .scale(Vec3::splat(0.28)),
        )
        .run({
            let mut t = 0.0_f32;
            let mut last_warm = -1.0_f32;
            let mut last_cool = -1.0_f32;
            let mut last_dust = -1.0_f32;
            move |ctx| {
                let dt = ctx.dt();
                t += dt;
                if ctx.input().key_pressed(Key::Escape) {
                    ctx.quit();
                }
                // Auto-quit after one loop + pad so capture tools can stop cleanly.
                if record && t >= LOOP_SECS + 2.0 {
                    ctx.quit();
                }

                let loop_t = t % LOOP_SECS;
                let phase = loop_t / LOOP_SECS; // 0..1
                let angle = phase * TAU;
                let radius = 6.4;
                let height = 2.6 + (phase * TAU).sin() * 0.25;
                let eye = Vec3::new(angle.cos() * radius, height, angle.sin() * radius);
                let target = Vec3::new(0.0, 0.75, 0.0);
                *ctx.camera_mut() = Camera::perspective(52.0).look_at(eye, target);

                for name in ["dielectric", "brushed_metal", "chrome", "gloss_plastic"] {
                    if let Some(e) = ctx.world_mut().get_mut(name) {
                        e.rotate_y(0.4 * dt);
                    }
                }
                if let Some(orb) = ctx.world_mut().get_mut("accent_orb") {
                    let bob = 2.0 + (phase * TAU * 2.0).sin() * 0.15;
                    orb.transform.set_translation(Vec3::new(
                        1.8,
                        bob,
                        0.6 + (phase * TAU).cos() * 0.1,
                    ));
                    orb.rotate_y(1.2 * dt);
                }

                // Deterministic particle beats within the 20s loop.
                let warm_beat = (loop_t / 0.28).floor();
                if warm_beat != last_warm {
                    last_warm = warm_beat;
                    ctx.spawn_particles(ParticleBurst {
                        origin: Vec3::new(1.8, 1.9, 0.6),
                        count: 22,
                        color: Color::rgb(1.0, 0.65, 0.25),
                        size: 0.07,
                        speed: 1.5,
                        lifetime: 0.6,
                        velocity: Vec3::Y * 0.5,
                        spread: 1.1,
                    });
                }
                let cool_beat = ((loop_t + 0.12) / 0.45).floor();
                if cool_beat != last_cool {
                    last_cool = cool_beat;
                    ctx.spawn_particles(ParticleBurst {
                        origin: Vec3::new(-2.0, 1.5, -0.8),
                        count: 14,
                        color: Color::rgb(0.4, 0.6, 1.0),
                        size: 0.06,
                        speed: 1.1,
                        lifetime: 0.7,
                        velocity: Vec3::Y * 0.35,
                        spread: 0.9,
                    });
                }
                let dust_beat = (loop_t / 4.0).floor();
                if dust_beat != last_dust {
                    last_dust = dust_beat;
                    ctx.spawn_particles(ParticleBurst {
                        origin: Vec3::new(0.0, 0.15, 0.0),
                        count: 28,
                        color: Color::rgb(0.7, 0.75, 0.85),
                        size: 0.05,
                        speed: 1.8,
                        lifetime: 0.85,
                        velocity: Vec3::ZERO,
                        spread: 1.4,
                    });
                }

                let pulse = 1.0 + (phase * TAU).sin() * 0.25;
                ctx.set_lights([
                    Light::sun(vec3(-0.45, -1.0, -0.25))
                        .intensity(1.4)
                        .color(Color::rgb(1.0, 0.96, 0.9)),
                    Light::point(vec3(1.8, 2.0, 0.6))
                        .intensity(2.4)
                        .color(Color::rgb(1.0, 0.55, 0.22))
                        .range(9.0),
                    Light::point(vec3(-2.0, 1.6, -0.8))
                        .intensity(1.8)
                        .color(Color::rgb(0.3, 0.5, 1.0))
                        .range(8.0),
                    Light::point(vec3(0.2, 1.2, 2.2))
                        .intensity(1.2 * pulse)
                        .color(Color::rgb(0.55, 1.0, 0.7))
                        .range(6.0),
                ]);

                if !record {
                    ctx.ui()
                        .rect(0.0, 0.0, 1.0, 0.14, Color::rgba(0.02, 0.02, 0.04, 0.45));
                    ctx.ui()
                        .text(0.04, 0.035, 0.045, Color::rgb(0.95, 0.92, 0.88), "KERABIT");
                    ctx.ui().text(
                        0.04,
                        0.09,
                        0.024,
                        Color::rgb(0.7, 0.72, 0.8),
                        "Summit showcase  ·  PBR-lite · multi-light · bloom · particles",
                    );
                    ctx.ui().text(
                        0.04,
                        0.94,
                        0.02,
                        Color::rgb(0.55, 0.58, 0.65),
                        "20s orbit loop  ·  Escape quit",
                    );
                }
            }
        });
}
