//! Strike — compact first-person arena.
//!
//! ```bash
//! cargo run -p strike
//! ```
//!
//! WASD move, mouse look (click the window), left-click hitscan, Space jump /
//! start, R retry, Esc quit. Hostile dummies chase and return fire.

use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;
use std::path::PathBuf;

use kerabit::prelude::*;
use kerabit::ColliderId;

const EYE_HEIGHT: f32 = 1.6;
const LOOK_SENS: f32 = 0.0022;
const MOVE_SPEED: f32 = 6.5;
const FIRE_COOLDOWN: f32 = 0.18;
const PITCH_LIMIT: f32 = FRAC_PI_2 - 0.12;
const PLAYER_HALF: Vec3 = Vec3::new(0.35, 0.9, 0.35);
/// Sit slightly above the ground slab so the AABB is not already overlapping.
const PLAYER_SPAWN_Y: f32 = PLAYER_HALF.y + 0.06;
const PLAYER_HP: i32 = 5;

const ENEMY_HALF: Vec3 = Vec3::new(0.38, 0.85, 0.38);
const ENEMY_HP: i32 = 2;
const ENEMY_SPEED: f32 = 3.4;
const PATROL_SPEED: f32 = 1.55;
const AGGRO_RANGE: f32 = 13.0;
const STOP_RANGE: f32 = 2.4;
const SHOOT_RANGE: f32 = 15.0;
const ENEMY_FIRE_CD: f32 = 0.9;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Title,
    Play,
    Win,
    Fail,
}

struct Enemy {
    name: String,
    collider: Option<ColliderId>,
    controller: CharacterController,
    health: i32,
    fire_cd: f32,
    home: Vec3,
    phase_off: f32,
    alert: bool,
}

struct Game {
    scene_path: PathBuf,
    phase: Phase,
    yaw: f32,
    pitch: f32,
    controller: CharacterController,
    colliders: HashMap<ColliderId, String>,
    player_collider: Option<ColliderId>,
    enemies: Vec<Enemy>,
    physics_ready: bool,
    fire_cd: f32,
    hp: i32,
    hurt_t: f32,
    elapsed: f32,
    score: u32,
    spawn: Vec3,
}

fn scene_path() -> PathBuf {
    kerabit::packaged_data_root("scenes", env!("CARGO_MANIFEST_DIR"))
        .join("scenes/arena.kerabit.json")
}

fn look_forward(yaw: f32, pitch: f32) -> Vec3 {
    Vec3::new(
        -yaw.sin() * pitch.cos(),
        pitch.sin(),
        -yaw.cos() * pitch.cos(),
    )
}

/// Horizontal right when looking with `yaw` (yaw 0 = −Z, right = +X).
fn look_right(yaw: f32) -> Vec3 {
    Vec3::new(yaw.cos(), 0.0, -yaw.sin())
}

/// Yaw so [`look_forward`]`(yaw, 0)` matches horizontal `dir`.
fn yaw_facing(dir: Vec3) -> f32 {
    let n = Vec3::new(dir.x, 0.0, dir.z);
    if n.length_squared() < 1e-8 {
        return 0.0;
    }
    let n = n.normalize();
    (-n.x).atan2(-n.z)
}

fn wish_from_keys(input: &InputState, yaw: f32) -> Vec3 {
    let forward = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    let right = look_right(yaw);
    let mut wish = Vec3::ZERO;
    if input.key_down(Key::W) || input.key_down(Key::Up) {
        wish += forward;
    }
    if input.key_down(Key::S) || input.key_down(Key::Down) {
        wish -= forward;
    }
    if input.key_down(Key::D) || input.key_down(Key::Right) {
        wish += right;
    }
    if input.key_down(Key::A) || input.key_down(Key::Left) {
        wish -= right;
    }
    wish
}

fn player_aabb(center: Vec3) -> Aabb {
    Aabb::from_center_half_extents(center, PLAYER_HALF)
}

fn enemy_aabb(center: Vec3) -> Aabb {
    Aabb::from_center_half_extents(center, ENEMY_HALF)
}

fn collect_static_bodies(world: &World) -> Vec<(String, Vec<String>, Vec3, Vec3)> {
    world
        .iter()
        .filter_map(|e| {
            let name = e.name()?.to_string();
            Some((
                name,
                e.tags().to_vec(),
                e.transform.translation(),
                e.transform.scale(),
            ))
        })
        .collect()
}

fn register_statics(ctx: &mut Context, colliders: &mut HashMap<ColliderId, String>) {
    colliders.clear();
    let bodies = collect_static_bodies(ctx.world());
    for (name, tags, pos, scale) in bodies {
        let ground = tags.iter().any(|t| t == "ground");
        let wall = tags.iter().any(|t| t == "wall");
        let cover = tags.iter().any(|t| t == "cover");
        if !ground && !wall && !cover {
            continue;
        }
        let aabb = if ground {
            Aabb::from_center_half_extents(Vec3::new(pos.x, -0.5, pos.z), Vec3::new(8.0, 0.5, 8.0))
        } else if name.starts_with("crate") {
            let half = Vec3::new(0.5, 0.5, 0.5);
            Aabb::from_center_half_extents(pos + Vec3::Y * half.y, half)
        } else if name.starts_with("barrel") {
            let half = Vec3::new(0.44, 0.53, 0.4);
            Aabb::from_center_half_extents(pos + Vec3::Y * half.y, half)
        } else {
            Aabb::from_center_half_extents(pos, (scale * 0.5).abs().max(Vec3::splat(0.08)))
        };
        let id = ctx.physics().add_aabb(aabb);
        colliders.insert(id, name);
    }
}

fn spawn_enemies(world: &World) -> Vec<Enemy> {
    let mut enemies = Vec::new();
    for id in world.entities_with_tag("enemy") {
        let Some(e) = world.get_by_id(id) else {
            continue;
        };
        let Some(name) = e.name().map(str::to_string) else {
            continue;
        };
        let feet = e.transform.translation();
        let center = Vec3::new(feet.x, ENEMY_HALF.y, feet.z);
        let phase_off = (enemies.len() as f32) * 1.17;
        enemies.push(Enemy {
            name,
            collider: None,
            controller: CharacterController::planar(center, ENEMY_HALF).with_max_speed(ENEMY_SPEED),
            health: ENEMY_HP,
            fire_cd: phase_off * 0.2,
            home: center,
            phase_off,
            alert: false,
        });
    }
    enemies
}

fn bind_enemy_colliders(ctx: &mut Context, game: &mut Game) {
    for enemy in &mut game.enemies {
        if let Some(id) = enemy.collider {
            ctx.physics().remove(id);
            game.colliders.remove(&id);
        }
        let id = ctx.physics().add_aabb(enemy_aabb(enemy.controller.position));
        game.colliders.insert(id, enemy.name.clone());
        enemy.collider = Some(id);
    }
}

fn drop_player_collider(ctx: &mut Context, game: &mut Game) {
    if let Some(id) = game.player_collider.take() {
        ctx.physics().remove(id);
        game.colliders.remove(&id);
    }
}

fn sync_player_collider(ctx: &mut Context, game: &mut Game) {
    let aabb = player_aabb(game.controller.position);
    if let Some(id) = game.player_collider {
        if ctx.physics().set_aabb(id, aabb) {
            return;
        }
    }
    let id = ctx.physics().add_aabb(aabb);
    game.colliders.insert(id, "player".into());
    game.player_collider = Some(id);
}

fn los_to_player(
    ctx: &mut Context,
    colliders: &HashMap<ColliderId, String>,
    from: Vec3,
    player_pos: Vec3,
    skip: &str,
) -> bool {
    let delta = player_pos - from;
    let dist = delta.length();
    if dist < 0.05 {
        return true;
    }
    let dir = delta / dist;
    let Some(hit) = ctx.physics().raycast(from, dir, dist + 0.15) else {
        return true;
    };
    match colliders.get(&hit.collider).map(String::as_str) {
        Some("player") => true,
        Some(name) if name == skip => {
            // Shouldn't happen if we dropped our own collider; treat as blocked.
            false
        }
        _ => false,
    }
}

fn pose_weapon(ctx: &mut Context, eye: Vec3, yaw: f32, pitch: f32) {
    let forward = look_forward(yaw, pitch);
    let right = look_right(yaw);
    let pos = eye + forward * 0.42 + right * 0.22 - Vec3::Y * 0.22;
    let rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-pitch);
    if let Some(weapon) = ctx.world_mut().get_mut("weapon") {
        weapon.transform.set_translation(pos);
        weapon.transform.set_rotation(rot);
    }
}

fn pose_enemy(ctx: &mut Context, enemy: &Enemy) {
    let feet = Vec3::new(enemy.controller.position.x, 0.0, enemy.controller.position.z);
    let yaw = yaw_facing(enemy.controller.velocity);
    // Prefer last velocity; if standing, keep current rotation.
    if let Some(e) = ctx.world_mut().get_mut(&enemy.name) {
        e.transform.set_translation(feet);
        if enemy.controller.velocity.length_squared() > 0.04 {
            e.transform.set_rotation(Quat::from_rotation_y(yaw));
        }
    }
}

fn face_player(ctx: &mut Context, name: &str, from: Vec3, player: Vec3) {
    let yaw = yaw_facing(player - from);
    if let Some(e) = ctx.world_mut().get_mut(name) {
        e.transform.set_rotation(Quat::from_rotation_y(yaw));
    }
}

fn kill_enemy(game: &mut Game, ctx: &mut Context, name: &str, point: Vec3) {
    ctx.spawn_particles(ParticleBurst {
        origin: point,
        count: 36,
        color: Color::rgb(1.0, 0.35, 0.15),
        size: 0.1,
        speed: 5.0,
        lifetime: 0.45,
        velocity: Vec3::Y * 2.0,
        spread: 0.95,
    });
    if let Some(idx) = game.enemies.iter().position(|e| e.name == name) {
        if let Some(id) = game.enemies[idx].collider {
            ctx.physics().remove(id);
            game.colliders.remove(&id);
        }
        game.enemies.swap_remove(idx);
    }
    let _ = ctx.despawn(name);
    game.score += 1;
}

fn damage_enemy(game: &mut Game, ctx: &mut Context, name: &str, point: Vec3) {
    let Some(enemy) = game.enemies.iter_mut().find(|e| e.name == name) else {
        return;
    };
    enemy.alert = true;
    enemy.health -= 1;
    let dead = enemy.health <= 0;
    ctx.spawn_particles(ParticleBurst {
        origin: point,
        count: if dead { 36 } else { 18 },
        color: Color::rgb(1.0, 0.35, 0.15),
        size: 0.08,
        speed: 4.5,
        lifetime: 0.4,
        velocity: Vec3::Y * 2.0,
        spread: 0.9,
    });
    if dead {
        kill_enemy(game, ctx, name, point);
    }
}

fn tick_enemies(game: &mut Game, ctx: &mut Context, dt: f32) {
    let player = game.controller.position;
    let player_eye = Vec3::new(player.x, player.y - PLAYER_HALF.y + EYE_HEIGHT, player.z);
    game.elapsed += dt;

    let names: Vec<String> = game.enemies.iter().map(|e| e.name.clone()).collect();
    for name in names {
        let Some(idx) = game.enemies.iter().position(|e| e.name == name) else {
            continue;
        };
        if let Some(id) = game.enemies[idx].collider.take() {
            ctx.physics().remove(id);
            game.colliders.remove(&id);
        }

        let pos = game.enemies[idx].controller.position;
        let to_player = Vec3::new(player.x - pos.x, 0.0, player.z - pos.z);
        let dist = to_player.length();
        let chest = pos + Vec3::Y * 0.45;
        let sees = los_to_player(ctx, &game.colliders, chest, player_eye, &name);
        if sees && dist < AGGRO_RANGE {
            game.enemies[idx].alert = true;
        }

        let wish = if game.enemies[idx].alert {
            if dist > STOP_RANGE {
                to_player / dist
            } else {
                Vec3::ZERO
            }
        } else {
            let t = game.elapsed * 0.7 + game.enemies[idx].phase_off;
            let dest = game.enemies[idx].home
                + Vec3::new(t.sin() * 1.7, 0.0, (t * 0.73).cos() * 1.7);
            let delta = Vec3::new(dest.x - pos.x, 0.0, dest.z - pos.z);
            if delta.length() > 0.2 {
                delta.normalize()
            } else {
                Vec3::ZERO
            }
        };

        let speed = if game.enemies[idx].alert {
            ENEMY_SPEED
        } else {
            PATROL_SPEED
        };
        game.enemies[idx]
            .controller
            .move_planar(ctx.physics(), wish, speed, dt);

        game.enemies[idx].fire_cd = (game.enemies[idx].fire_cd - dt).max(0.0);
        let muzzle = game.enemies[idx].controller.position + Vec3::Y * 0.45;
        let should_shoot = game.enemies[idx].alert
            && dist < SHOOT_RANGE
            && game.enemies[idx].fire_cd <= 0.0
            && los_to_player(ctx, &game.colliders, muzzle, player_eye, &name);

        if should_shoot {
            game.enemies[idx].fire_cd = ENEMY_FIRE_CD;
            let delta = player_eye - muzzle;
            let dir = if delta.length_squared() > 1e-6 {
                delta.normalize()
            } else {
                Vec3::NEG_Z
            };
            ctx.spawn_particles(ParticleBurst {
                origin: muzzle + dir * 0.35,
                count: 8,
                color: Color::rgb(1.0, 0.55, 0.2),
                size: 0.05,
                speed: 3.5,
                lifetime: 0.16,
                velocity: dir * 2.5,
                spread: 0.3,
            });
            if let Some(hit) = ctx.physics().raycast(muzzle, dir, 40.0) {
                if game.colliders.get(&hit.collider).map(String::as_str) == Some("player") {
                    game.hp -= 1;
                    game.hurt_t = 0.22;
                    ctx.spawn_particles(ParticleBurst {
                        origin: hit.point,
                        count: 14,
                        color: Color::rgb(0.9, 0.15, 0.12),
                        size: 0.07,
                        speed: 2.8,
                        lifetime: 0.28,
                        velocity: -dir,
                        spread: 0.7,
                    });
                }
            }
        }

        let id = ctx
            .physics()
            .add_aabb(enemy_aabb(game.enemies[idx].controller.position));
        game.colliders.insert(id, name.clone());
        game.enemies[idx].collider = Some(id);

        if game.enemies[idx].alert {
            face_player(ctx, &name, game.enemies[idx].controller.position, player);
        } else {
            pose_enemy(ctx, &game.enemies[idx]);
        }
    }
}

fn reset_round(game: &mut Game, ctx: &mut Context) {
    let _ = ctx.load_scene(&game.scene_path);
    ctx.world_mut().set_enabled_named("player", false);
    game.spawn = ctx
        .world()
        .get("player")
        .map(|e| e.transform.translation())
        .unwrap_or(vec3(-5.0, PLAYER_SPAWN_Y, 5.0));
    game.spawn.y = PLAYER_SPAWN_Y;
    game.controller = CharacterController::new(game.spawn, PLAYER_HALF)
        .with_max_speed(MOVE_SPEED)
        .with_jump_speed(7.2)
        .with_gravity(24.0);
    game.yaw = 0.0;
    game.pitch = 0.0;
    game.physics_ready = false;
    game.fire_cd = 0.0;
    game.hp = PLAYER_HP;
    game.hurt_t = 0.0;
    game.elapsed = 0.0;
    game.score = 0;
    game.colliders.clear();
    game.player_collider = None;
    game.enemies.clear();
}

fn draw_crosshair(ui: &mut Ui) {
    let c = Color::rgba(0.95, 0.95, 0.9, 0.85);
    ui.rect(0.497, 0.475, 0.006, 0.05, c);
    ui.rect(0.478, 0.494, 0.044, 0.012, c);
}

fn hud_text(ctx: &mut Context, game: &Game) {
    let remaining = game.enemies.len();
    match game.phase {
        Phase::Title => {
            ctx.ui()
                .rect(0.0, 0.0, 1.0, 1.0, Color::rgba(0.02, 0.03, 0.04, 0.45));
            ctx.ui()
                .text(0.36, 0.32, 0.07, Color::rgb(0.95, 0.82, 0.35), "STRIKE");
            ctx.ui().text(
                0.18,
                0.44,
                0.022,
                Color::rgb(0.85, 0.85, 0.8),
                "WASD move   mouse look   click shoot",
            );
            ctx.ui().text(
                0.20,
                0.50,
                0.022,
                Color::rgb(0.75, 0.72, 0.65),
                "dummies patrol, chase, and shoot back",
            );
            ctx.ui().text(
                0.26,
                0.56,
                0.022,
                Color::rgb(0.75, 0.75, 0.7),
                "Space start   R retry   Esc quit",
            );
        }
        Phase::Play => {
            if game.hurt_t > 0.0 {
                let a = (game.hurt_t / 0.22).clamp(0.0, 1.0) * 0.35;
                ctx.ui()
                    .rect(0.0, 0.0, 1.0, 1.0, Color::rgba(0.6, 0.05, 0.04, a));
            }
            draw_crosshair(ctx.ui());
            ctx.ui()
                .text(0.03, 0.03, 0.028, Color::rgb(0.95, 0.82, 0.35), "STRIKE");
            ctx.ui().text(
                0.03,
                0.08,
                0.02,
                Color::rgb(0.85, 0.82, 0.75),
                &format!(
                    "hp {}   foes {}   score {}",
                    game.hp, remaining, game.score
                ),
            );
        }
        Phase::Win => {
            ctx.ui()
                .rect(0.0, 0.0, 1.0, 1.0, Color::rgba(0.05, 0.1, 0.06, 0.55));
            ctx.ui()
                .text(0.36, 0.36, 0.07, Color::rgb(0.91, 1.0, 0.29), "CLEAR");
            ctx.ui().text(
                0.28,
                0.48,
                0.024,
                Color::rgb(0.9, 0.9, 0.85),
                &format!("score {}   R retry   Esc quit", game.score),
            );
        }
        Phase::Fail => {
            ctx.ui()
                .rect(0.0, 0.0, 1.0, 1.0, Color::rgba(0.18, 0.06, 0.05, 0.55));
            ctx.ui()
                .text(0.36, 0.36, 0.07, Color::rgb(1.0, 0.35, 0.35), "DOWN");
            ctx.ui().text(
                0.26,
                0.48,
                0.024,
                Color::rgb(0.9, 0.82, 0.76),
                "R retry   Esc quit",
            );
        }
    }
}

fn fire(game: &mut Game, ctx: &mut Context, eye: Vec3, forward: Vec3) {
    game.fire_cd = FIRE_COOLDOWN;
    ctx.spawn_particles(ParticleBurst {
        origin: eye + forward * 0.5,
        count: 10,
        color: Color::rgb(1.0, 0.85, 0.4),
        size: 0.04,
        speed: 4.0,
        lifetime: 0.18,
        velocity: forward * 3.0,
        spread: 0.25,
    });

    let Some(hit) = ctx.physics().raycast(eye, forward, 80.0) else {
        return;
    };
    let Some(name) = game.colliders.get(&hit.collider).cloned() else {
        return;
    };
    if name == "player" || name == "weapon" {
        return;
    }

    if game.enemies.iter().any(|e| e.name == name) {
        damage_enemy(game, ctx, &name, hit.point);
    } else {
        ctx.spawn_particles(ParticleBurst {
            origin: hit.point,
            count: 12,
            color: Color::rgb(0.85, 0.75, 0.45),
            size: 0.06,
            speed: 2.2,
            lifetime: 0.25,
            velocity: hit.normal,
            spread: 0.6,
        });
    }
}

fn main() {
    let scene_path = scene_path();
    if let Some(root) = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
    {
        let _ = std::env::set_current_dir(root);
    }
    let mut game = Game {
        scene_path: scene_path.clone(),
        phase: Phase::Title,
        yaw: 0.0,
        pitch: 0.0,
        controller: CharacterController::new(vec3(-5.0, PLAYER_SPAWN_Y, 5.0), PLAYER_HALF)
            .with_max_speed(MOVE_SPEED)
            .with_jump_speed(7.2)
            .with_gravity(24.0),
        colliders: HashMap::new(),
        player_collider: None,
        enemies: Vec::new(),
        physics_ready: false,
        fire_cd: 0.0,
        hp: PLAYER_HP,
        hurt_t: 0.0,
        elapsed: 0.0,
        score: 0,
        spawn: vec3(-5.0, PLAYER_SPAWN_Y, 5.0),
    };

    Kerabit::new("Strike")
        .load_scene(&scene_path)
        .expect("strike arena scene")
        .run(move |ctx| {
            if ctx.input().key_pressed(Key::Escape) {
                ctx.quit();
                return;
            }

            if !game.physics_ready {
                ctx.world_mut().set_enabled_named("player", false);
                if let Some(p) = ctx.world().get("player") {
                    let t = p.transform.translation();
                    game.spawn = Vec3::new(t.x, PLAYER_SPAWN_Y, t.z);
                    game.controller.position = game.spawn;
                }
                register_statics(ctx, &mut game.colliders);
                game.enemies = spawn_enemies(ctx.world());
                bind_enemy_colliders(ctx, &mut game);
                game.physics_ready = true;
            }

            let (dx, dy) = ctx.input().mouse_delta();
            game.yaw -= dx * LOOK_SENS;
            game.pitch = (game.pitch - dy * LOOK_SENS).clamp(-PITCH_LIMIT, PITCH_LIMIT);
            game.hurt_t = (game.hurt_t - ctx.dt()).max(0.0);

            if ctx.input().key_pressed(Key::R) && game.phase != Phase::Title {
                reset_round(&mut game, ctx);
                game.phase = Phase::Play;
                return;
            }

            match game.phase {
                Phase::Title => {
                    if ctx.input().key_pressed(Key::Space)
                        || ctx.input().mouse_button_pressed(MouseButton::Left)
                    {
                        game.phase = Phase::Play;
                    }
                }
                Phase::Play => {
                    let wish = wish_from_keys(ctx.input(), game.yaw);
                    let jump = ctx.input().key_pressed(Key::Space);
                    let dt = ctx.dt();
                    // Hurtbox must not be in the world during move_wish — overlapping
                    // our own AABB depenetrates ~0.7m/frame and yeets us off the map.
                    drop_player_collider(ctx, &mut game);
                    game.controller.move_wish(ctx.physics(), wish, jump, dt);
                    sync_player_collider(ctx, &mut game);

                    tick_enemies(&mut game, ctx, dt);

                    game.fire_cd = (game.fire_cd - dt).max(0.0);
                    if game.fire_cd <= 0.0 && ctx.input().mouse_button_pressed(MouseButton::Left)
                    {
                        let forward = look_forward(game.yaw, game.pitch);
                        let eye = Vec3::new(
                            game.controller.position.x,
                            game.controller.position.y - PLAYER_HALF.y + EYE_HEIGHT,
                            game.controller.position.z,
                        );
                        fire(&mut game, ctx, eye, forward);
                    }

                    if game.hp <= 0 {
                        game.phase = Phase::Fail;
                    } else if game.enemies.is_empty() {
                        game.phase = Phase::Win;
                    }
                }
                Phase::Win | Phase::Fail => {}
            }

            let forward = look_forward(game.yaw, game.pitch);
            let eye = Vec3::new(
                game.controller.position.x,
                game.controller.position.y - PLAYER_HALF.y + EYE_HEIGHT,
                game.controller.position.z,
            );
            let target = eye + forward;
            *ctx.camera_mut() = ctx.camera().clone().look_at(eye, target);
            pose_weapon(ctx, eye, game.yaw, game.pitch);

            hud_text(ctx, &game);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standing_on_ground_stays_in_arena() {
        let mut phys = PhysicsWorld::new();
        phys.add_aabb(Aabb::from_center_half_extents(
            vec3(0.0, -0.5, 0.0),
            vec3(8.0, 0.5, 8.0),
        ));
        let mut cc = CharacterController::new(vec3(-5.0, PLAYER_SPAWN_Y, 5.0), PLAYER_HALF)
            .with_max_speed(MOVE_SPEED)
            .with_gravity(24.0);
        for _ in 0..120 {
            cc.move_wish(&phys, Vec3::ZERO, false, 1.0 / 60.0);
        }
        assert!(
            cc.position.x.abs() < 7.5 && cc.position.z.abs() < 7.5,
            "slid out of arena: {:?}",
            cc.position
        );
        assert!(
            cc.position.y > 0.5 && cc.position.y < 2.5,
            "fell or launched on Y: {}",
            cc.position.y
        );
    }

    #[test]
    fn look_forward_default_is_neg_z() {
        let f = look_forward(0.0, 0.0);
        assert!((f.x).abs() < 1e-5);
        assert!((f.y).abs() < 1e-5);
        assert!((f.z + 1.0).abs() < 1e-5);
    }

    #[test]
    fn yaw_facing_matches_look_forward() {
        let dir = Vec3::new(0.6, 0.0, -0.8).normalize();
        let yaw = yaw_facing(dir);
        let f = look_forward(yaw, 0.0);
        assert!((f.x - dir.x).abs() < 1e-4);
        assert!((f.z - dir.z).abs() < 1e-4);
    }

    #[test]
    fn d_strafes_positive_x_when_looking_neg_z() {
        let mut input = InputState::new();
        input.set_key(Key::D, true);
        let wish = wish_from_keys(&input, 0.0);
        assert!(
            wish.x > 0.5,
            "D should strafe +X (right) when looking −Z, got {wish:?}"
        );
        assert!(wish.z.abs() < 1e-5);
    }

    #[test]
    fn a_strafes_negative_x_when_looking_neg_z() {
        let mut input = InputState::new();
        input.set_key(Key::A, true);
        let wish = wish_from_keys(&input, 0.0);
        assert!(
            wish.x < -0.5,
            "A should strafe −X (left) when looking −Z, got {wish:?}"
        );
    }

    #[test]
    fn arena_gltf_assets_load() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root")
            .to_path_buf();
        let _ = std::env::set_current_dir(&root);
        let scene = Scene::load(scene_path()).expect("arena json");
        let entities = scene.build_entities().expect("gltf meshes");
        assert!(entities.len() >= 10);
        assert_eq!(
            scene
                .entities
                .iter()
                .filter(|e| e.has_tag("enemy"))
                .count(),
            5
        );
    }
}
