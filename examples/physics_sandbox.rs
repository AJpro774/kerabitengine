//! Physics sandbox (M2) — dynamics, character controller, tags.
//!
//! WASD + Space (jump) moves the green player. Orange boxes fall under gravity.
//! Escape quits.
//!
//! ```bash
//! cargo run -p kerabit --example physics_sandbox
//! ```

use kerabit::prelude::*;

fn main() {
    let floor_half = Vec3::new(8.0, 0.25, 8.0);
    let floor_center = Vec3::new(0.0, -0.25, 0.0);
    let mut player = CharacterController::new(Vec3::new(-2.0, 1.0, 0.0), Vec3::new(0.4, 0.9, 0.4))
        .with_max_speed(6.0)
        .with_jump_speed(8.0)
        .with_gravity(22.0);
    let mut ready = false;
    let mut crate_ids: Vec<(String, BodyId)> = Vec::new();

    Kerabit::new("Kerabit Physics Sandbox")
        .clear_color(Color::rgb(0.07, 0.08, 0.11))
        .spawn(
            Entity::new("player")
                .mesh(Mesh::cube())
                .material(Material::color(Color::rgb(0.35, 0.85, 0.45)).roughness(0.4))
                .at(player.position)
                .scale(Vec3::new(0.8, 1.8, 0.8))
                .tag("player"),
        )
        .spawn(
            Entity::new("floor")
                .mesh(Mesh::cube())
                .material(Material::color(Color::GRAY).roughness(0.95))
                .at(floor_center)
                .scale(floor_half * 2.0)
                .tag("ground"),
        )
        .spawn(
            Entity::new("crate_a")
                .mesh(Mesh::cube())
                .material(Material::color(Color::ORANGE).roughness(0.55))
                .at(Vec3::new(1.5, 4.0, 0.5))
                .tag("dynamic"),
        )
        .spawn(
            Entity::new("crate_b")
                .mesh(Mesh::cube())
                .material(Material::color(Color::rgb(0.95, 0.55, 0.2)).roughness(0.5))
                .at(Vec3::new(2.2, 6.0, -0.4))
                .tag("dynamic"),
        )
        .spawn(
            Entity::new("wall")
                .mesh(Mesh::cube())
                .material(Material::color(Color::rgb(0.45, 0.5, 0.6)).roughness(0.7))
                .at(Vec3::new(4.0, 1.0, 0.0))
                .scale(Vec3::new(0.5, 2.0, 4.0))
                .tag("wall"),
        )
        .camera(Camera::perspective(60.0).look_at(vec3(0.0, 5.0, 12.0), Vec3::new(0.0, 1.0, 0.0)))
        .light(Light::sun(vec3(-0.4, -1.0, -0.3)).intensity(1.25))
        .ambient(Color::rgb(0.14, 0.15, 0.18))
        .run(move |ctx| {
            if ctx.input().key_pressed(Key::Escape) {
                ctx.quit();
                return;
            }

            if !ready {
                ctx.physics()
                    .add_aabb(Aabb::from_center_half_extents(floor_center, floor_half));
                ctx.physics().add_aabb(Aabb::from_center_half_extents(
                    Vec3::new(4.0, 1.0, 0.0),
                    Vec3::new(0.25, 1.0, 2.0),
                ));

                for name in ["crate_a", "crate_b"] {
                    let pos = ctx
                        .world()
                        .get(name)
                        .map(|e| e.transform.translation())
                        .unwrap_or(Vec3::ZERO);
                    let id = ctx.physics().add_dynamic(
                        DynamicBody::aabb(pos, Vec3::splat(0.5), 1.0).with_restitution(0.15),
                    );
                    crate_ids.push((name.to_string(), id));
                }
                ready = true;
            }

            let dt = ctx.dt();
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
            let jump = ctx.input().key_pressed(Key::Space);
            player.move_wish(ctx.physics(), wish, jump, dt);

            if let Some(e) = ctx.world_mut().get_mut("player") {
                e.transform.set_translation(player.position);
            }

            ctx.physics().step(dt);
            let crate_positions: Vec<(String, Vec3)> = crate_ids
                .iter()
                .filter_map(|(name, id)| {
                    ctx.physics()
                        .get_body(*id)
                        .map(|b| (name.clone(), b.position))
                })
                .collect();
            for (name, pos) in crate_positions {
                if let Some(e) = ctx.world_mut().get_mut(&name) {
                    e.transform.set_translation(pos);
                }
            }

            // Toggle wall visibility with T (enable/disable query demo).
            if ctx.input().key_pressed(Key::T) {
                if let Some(wall) = ctx.world_mut().get_mut("wall") {
                    wall.set_enabled(!wall.is_enabled());
                }
            }

            let dynamic_count = ctx.world().entities_with_tag("dynamic").len();
            ctx.ui().text(
                0.02,
                0.02,
                0.028,
                Color::WHITE,
                &format!(
                    "WASD move  Space jump  T toggle wall\ngrounded={}  dynamics={}",
                    player.grounded, dynamic_count
                ),
            );
        });
}
