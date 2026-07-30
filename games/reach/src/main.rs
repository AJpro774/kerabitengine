//! Reach — Kerabit flagship micro-game.
//!
//! Title → play → clear / fail → next level or retry. HUD and juice via the
//! public Kerabit API only (`ctx.ui()`, camera lerp, squash, SFX).
//!
//! Level transitions use mid-run [`Context::apply_scene`] (same window / GPU /
//! EventLoop) — no App teardown between levels.
//!
//! **Controls**
//! - WASD — move
//! - Space — start / confirm / next level
//! - R — retry after fail (or mid-run)
//! - Escape — quit
//!
//! ```bash
//! cargo run -p reach
//! ```

use std::path::PathBuf;

use kerabit::prelude::*;
use kerabit::{SceneEntity, SceneMesh};

const LEVEL_FILES: &[&str] = &[
    "01_intro.kerabit.json",
    "02_bent.kerabit.json",
    "03_gauntlet.kerabit.json",
    "04_switchback.kerabit.json",
    "05_crossfire.kerabit.json",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Title,
    Playing,
    Won,
    Failed,
}

struct Hazard {
    name: String,
    center: Vec3,
    half: Vec3,
    base_scale: Vec3,
}

struct Level {
    player_start: Vec3,
    player_half: Vec3,
    player_base_scale: Vec3,
    walls: Vec<(Vec3, Vec3)>,
    hazards: Vec<Hazard>,
    goal_center: Vec3,
    goal_half: Vec3,
    goal_base_scale: Vec3,
    /// Half-extents of the safe platform on XZ (leave → fail).
    platform_half: Vec2,
    cam_eye_offset: Vec3,
    cam_height: f32,
}

/// Data root for `levels/` and `assets/`.
///
/// Resolution order (first hit with a `levels/` directory wins):
/// 1. macOS `.app` → `Contents/Resources` next to `Contents/MacOS/<exe>`
/// 2. Directory containing the executable (flat release zip)
/// 3. `CARGO_MANIFEST_DIR` (`cargo run` / `cargo test`)
fn root_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(mac_os) = exe.parent() {
            if mac_os
                .file_name()
                .is_some_and(|n| n == "MacOS")
            {
                let resources = mac_os.join("../Resources");
                if resources.join("levels").is_dir() {
                    return resources
                        .canonicalize()
                        .unwrap_or(resources);
                }
            }
            if mac_os.join("levels").is_dir() {
                return mac_os.to_path_buf();
            }
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn level_path(index: usize) -> PathBuf {
    root_dir().join("levels").join(LEVEL_FILES[index])
}

fn asset_path(name: &str) -> PathBuf {
    root_dir().join("assets").join(name)
}

fn play_sfx(ctx: &mut Context<'_>, name: &str) {
    let path = asset_path(name);
    if let Err(err) = ctx.audio().play(&path) {
        eprintln!("reach audio: {err}");
    }
}

fn text_width(s: &str, size: f32) -> f32 {
    s.chars().filter(|c| *c != '\n').count() as f32 * size
}

fn centered_x(s: &str, size: f32) -> f32 {
    (1.0 - text_width(s, size)) * 0.5
}

fn register_walls(ctx: &mut Context<'_>, level: &Level) {
    for (center, half) in &level.walls {
        ctx.physics()
            .add_aabb(Aabb::from_center_half_extents(*center, *half));
    }
}

fn begin_level_camera(
    level: &Level,
    player_pos: Vec3,
    cam_eye: &mut Vec3,
    cam_target: &mut Vec3,
) {
    *cam_eye = player_pos + level.cam_eye_offset;
    cam_eye.y = level.cam_height;
    *cam_target = Vec3::new(player_pos.x, 0.4, player_pos.z);
}

/// Load the next level JSON and apply it in-process (same window / GPU).
fn advance_level(
    ctx: &mut Context<'_>,
    level_index: &mut usize,
    level: &mut Level,
    player_pos: &mut Vec3,
    phase: &mut Phase,
    elapsed: &mut f32,
    squash: &mut f32,
    flash: &mut f32,
    velocity_xz: &mut Vec2,
    cam_eye: &mut Vec3,
    cam_target: &mut Vec3,
    physics_ready: &mut bool,
) -> bool {
    let next = *level_index + 1;
    if next >= LEVEL_FILES.len() {
        return false;
    }
    let path = level_path(next);
    let scene = match Scene::load(&path) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("reach: failed to load {}: {err}", path.display());
            return false;
        }
    };
    *level = Level::from_scene(&scene);
    if let Err(err) = ctx.apply_scene(&scene) {
        eprintln!("reach: apply_scene failed: {err}");
        return false;
    }
    *level_index = next;
    *player_pos = level.player_start;
    *phase = Phase::Playing;
    *elapsed = 0.0;
    *squash = 0.3;
    *flash = 0.0;
    *velocity_xz = Vec2::ZERO;
    begin_level_camera(level, *player_pos, cam_eye, cam_target);
    register_walls(ctx, level);
    *physics_ready = true;
    let cam = ctx.camera_mut();
    cam.eye = *cam_eye;
    cam.target = *cam_target;
    true
}

fn main() {
    let path = level_path(0);
    let scene = Scene::load(&path).unwrap_or_else(|err| {
        eprintln!("reach: failed to load {}: {err}", path.display());
        std::process::exit(1);
    });

    let mut level = Level::from_scene(&scene);
    let mut level_index = 0usize;

    let mut player_pos = level.player_start;
    let mut phase = Phase::Title;
    let mut physics_ready = false;
    let mut elapsed = 0.0f32;
    let mut time_alive = 0.0f32;
    let mut squash = 0.0f32;
    let mut flash = 0.0f32;
    let mut flash_color = Color::WHITE;
    let mut cam_eye = level.player_start + level.cam_eye_offset;
    let mut cam_target = level.player_start;
    cam_target.y = 0.4;
    let mut velocity_xz = Vec2::ZERO;
    let mut clear_time = 0.0f32;
    let mut all_clear = false;

    scene
        .into_kerabit("Reach")
        .unwrap_or_else(|err| {
            eprintln!("reach: {err}");
            std::process::exit(1);
        })
        .run(move |ctx| {
            time_alive += ctx.dt();
            let dt = ctx.dt();

            if ctx.input().key_pressed(Key::Escape) {
                ctx.quit();
                return;
            }

            if !physics_ready {
                register_walls(ctx, &level);
                physics_ready = true;
                let cam = ctx.camera_mut();
                cam.eye = cam_eye;
                cam.target = cam_target;
            }

            // Decay juice.
            squash = (squash - dt * 4.0).max(0.0);
            flash = (flash - dt * 2.2).max(0.0);

            match phase {
                Phase::Title => {
                    // Idle bob on title.
                    let bob = (time_alive * 2.2).sin() * 0.12;
                    player_pos = level.player_start;
                    player_pos.y = level.player_start.y + bob;
                    apply_player_visual(ctx, &level, player_pos, squash, 1.0);
                    spin_goal(ctx, dt * 0.6);
                    pulse_hazards(ctx, &level, time_alive);

                    follow_camera(
                        ctx,
                        &mut cam_eye,
                        &mut cam_target,
                        &level,
                        player_pos,
                        Vec2::ZERO,
                        dt,
                        2.5,
                    );

                    draw_flash(ctx, flash, flash_color);
                    ctx.ui().rect(
                        0.0,
                        0.0,
                        1.0,
                        1.0,
                        Color::rgba(0.02, 0.03, 0.06, 0.55),
                    );
                    let title_size = 0.09;
                    let title = "REACH";
                    ctx.ui().text(
                        centered_x(title, title_size),
                        0.36,
                        title_size,
                        Color::WHITE,
                        title,
                    );
                    let hint = "Press Space";
                    let hint_size = 0.035;
                    ctx.ui().text(
                        centered_x(hint, hint_size),
                        0.50,
                        hint_size,
                        Color::rgb(0.75, 0.78, 0.85),
                        hint,
                    );

                    if ctx.input().key_pressed(Key::Space) {
                        play_sfx(ctx, "ui.wav");
                        phase = Phase::Playing;
                        player_pos = level.player_start;
                        elapsed = 0.0;
                        squash = 0.35;
                    }
                }
                Phase::Playing => {
                    elapsed += dt;

                    if ctx.input().key_pressed(Key::R) {
                        reset_level(
                            ctx,
                            &level,
                            &mut player_pos,
                            &mut phase,
                            &mut elapsed,
                            &mut squash,
                            &mut flash,
                            &mut velocity_xz,
                        );
                        play_sfx(ctx, "ui.wav");
                    }

                    let speed = 5.2;
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
                    velocity_xz = Vec2::new(velocity.x, velocity.z);

                    let result = ctx.physics().move_and_collide(
                        player_pos,
                        velocity,
                        level.player_half,
                        dt,
                    );
                    player_pos = result.position;
                    player_pos.y = level.player_start.y;

                    if result.hit && velocity.length_squared() > 0.01 {
                        squash = squash.max(0.45);
                    }

                    apply_player_visual(ctx, &level, player_pos, squash, 1.0);
                    spin_goal(ctx, dt * 1.8);
                    pulse_hazards(ctx, &level, time_alive);
                    follow_camera(
                        ctx,
                        &mut cam_eye,
                        &mut cam_target,
                        &level,
                        player_pos,
                        velocity_xz,
                        dt,
                        6.0,
                    );

                    let player_aabb =
                        Aabb::from_center_half_extents(player_pos, level.player_half);
                    let off_platform = player_pos.x.abs() > level.platform_half.x
                        || player_pos.z.abs() > level.platform_half.y;
                    let hit_hazard = level.hazards.iter().any(|h| {
                        player_aabb
                            .overlaps(Aabb::from_center_half_extents(h.center, h.half))
                    });
                    let hit_goal = player_aabb.overlaps(Aabb::from_center_half_extents(
                        level.goal_center,
                        level.goal_half,
                    ));

                    if hit_hazard || off_platform {
                        phase = Phase::Failed;
                        squash = 0.85;
                        flash = 1.0;
                        flash_color = Color::rgb(0.95, 0.12, 0.15);
                        play_sfx(ctx, "fail.wav");
                    } else if hit_goal {
                        phase = Phase::Won;
                        clear_time = elapsed;
                        all_clear = level_index + 1 >= LEVEL_FILES.len();
                        squash = 0.7;
                        flash = 0.85;
                        flash_color = Color::rgb(0.2, 0.95, 0.85);
                        play_sfx(ctx, "win.wav");
                    }

                    draw_flash(ctx, flash, flash_color);
                    draw_hud(ctx, level_index, elapsed);
                }
                Phase::Won => {
                    spin_goal(ctx, dt * 3.2);
                    pulse_hazards(ctx, &level, time_alive);
                    apply_player_visual(ctx, &level, player_pos, squash, 1.15);
                    follow_camera(
                        ctx,
                        &mut cam_eye,
                        &mut cam_target,
                        &level,
                        player_pos,
                        Vec2::ZERO,
                        dt,
                        3.0,
                    );

                    draw_flash(ctx, flash, flash_color);
                    ctx.ui().rect(
                        0.0,
                        0.0,
                        1.0,
                        1.0,
                        Color::rgba(0.02, 0.08, 0.1, 0.45),
                    );

                    let headline = if all_clear {
                        "ALL CLEAR"
                    } else {
                        "CLEAR"
                    };
                    let hs = 0.08;
                    ctx.ui().text(
                        centered_x(headline, hs),
                        0.34,
                        hs,
                        Color::rgb(0.35, 1.0, 0.9),
                        headline,
                    );
                    let time_line = format!("Time {clear_time:.2}s");
                    let ts = 0.035;
                    ctx.ui().text(
                        centered_x(&time_line, ts),
                        0.46,
                        ts,
                        Color::WHITE,
                        &time_line,
                    );
                    let prompt = if all_clear {
                        "Space — done"
                    } else {
                        "Space — next level"
                    };
                    let ps = 0.03;
                    ctx.ui().text(
                        centered_x(prompt, ps),
                        0.56,
                        ps,
                        Color::rgb(0.75, 0.8, 0.88),
                        prompt,
                    );

                    if ctx.input().key_pressed(Key::Space) {
                        play_sfx(ctx, "ui.wav");
                        if all_clear {
                            ctx.quit();
                        } else if !advance_level(
                            ctx,
                            &mut level_index,
                            &mut level,
                            &mut player_pos,
                            &mut phase,
                            &mut elapsed,
                            &mut squash,
                            &mut flash,
                            &mut velocity_xz,
                            &mut cam_eye,
                            &mut cam_target,
                            &mut physics_ready,
                        ) {
                            ctx.quit();
                        }
                    }
                }
                Phase::Failed => {
                    apply_player_visual(ctx, &level, player_pos, squash, 0.85);
                    pulse_hazards(ctx, &level, time_alive);
                    follow_camera(
                        ctx,
                        &mut cam_eye,
                        &mut cam_target,
                        &level,
                        player_pos,
                        Vec2::ZERO,
                        dt,
                        3.0,
                    );

                    draw_flash(ctx, flash, flash_color);
                    ctx.ui().rect(
                        0.0,
                        0.0,
                        1.0,
                        1.0,
                        Color::rgba(0.25, 0.02, 0.04, 0.5),
                    );
                    let retry = "RETRY";
                    let rs = 0.09;
                    ctx.ui().text(
                        centered_x(retry, rs),
                        0.38,
                        rs,
                        Color::rgb(1.0, 0.35, 0.35),
                        retry,
                    );
                    let prompt = "Space / R";
                    let ps = 0.035;
                    ctx.ui().text(
                        centered_x(prompt, ps),
                        0.52,
                        ps,
                        Color::rgb(0.9, 0.85, 0.85),
                        prompt,
                    );

                    if ctx.input().key_pressed(Key::Space)
                        || ctx.input().key_pressed(Key::R)
                    {
                        play_sfx(ctx, "ui.wav");
                        reset_level(
                            ctx,
                            &level,
                            &mut player_pos,
                            &mut phase,
                            &mut elapsed,
                            &mut squash,
                            &mut flash,
                            &mut velocity_xz,
                        );
                    }
                }
            }
        });
}

fn reset_level(
    ctx: &mut Context<'_>,
    level: &Level,
    player_pos: &mut Vec3,
    phase: &mut Phase,
    elapsed: &mut f32,
    squash: &mut f32,
    flash: &mut f32,
    velocity_xz: &mut Vec2,
) {
    *player_pos = level.player_start;
    *phase = Phase::Playing;
    *elapsed = 0.0;
    *squash = 0.3;
    *flash = 0.0;
    *velocity_xz = Vec2::ZERO;
    apply_player_visual(ctx, level, *player_pos, *squash, 1.0);
    // Restore hazard rest poses.
    for h in &level.hazards {
        if let Some(ent) = ctx.world_mut().get_mut(&h.name) {
            ent.transform.set_translation(h.center);
            ent.transform.set_scale(h.base_scale);
        }
    }
    if let Some(goal) = ctx.world_mut().get_mut("goal") {
        goal.transform.set_translation(level.goal_center);
        goal.transform.set_scale(level.goal_base_scale);
        goal.transform.set_rotation(Quat::IDENTITY);
    }
}

fn apply_player_visual(
    ctx: &mut Context<'_>,
    level: &Level,
    pos: Vec3,
    squash: f32,
    stretch: f32,
) {
    if let Some(player) = ctx.world_mut().get_mut("player") {
        player.transform.set_translation(pos);
        let punch = squash.clamp(0.0, 1.0);
        let sx = level.player_base_scale.x * (1.0 + punch * 0.35) * stretch;
        let sy = level.player_base_scale.y * (1.0 - punch * 0.45) / stretch.max(0.01);
        let sz = level.player_base_scale.z * (1.0 + punch * 0.35) * stretch;
        player.transform.set_scale(Vec3::new(sx, sy, sz));
    }
}

fn spin_goal(ctx: &mut Context<'_>, rate: f32) {
    if let Some(goal) = ctx.world_mut().get_mut("goal") {
        goal.rotate_y(rate);
    }
}

fn pulse_hazards(ctx: &mut Context<'_>, level: &Level, t: f32) {
    for (i, h) in level.hazards.iter().enumerate() {
        if let Some(ent) = ctx.world_mut().get_mut(&h.name) {
            let phase = t * 3.0 + i as f32 * 1.3;
            let bob = phase.sin() * 0.08;
            let pulse = 1.0 + phase.cos() * 0.08;
            let mut p = h.center;
            p.y += bob;
            ent.transform.set_translation(p);
            ent.transform.set_scale(h.base_scale * pulse);
        }
    }
}

fn follow_camera(
    ctx: &mut Context<'_>,
    cam_eye: &mut Vec3,
    cam_target: &mut Vec3,
    level: &Level,
    player_pos: Vec3,
    velocity_xz: Vec2,
    dt: f32,
    speed: f32,
) {
    let look_ahead = Vec3::new(velocity_xz.x, 0.0, velocity_xz.y) * 0.35;
    let desired_target = Vec3::new(player_pos.x, 0.4, player_pos.z) + look_ahead;
    let desired_eye = desired_target + level.cam_eye_offset;
    let mut desired_eye = desired_eye;
    desired_eye.y = level.cam_height;

    let t = (1.0 - (-speed * dt).exp()).clamp(0.0, 1.0);
    *cam_eye = cam_eye.lerp(desired_eye, t);
    *cam_target = cam_target.lerp(desired_target, t);

    let cam = ctx.camera_mut();
    cam.eye = *cam_eye;
    cam.target = *cam_target;
}

fn draw_flash(ctx: &mut Context<'_>, flash: f32, color: Color) {
    if flash <= 0.001 {
        return;
    }
    let tint = Color::rgba(0.0, 0.0, 0.0, 0.0).lerp(
        Color::rgba(color.r, color.g, color.b, 0.5),
        flash,
    );
    ctx.ui().rect(0.0, 0.0, 1.0, 1.0, tint);
}

fn draw_hud(ctx: &mut Context<'_>, level_index: usize, elapsed: f32) {
    let level_line = format!("Level {}/{}", level_index + 1, LEVEL_FILES.len());
    let time_line = format!("{elapsed:.1}s");
    ctx.ui()
        .text(0.03, 0.03, 0.028, Color::rgb(0.9, 0.92, 0.95), &level_line);
    ctx.ui()
        .text(0.03, 0.07, 0.028, Color::rgb(0.75, 0.9, 0.95), &time_line);
    ctx.ui().text(
        0.03,
        0.94,
        0.022,
        Color::rgb(0.65, 0.68, 0.74),
        "WASD move  R retry  Esc quit",
    );
}

impl Level {
    fn from_scene(scene: &Scene) -> Self {
        let mut player_start = Vec3::new(-5.0, 0.5, 0.0);
        let mut player_base_scale = Vec3::ONE;
        let mut walls = Vec::new();
        let mut hazards = Vec::new();
        let mut goal_center = Vec3::new(5.0, 0.3, 0.0);
        let mut goal_half = Vec3::new(0.8, 0.3, 0.8);
        let mut goal_base_scale = Vec3::new(1.6, 0.6, 1.6);
        let mut platform_half = Vec2::new(6.6, 6.6);

        for e in &scene.entities {
            let half = e.scale * 0.5;
            if entity_has_role(e, "player") {
                player_start = e.at;
                player_base_scale = e.scale;
            } else if entity_has_role(e, "goal") {
                goal_center = e.at;
                goal_half = half;
                goal_base_scale = e.scale;
            } else if entity_has_role(e, "ground") {
                if let SceneMesh::Plane { size } = &e.mesh {
                    let edge = size * 0.5 - 0.4;
                    platform_half = Vec2::new(edge, edge);
                }
            } else if entity_has_role(e, "wall") {
                walls.push((e.at, half));
            } else if entity_has_role(e, "hazard") {
                hazards.push(Hazard {
                    name: e.name.clone(),
                    center: e.at,
                    half,
                    base_scale: e.scale,
                });
            }
        }

        let cam_eye_offset = Vec3::new(
            scene.camera.eye.x - scene.camera.target.x,
            0.0,
            scene.camera.eye.z - scene.camera.target.z,
        );

        Self {
            player_start,
            player_half: player_base_scale * 0.5,
            player_base_scale,
            walls,
            hazards,
            goal_center,
            goal_half,
            goal_base_scale,
            platform_half,
            cam_eye_offset,
            cam_height: scene.camera.eye.y,
        }
    }
}

/// Reach roles: prefer entity `tags`; fall back to legacy name conventions for one version.
fn entity_has_role(e: &SceneEntity, role: &str) -> bool {
    const KNOWN: &[&str] = &["player", "goal", "ground", "wall", "hazard"];
    let tagged = KNOWN.iter().any(|r| e.has_tag(r));
    if tagged {
        return e.has_tag(role);
    }
    match role {
        "player" | "goal" | "ground" => e.name == role,
        "wall" => e.name.starts_with("wall_"),
        "hazard" => e.name.starts_with("hazard_"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kerabit::{Color, Quat, SceneCamera, SceneLight, SceneMaterial};

    fn entity(name: &str, tags: &[&str], at: Vec3, scale: Vec3) -> SceneEntity {
        SceneEntity {
            name: name.into(),
            tags: tags.iter().map(|t| (*t).to_string()).collect(),
            mesh: SceneMesh::Cube,
            material: SceneMaterial {
                color: Color::WHITE,
                roughness: 0.5,
                texture: None,
            },
            at,
            rotation: Quat::IDENTITY,
            scale,
            parent: None,
        }
    }

    #[test]
    fn from_scene_uses_tags_without_name_prefixes() {
        let scene = Scene {
            clear_color: Color::BLACK,
            ambient: Color::WHITE,
            camera: SceneCamera {
                fov_y: 55.0,
                eye: Vec3::new(0.0, 9.0, 12.0),
                target: Vec3::new(0.0, 0.4, 0.0),
                near: 0.1,
                far: 100.0,
            },
            light: SceneLight {
                direction: Vec3::new(0.0, -1.0, 0.0),
                intensity: 1.0,
                color: Color::WHITE,
            },
            entities: vec![
                SceneEntity {
                    name: "floor".into(),
                    tags: vec!["ground".into()],
                    mesh: SceneMesh::Plane { size: 14.0 },
                    material: SceneMaterial {
                        color: Color::GRAY,
                        roughness: 0.9,
                        texture: None,
                    },
                    at: Vec3::ZERO,
                    rotation: Quat::IDENTITY,
                    scale: Vec3::ONE,
                    parent: None,
                },
                entity("hero", &["player"], Vec3::new(-5.0, 0.5, 0.0), Vec3::ONE),
                entity(
                    "south_barrier",
                    &["wall"],
                    Vec3::new(0.0, 0.75, 2.0),
                    Vec3::new(4.0, 1.5, 0.5),
                ),
                entity(
                    "spike",
                    &["hazard"],
                    Vec3::new(1.0, 0.45, 0.0),
                    Vec3::new(1.0, 0.9, 1.0),
                ),
                entity(
                    "exit",
                    &["goal"],
                    Vec3::new(5.0, 0.3, 0.0),
                    Vec3::new(1.6, 0.6, 1.6),
                ),
            ],
        };

        let level = Level::from_scene(&scene);
        assert_eq!(level.player_start, Vec3::new(-5.0, 0.5, 0.0));
        assert_eq!(level.walls.len(), 1);
        assert_eq!(level.hazards.len(), 1);
        assert_eq!(level.hazards[0].name, "spike");
        assert_eq!(level.goal_center, Vec3::new(5.0, 0.3, 0.0));
        assert!((level.platform_half.x - 6.6).abs() < 1e-3);
    }

    #[test]
    fn from_scene_falls_back_to_name_prefixes() {
        let scene = Scene {
            clear_color: Color::BLACK,
            ambient: Color::WHITE,
            camera: SceneCamera {
                fov_y: 55.0,
                eye: Vec3::new(0.0, 9.0, 12.0),
                target: Vec3::ZERO,
                near: 0.1,
                far: 100.0,
            },
            light: SceneLight {
                direction: Vec3::new(0.0, -1.0, 0.0),
                intensity: 1.0,
                color: Color::WHITE,
            },
            entities: vec![
                entity("player", &[], Vec3::new(-1.0, 0.5, 0.0), Vec3::ONE),
                entity(
                    "wall_a",
                    &[],
                    Vec3::new(0.0, 0.75, 0.0),
                    Vec3::new(2.0, 1.5, 0.5),
                ),
                entity("hazard_a", &[], Vec3::new(1.0, 0.45, 0.0), Vec3::ONE),
                entity("goal", &[], Vec3::new(3.0, 0.3, 0.0), Vec3::splat(1.0)),
            ],
        };
        let level = Level::from_scene(&scene);
        assert_eq!(level.walls.len(), 1);
        assert_eq!(level.hazards.len(), 1);
        assert_eq!(level.player_start.x, -1.0);
    }

    #[test]
    fn intro_level_tags_without_wall_prefix() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("levels/01_intro.kerabit.json");
        let scene = Scene::load(&path).expect("load intro");
        assert!(scene
            .entities
            .iter()
            .filter(|e| e.has_tag("wall"))
            .all(|e| !e.name.starts_with("wall_")));
        assert!(scene
            .entities
            .iter()
            .filter(|e| e.has_tag("hazard"))
            .all(|e| !e.name.starts_with("hazard_")));
        let level = Level::from_scene(&scene);
        assert_eq!(level.walls.len(), 2);
        assert_eq!(level.hazards.len(), 1);
        assert_eq!(level.hazards[0].name, "spike_a");
    }

    #[test]
    fn all_level_files_load_with_required_roles() {
        assert!(
            LEVEL_FILES.len() >= 5,
            "E4 accept: need at least 5 Reach levels"
        );
        for file in LEVEL_FILES {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("levels")
                .join(file);
            let scene = Scene::load(&path).unwrap_or_else(|e| panic!("load {file}: {e}"));
            assert!(
                scene.entities.iter().any(|e| entity_has_role(e, "player")),
                "{file}: missing player"
            );
            assert!(
                scene.entities.iter().any(|e| entity_has_role(e, "goal")),
                "{file}: missing goal"
            );
            assert!(
                scene.entities.iter().any(|e| entity_has_role(e, "ground")),
                "{file}: missing ground"
            );
            let level = Level::from_scene(&scene);
            assert!(
                (level.player_half.x - 0.5).abs() < 1e-3,
                "{file}: expected unit-cube player half=0.5, got {:?}",
                level.player_half
            );
            assert!(!level.hazards.is_empty(), "{file}: expected hazards");
        }
    }
}
