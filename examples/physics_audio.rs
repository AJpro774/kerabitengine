//! Physics + audio smoke demo (P6 / M3).
//!
//! WASD moves the green player cube; walking into the orange wall blocks.
//! Press Space to play a short beep at the player (spatial). Escape quits.
//!
//! ```bash
//! cargo run -p kerabit --example physics_audio
//! ```

use std::path::PathBuf;

use kerabit::prelude::*;

fn beep_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/assets/beep.wav")
}

fn main() {
    let wall_center = Vec3::new(2.0, 0.5, 0.0);
    let wall_half = Vec3::new(0.5, 0.5, 1.5);
    let player_half = Vec3::new(0.5, 0.5, 0.5);
    let mut player_pos = Vec3::new(-2.0, 0.5, 0.0);
    let mut wall_registered = false;

    Kerabit::new("Kerabit Physics + Audio")
        .clear_color(Color::rgb(0.08, 0.09, 0.12))
        .spawn(
            Entity::new("player")
                .mesh(Mesh::cube())
                .material(Material::color(Color::rgb(0.35, 0.85, 0.45)).roughness(0.4))
                .at(player_pos),
        )
        .spawn(
            Entity::new("wall")
                .mesh(Mesh::cube())
                .material(Material::color(Color::ORANGE).roughness(0.5))
                .at(wall_center),
        )
        .spawn(
            Entity::new("ground")
                .mesh(Mesh::plane(20.0))
                .material(Material::color(Color::GRAY).roughness(0.9))
                .at(Vec3::ZERO),
        )
        .camera(Camera::perspective(60.0).look_at(vec3(0.0, 4.0, 8.0), Vec3::new(0.0, 0.5, 0.0)))
        .light(Light::sun(vec3(-0.35, -1.0, -0.25)).intensity(1.2))
        .ambient(Color::rgb(0.15, 0.16, 0.18))
        .run(move |ctx| {
            ctx.sync_audio_listener();

            if ctx.input().key_pressed(Key::Escape) {
                ctx.quit();
                return;
            }

            if !wall_registered {
                // Unit cube scaled to match wall_half (edge = 2 * half).
                if let Some(wall) = ctx.world_mut().get_mut("wall") {
                    wall.transform.set_scale(wall_half * 2.0);
                }
                ctx.physics()
                    .add_aabb(Aabb::from_center_half_extents(wall_center, wall_half));
                wall_registered = true;
            }

            if ctx.input().key_pressed(Key::Space) {
                let path = beep_path();
                if let Err(err) = ctx.audio().play_at(&path, player_pos) {
                    eprintln!("kerabit audio: {err}");
                }
            }

            let dt = ctx.dt();
            let speed = 4.0;
            let mut wish = Vec3::ZERO;
            if ctx.input().key_down(Key::W) {
                wish.z -= 1.0;
            }
            if ctx.input().key_down(Key::S) {
                wish.z += 1.0;
            }
            if ctx.input().key_down(Key::A) {
                wish.x -= 1.0;
            }
            if ctx.input().key_down(Key::D) {
                wish.x += 1.0;
            }
            let velocity = if wish.length_squared() > 0.0 {
                wish.normalize() * speed
            } else {
                Vec3::ZERO
            };

            let result = ctx
                .physics()
                .move_and_collide(player_pos, velocity, player_half, dt);
            player_pos = result.position;
            player_pos.y = 0.5;

            if let Some(player) = ctx.world_mut().get_mut("player") {
                player.transform.set_translation(player_pos);
            }
        });
}
