//! Mini-game vertical slice (P7) — reach the cyan pad, avoid red hazards.
//!
//! **Legacy demo.** Prefer the flagship game: `cargo run -p reach`.
//!
//! Loads a checked-in `.kerabit.json` scene, then runs a short playable loop
//! using **only** the public Kerabit API. Win/fail are terminal messages (no HUD).
//!
//! **Controls**
//! - WASD — move
//! - R — restart after win / fail (or mid-run)
//! - Escape — quit
//!
//! **Goal:** touch the cyan pad. **Fail:** touch a red hazard or leave the platform.
//!
//! ```bash
//! cargo run -p kerabit --example mini_game
//! ```

use std::path::PathBuf;

use kerabit::prelude::*;

fn scene_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/scenes/mini_game.kerabit.json")
}

fn beep_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/assets/beep.wav")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Playing,
    Won,
    Failed,
}

struct Level {
    player_start: Vec3,
    player_half: Vec3,
    walls: Vec<(Vec3, Vec3)>,
    hazards: Vec<(Vec3, Vec3)>,
    goal_center: Vec3,
    goal_half: Vec3,
    /// Half-extents of the safe platform on XZ (leave → fail).
    platform_half: Vec2,
}

fn main() {
    let path = scene_path();
    let scene = Scene::load(&path).unwrap_or_else(|err| {
        eprintln!("kerabit: failed to load scene {}: {err}", path.display());
        std::process::exit(1);
    });

    let level = Level::from_scene(&scene);
    let mut player_pos = level.player_start;
    let mut phase = Phase::Playing;
    let mut physics_ready = false;
    let mut status_logged = false;

    println!("Kerabit mini_game");
    println!("  Reach the cyan pad. Avoid red hazards. Stay on the platform.");
    println!("  WASD move · R restart · Escape quit");

    scene
        .into_kerabit("Kerabit Mini Game")
        .unwrap_or_else(|err| {
            eprintln!("kerabit: {err}");
            std::process::exit(1);
        })
        .run(move |ctx| {
            if ctx.input().key_pressed(Key::Escape) {
                ctx.quit();
                return;
            }

            if !physics_ready {
                for (center, half) in &level.walls {
                    ctx.physics()
                        .add_aabb(Aabb::from_center_half_extents(*center, *half));
                }
                physics_ready = true;
            }

            if ctx.input().key_pressed(Key::R) {
                player_pos = level.player_start;
                phase = Phase::Playing;
                status_logged = false;
                if let Some(player) = ctx.world_mut().get_mut("player") {
                    player.transform.set_translation(player_pos);
                }
                println!("Restart!");
            }

            match phase {
                Phase::Playing => {
                    let dt = ctx.dt();
                    let speed = 5.0;
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

                    let result = ctx.physics().move_and_collide(
                        player_pos,
                        velocity,
                        level.player_half,
                        dt,
                    );
                    player_pos = result.position;
                    player_pos.y = level.player_start.y;

                    if let Some(player) = ctx.world_mut().get_mut("player") {
                        player.transform.set_translation(player_pos);
                    }

                    let player_aabb =
                        Aabb::from_center_half_extents(player_pos, level.player_half);

                    let off_platform = player_pos.x.abs() > level.platform_half.x
                        || player_pos.z.abs() > level.platform_half.y;
                    let hit_hazard = level.hazards.iter().any(|(c, h)| {
                        player_aabb.overlaps(Aabb::from_center_half_extents(*c, *h))
                    });
                    let hit_goal = player_aabb
                        .overlaps(Aabb::from_center_half_extents(level.goal_center, level.goal_half));

                    if hit_hazard || off_platform {
                        phase = Phase::Failed;
                        play_beep(ctx);
                    } else if hit_goal {
                        phase = Phase::Won;
                        play_beep(ctx);
                    }
                }
                Phase::Won => {
                    let dt = ctx.dt();
                    if let Some(goal) = ctx.world_mut().get_mut("goal") {
                        goal.rotate_y(2.5 * dt);
                    }
                    if !status_logged {
                        println!("You win! Press R to play again, Escape to quit.");
                        status_logged = true;
                    }
                }
                Phase::Failed => {
                    if !status_logged {
                        println!("Failed — hazard or off the platform. Press R to retry.");
                        status_logged = true;
                    }
                }
            }
        });
}

fn play_beep(ctx: &mut Context<'_>) {
    let path = beep_path();
    if let Err(err) = ctx.audio().play(&path) {
        eprintln!("kerabit audio: {err}");
    }
}

impl Level {
    fn from_scene(scene: &Scene) -> Self {
        let mut player_start = Vec3::new(-5.0, 0.5, 0.0);
        let mut walls = Vec::new();
        let mut hazards = Vec::new();
        let mut goal_center = Vec3::new(5.0, 0.3, 0.0);
        let mut goal_half = Vec3::new(0.8, 0.3, 0.8);

        for e in &scene.entities {
            let half = e.scale * 0.5;
            match e.name.as_str() {
                "player" => player_start = e.at,
                "goal" => {
                    goal_center = e.at;
                    goal_half = half;
                }
                name if name.starts_with("wall_") => {
                    walls.push((e.at, half));
                }
                name if name.starts_with("hazard_") => {
                    hazards.push((e.at, half));
                }
                _ => {}
            }
        }

        Self {
            player_start,
            player_half: Vec3::splat(0.5),
            walls,
            hazards,
            goal_center,
            goal_half,
            // Matches plane size 14 → half edge 7, with a small margin.
            platform_half: Vec2::new(6.6, 6.6),
        }
    }
}
