//! M1 PBR room — metallic/roughness spheres, multi-light, bloom, particles.
//!
//! ```bash
//! cargo run -p kerabit --example pbr_room
//! ```

use kerabit::prelude::*;

fn main() {
    Kerabit::new("Kerabit PBR Room")
        .clear_color(Color::rgb(0.04, 0.045, 0.06))
        .ambient(Color::rgb(0.08, 0.09, 0.11))
        .camera(Camera::perspective(55.0).look_at(vec3(4.5, 2.4, 5.5), vec3(0.0, 0.6, 0.0)))
        .lights([
            Light::sun(vec3(-0.4, -1.0, -0.2)).intensity(1.35).color(Color::rgb(1.0, 0.96, 0.9)),
            Light::point(vec3(1.6, 1.8, 0.4))
                .intensity(2.2)
                .color(Color::rgb(1.0, 0.55, 0.25))
                .range(8.0),
            Light::point(vec3(-1.8, 1.4, -0.6))
                .intensity(1.6)
                .color(Color::rgb(0.35, 0.55, 1.0))
                .range(7.0),
        ])
        .spawn(
            Entity::new("floor")
                .mesh(Mesh::plane(12.0))
                .material(Material::color(Color::rgb(0.22, 0.23, 0.25)).roughness(0.85))
                .at(Vec3::ZERO),
        )
        .spawn(
            Entity::new("back_wall")
                .mesh(Mesh::cube())
                .material(Material::color(Color::rgb(0.18, 0.19, 0.22)).roughness(0.95))
                .at(Vec3::new(0.0, 1.5, -3.0))
                .scale(Vec3::new(6.0, 3.0, 0.15)),
        )
        .spawn(
            Entity::new("dielectric")
                .mesh(Mesh::cube())
                .material(
                    Material::color(Color::rgb(0.85, 0.2, 0.15))
                        .roughness(0.25)
                        .metallic(0.0),
                )
                .at(Vec3::new(-1.4, 0.5, 0.0)),
        )
        .spawn(
            Entity::new("brushed_metal")
                .mesh(Mesh::cube())
                .material(
                    Material::color(Color::rgb(0.92, 0.9, 0.85))
                        .roughness(0.35)
                        .metallic(1.0),
                )
                .at(Vec3::new(0.0, 0.5, 0.0)),
        )
        .spawn(
            Entity::new("chrome")
                .mesh(Mesh::cube())
                .material(
                    Material::color(Color::rgb(0.95, 0.95, 0.98))
                        .roughness(0.08)
                        .metallic(1.0),
                )
                .at(Vec3::new(1.4, 0.5, 0.0)),
        )
        .spawn(
            Entity::new("gloss_plastic")
                .mesh(Mesh::cube())
                .material(
                    Material::color(Color::rgb(0.15, 0.55, 0.95))
                        .roughness(0.12)
                        .metallic(0.0),
                )
                .at(Vec3::new(0.0, 0.5, 1.5)),
        )
        .run({
            let mut sparkle = 0.0_f32;
            move |ctx| {
            let dt = ctx.dt();
            if ctx.input().key_pressed(Key::Escape) {
                ctx.quit();
            }

            for name in ["dielectric", "brushed_metal", "chrome", "gloss_plastic"] {
                if let Some(e) = ctx.world_mut().get_mut(name) {
                    e.rotate_y(0.35 * dt);
                }
            }

            sparkle += dt;
            if sparkle > 0.35 {
                sparkle = 0.0;
                ctx.spawn_particles(ParticleBurst {
                    origin: Vec3::new(1.6, 1.6, 0.4),
                    count: 18,
                    color: Color::rgb(1.0, 0.7, 0.3),
                    size: 0.08,
                    speed: 1.4,
                    lifetime: 0.55,
                    velocity: Vec3::Y * 0.4,
                    spread: 1.0,
                });
            }

            ctx.ui().text(0.02, 0.02, 0.035, Color::WHITE, "PBR Room (M1)");
            ctx.ui().text(
                0.02,
                0.07,
                0.025,
                Color::GRAY,
                "sun + 2 points  |  tonemap/bloom  |  particles",
            );
            }
        });
}
