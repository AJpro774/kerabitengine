//! Surge — Kerabit score-attack arena (M6).
//!
//! Modes:
//! - **Timed ranked** — survive 60s per arena; clear bonus + rank tier; cycle arenas
//! - **Endless** — survive until you fall; waves keep ramping; best score tracked
//!
//! Five editor-authored arenas under `levels/`.
//!
//! **Controls**
//! - Title: ←/→ arena · 1/2 or ↑/↓ mode · Space start
//! - WASD — move
//! - R — retry
//! - Escape — title / quit
//!
//! ```bash
//! cargo run -p surge
//! ```

use std::fs;
use std::path::PathBuf;

use kerabit::prelude::*;
use kerabit::{SceneEntity, SceneMesh};

const LEVEL_FILES: &[&str] = &[
    "01_pit.kerabit.json",
    "02_ring.kerabit.json",
    "03_cross.kerabit.json",
    "04_gauntlet.kerabit.json",
    "05_vortex.kerabit.json",
];

const ARENA_NAMES: &[&str] = &["Pit", "Ring", "Cross", "Gauntlet", "Vortex"];

const SURVIVE_SECS: f32 = 60.0;
const WAVE_PERIOD: f32 = 15.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Title,
    Playing,
    Survived,
    Failed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GameMode {
    Timed,
    Endless,
}

impl GameMode {
    fn label(self) -> &'static str {
        match self {
            GameMode::Timed => "Timed Ranked",
            GameMode::Endless => "Endless",
        }
    }

    fn blurb(self) -> &'static str {
        match self {
            GameMode::Timed => "Survive 60s · clear arenas · earn a rank",
            GameMode::Endless => "No time limit · waves keep rising · beat your best",
        }
    }
}

#[derive(Clone, Copy)]
enum Motion {
    Orbit,
    SlideX,
    SlideZ,
}

struct Hazard {
    name: String,
    rest: Vec3,
    half: Vec3,
    base_scale: Vec3,
    motion: Motion,
    phase: f32,
    amplitude: f32,
}

struct Arena {
    player_start: Vec3,
    player_half: Vec3,
    player_base_scale: Vec3,
    walls: Vec<(Vec3, Vec3)>,
    hazards: Vec<Hazard>,
    platform_half: Vec2,
    cam_eye_offset: Vec3,
    cam_height: f32,
}

struct BestScores {
    timed: f32,
    endless: f32,
}

fn root_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn level_path(index: usize) -> PathBuf {
    root_dir().join("levels").join(LEVEL_FILES[index])
}

fn asset_path(name: &str) -> PathBuf {
    root_dir().join("assets").join(name)
}

fn progress_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("Kerabit"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".kerabit"))
    }
}

fn progress_path() -> PathBuf {
    if let Some(base) = progress_dir() {
        let _ = fs::create_dir_all(&base);
        return base.join("surge_best.txt");
    }
    root_dir().join("surge_best.txt")
}

fn load_bests() -> BestScores {
    let mut bests = BestScores {
        timed: 0.0,
        endless: 0.0,
    };
    let Ok(text) = fs::read_to_string(progress_path()) else {
        return bests;
    };
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        match (parts.next(), parts.next()) {
            (Some("timed"), Some(v)) => bests.timed = v.parse().unwrap_or(0.0),
            (Some("endless"), Some(v)) => bests.endless = v.parse().unwrap_or(0.0),
            _ => {}
        }
    }
    bests
}

fn save_bests(bests: &BestScores) {
    let body = format!("timed {:.1}\nendless {:.1}\n", bests.timed, bests.endless);
    if let Err(err) = fs::write(progress_path(), body) {
        eprintln!("surge: failed to save bests: {err}");
    }
}

fn rank_for_score(score: f32) -> &'static str {
    if score >= 2200.0 {
        "PLATINUM"
    } else if score >= 1600.0 {
        "GOLD"
    } else if score >= 1000.0 {
        "SILVER"
    } else if score >= 500.0 {
        "BRONZE"
    } else {
        "ROOKIE"
    }
}

fn play_sfx_at(ctx: &mut Context<'_>, name: &str, position: Vec3) {
    let path = asset_path(name);
    if let Err(err) = ctx.audio().play_at(&path, position) {
        eprintln!("surge audio: {err}");
    }
}

fn text_width(s: &str, size: f32) -> f32 {
    s.chars().filter(|c| *c != '\n').count() as f32 * size
}

fn centered_x(s: &str, size: f32) -> f32 {
    (1.0 - text_width(s, size)) * 0.5
}

fn register_walls(ctx: &mut Context<'_>, arena: &Arena) {
    for (center, half) in &arena.walls {
        ctx.physics()
            .add_aabb(Aabb::from_center_half_extents(*center, *half));
    }
}

fn begin_camera(arena: &Arena, player_pos: Vec3, cam_eye: &mut Vec3, cam_target: &mut Vec3) {
    *cam_eye = player_pos + arena.cam_eye_offset;
    cam_eye.y = arena.cam_height;
    *cam_target = Vec3::new(player_pos.x, 0.4, player_pos.z);
}

fn wave_speed(elapsed: f32) -> f32 {
    1.0 + (elapsed / WAVE_PERIOD).floor() * 0.35
}

fn wave_index(elapsed: f32) -> u32 {
    (elapsed / WAVE_PERIOD).floor() as u32 + 1
}

fn load_arena_at(
    ctx: &mut Context<'_>,
    level_index: usize,
    arena: &mut Arena,
    player_pos: &mut Vec3,
    cam_eye: &mut Vec3,
    cam_target: &mut Vec3,
    physics_ready: &mut bool,
) -> bool {
    let path = level_path(level_index);
    let scene = match Scene::load(&path) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("surge: failed to load {}: {err}", path.display());
            return false;
        }
    };
    *arena = Arena::from_scene(&scene);
    if let Err(err) = ctx.apply_scene(&scene) {
        eprintln!("surge: apply_scene failed: {err}");
        return false;
    }
    *player_pos = arena.player_start;
    begin_camera(arena, *player_pos, cam_eye, cam_target);
    register_walls(ctx, arena);
    *physics_ready = true;
    let cam = ctx.camera_mut();
    cam.eye = *cam_eye;
    cam.target = *cam_target;
    true
}

fn advance_arena(
    ctx: &mut Context<'_>,
    level_index: &mut usize,
    arena: &mut Arena,
    player_pos: &mut Vec3,
    phase: &mut Phase,
    elapsed: &mut f32,
    score: &mut f32,
    flash: &mut f32,
    cam_eye: &mut Vec3,
    cam_target: &mut Vec3,
    physics_ready: &mut bool,
) -> bool {
    let next = *level_index + 1;
    if next >= LEVEL_FILES.len() {
        return false;
    }
    if !load_arena_at(
        ctx,
        next,
        arena,
        player_pos,
        cam_eye,
        cam_target,
        physics_ready,
    ) {
        return false;
    }
    *level_index = next;
    *phase = Phase::Playing;
    *elapsed = 0.0;
    *score = 0.0;
    *flash = 0.0;
    true
}

fn main() {
    let path = level_path(0);
    let scene = Scene::load(&path).unwrap_or_else(|err| {
        eprintln!("surge: failed to load {}: {err}", path.display());
        std::process::exit(1);
    });

    let mut arena = Arena::from_scene(&scene);
    let mut level_index = 0usize;
    let mut player_pos = arena.player_start;
    let mut phase = Phase::Title;
    let mut mode = GameMode::Timed;
    let mut physics_ready = false;
    let mut elapsed = 0.0f32;
    let mut score = 0.0f32;
    let mut bests = load_bests();
    let mut time_alive = 0.0f32;
    let mut flash = 0.0f32;
    let mut flash_color = Color::WHITE;
    let mut cam_eye = arena.player_start + arena.cam_eye_offset;
    let mut cam_target = Vec3::new(arena.player_start.x, 0.4, arena.player_start.z);
    let mut all_clear = false;
    let mut final_score = 0.0f32;
    let mut final_rank = "ROOKIE";

    scene
        .into_kerabit("Surge")
        .unwrap_or_else(|err| {
            eprintln!("surge: {err}");
            std::process::exit(1);
        })
        .run(move |ctx| {
            time_alive += ctx.dt();
            let dt = ctx.dt();
            ctx.sync_audio_listener();

            if ctx.input().key_pressed(Key::Escape) {
                if phase == Phase::Title {
                    ctx.quit();
                    return;
                }
                phase = Phase::Title;
                player_pos = arena.player_start;
                return;
            }

            if !physics_ready {
                register_walls(ctx, &arena);
                physics_ready = true;
                let cam = ctx.camera_mut();
                cam.eye = cam_eye;
                cam.target = cam_target;
            }

            flash = (flash - dt * 2.0).max(0.0);

            match phase {
                Phase::Title => {
                    let bob = (time_alive * 2.0).sin() * 0.1;
                    player_pos = arena.player_start;
                    player_pos.y = arena.player_start.y + bob;
                    apply_player_visual(ctx, &arena, player_pos);
                    update_hazards(ctx, &arena, time_alive, 0.55);
                    follow_camera(ctx, &mut cam_eye, &mut cam_target, &arena, player_pos, dt, 2.2);

                    if ctx.input().key_pressed(Key::Digit1) || ctx.input().key_pressed(Key::Up) {
                        mode = GameMode::Timed;
                    }
                    if ctx.input().key_pressed(Key::Digit2) || ctx.input().key_pressed(Key::Down) {
                        mode = GameMode::Endless;
                    }
                    if ctx.input().key_pressed(Key::Left) {
                        let prev = if level_index == 0 {
                            LEVEL_FILES.len() - 1
                        } else {
                            level_index - 1
                        };
                        if load_arena_at(
                            ctx,
                            prev,
                            &mut arena,
                            &mut player_pos,
                            &mut cam_eye,
                            &mut cam_target,
                            &mut physics_ready,
                        ) {
                            level_index = prev;
                        }
                    }
                    if ctx.input().key_pressed(Key::Right) {
                        let next = (level_index + 1) % LEVEL_FILES.len();
                        if load_arena_at(
                            ctx,
                            next,
                            &mut arena,
                            &mut player_pos,
                            &mut cam_eye,
                            &mut cam_target,
                            &mut physics_ready,
                        ) {
                            level_index = next;
                        }
                    }

                    draw_flash(ctx, flash, flash_color);
                    ctx.ui()
                        .rect(0.0, 0.0, 1.0, 1.0, Color::rgba(0.08, 0.02, 0.04, 0.58));
                    let title = "SURGE";
                    let ts = 0.1;
                    ctx.ui().text(
                        centered_x(title, ts),
                        0.2,
                        ts,
                        Color::rgb(1.0, 0.55, 0.35),
                        title,
                    );

                    let arena_line = format!(
                        "Arena  {}/{}  ·  {}",
                        level_index + 1,
                        LEVEL_FILES.len(),
                        ARENA_NAMES[level_index]
                    );
                    let asz = 0.026;
                    ctx.ui().text(
                        centered_x(&arena_line, asz),
                        0.34,
                        asz,
                        Color::rgb(0.9, 0.78, 0.72),
                        &arena_line,
                    );

                    let mode_line = format!("Mode  {}", mode.label());
                    let msz = 0.032;
                    ctx.ui().text(
                        centered_x(&mode_line, msz),
                        0.42,
                        msz,
                        Color::rgb(1.0, 0.75, 0.4),
                        &mode_line,
                    );
                    let blurb = mode.blurb();
                    let bsz = 0.022;
                    ctx.ui().text(
                        centered_x(blurb, bsz),
                        0.48,
                        bsz,
                        Color::rgb(0.8, 0.68, 0.65),
                        blurb,
                    );

                    let best_line = match mode {
                        GameMode::Timed => format!("Best timed  {:.0}", bests.timed),
                        GameMode::Endless => format!("Best endless  {:.0}", bests.endless),
                    };
                    let bsz2 = 0.024;
                    ctx.ui().text(
                        centered_x(&best_line, bsz2),
                        0.56,
                        bsz2,
                        Color::rgb(0.85, 0.8, 0.75),
                        &best_line,
                    );

                    let hint = "1/2 mode  ←/→ arena  Space start";
                    let hs = 0.026;
                    ctx.ui().text(
                        centered_x(hint, hs),
                        0.68,
                        hs,
                        Color::rgb(0.95, 0.9, 0.88),
                        hint,
                    );

                    if ctx.input().key_pressed(Key::Space) {
                        phase = Phase::Playing;
                        player_pos = arena.player_start;
                        elapsed = 0.0;
                        score = 0.0;
                        flash = 0.25;
                        flash_color = Color::rgb(1.0, 0.6, 0.3);
                    }
                }
                Phase::Playing => {
                    elapsed += dt;
                    let speed_mul = wave_speed(elapsed);
                    score += dt * (10.0 + (wave_index(elapsed) as f32 - 1.0) * 4.0);
                    match mode {
                        GameMode::Timed => bests.timed = bests.timed.max(score),
                        GameMode::Endless => bests.endless = bests.endless.max(score),
                    }

                    if ctx.input().key_pressed(Key::R) {
                        reset_run(
                            ctx,
                            &arena,
                            &mut player_pos,
                            &mut phase,
                            &mut elapsed,
                            &mut score,
                            &mut flash,
                        );
                    }

                    let move_speed = 5.6;
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
                        wish.normalize() * move_speed
                    } else {
                        Vec3::ZERO
                    };

                    let result = ctx.physics().move_and_collide(
                        player_pos,
                        velocity,
                        arena.player_half,
                        dt,
                    );
                    player_pos = result.position;
                    player_pos.y = arena.player_start.y;

                    apply_player_visual(ctx, &arena, player_pos);
                    let hazard_centers = update_hazards(ctx, &arena, elapsed, speed_mul);
                    follow_camera(ctx, &mut cam_eye, &mut cam_target, &arena, player_pos, dt, 7.0);

                    let player_aabb =
                        Aabb::from_center_half_extents(player_pos, arena.player_half);
                    let off_platform = player_pos.x.abs() > arena.platform_half.x
                        || player_pos.z.abs() > arena.platform_half.y;
                    let hit_hazard = arena.hazards.iter().zip(hazard_centers.iter()).any(
                        |(h, center)| {
                            player_aabb
                                .overlaps(Aabb::from_center_half_extents(*center, h.half))
                        },
                    );

                    if hit_hazard || off_platform {
                        phase = Phase::Failed;
                        flash = 1.0;
                        flash_color = Color::rgb(0.95, 0.1, 0.18);
                        match mode {
                            GameMode::Timed => bests.timed = bests.timed.max(score),
                            GameMode::Endless => bests.endless = bests.endless.max(score),
                        }
                        save_bests(&bests);
                        play_sfx_at(ctx, "fail.wav", player_pos);
                    } else if mode == GameMode::Timed && elapsed >= SURVIVE_SECS {
                        phase = Phase::Survived;
                        final_score = score + 250.0;
                        bests.timed = bests.timed.max(final_score);
                        final_rank = rank_for_score(final_score);
                        all_clear = level_index + 1 >= LEVEL_FILES.len();
                        save_bests(&bests);
                        flash = 0.9;
                        flash_color = Color::rgb(1.0, 0.75, 0.25);
                        play_sfx_at(ctx, "win.wav", player_pos);
                    }

                    draw_flash(ctx, flash, flash_color);
                    draw_hud(ctx, mode, level_index, elapsed, score, wave_index(elapsed));
                }
                Phase::Survived => {
                    apply_player_visual(ctx, &arena, player_pos);
                    update_hazards(ctx, &arena, elapsed, wave_speed(elapsed) * 0.4);
                    follow_camera(ctx, &mut cam_eye, &mut cam_target, &arena, player_pos, dt, 3.0);

                    draw_flash(ctx, flash, flash_color);
                    ctx.ui()
                        .rect(0.0, 0.0, 1.0, 1.0, Color::rgba(0.1, 0.05, 0.02, 0.5));
                    let headline = if all_clear { "ALL CLEAR" } else { "SURVIVED" };
                    let hs = 0.08;
                    ctx.ui().text(
                        centered_x(headline, hs),
                        0.26,
                        hs,
                        Color::rgb(1.0, 0.8, 0.35),
                        headline,
                    );
                    let rank_line = format!("Rank  {final_rank}");
                    let rs = 0.04;
                    ctx.ui().text(
                        centered_x(&rank_line, rs),
                        0.38,
                        rs,
                        Color::rgb(1.0, 0.7, 0.4),
                        &rank_line,
                    );
                    let score_line = format!("Score {final_score:.0}   Best {:.0}", bests.timed);
                    let ss = 0.032;
                    ctx.ui().text(
                        centered_x(&score_line, ss),
                        0.48,
                        ss,
                        Color::WHITE,
                        &score_line,
                    );
                    let prompt = if all_clear {
                        "Space — title"
                    } else {
                        "Space — next arena"
                    };
                    let ps = 0.03;
                    ctx.ui().text(
                        centered_x(prompt, ps),
                        0.6,
                        ps,
                        Color::rgb(0.85, 0.78, 0.72),
                        prompt,
                    );

                    if ctx.input().key_pressed(Key::Space) {
                        if all_clear {
                            phase = Phase::Title;
                        } else if !advance_arena(
                            ctx,
                            &mut level_index,
                            &mut arena,
                            &mut player_pos,
                            &mut phase,
                            &mut elapsed,
                            &mut score,
                            &mut flash,
                            &mut cam_eye,
                            &mut cam_target,
                            &mut physics_ready,
                        ) {
                            phase = Phase::Title;
                        }
                    }
                }
                Phase::Failed => {
                    apply_player_visual(ctx, &arena, player_pos);
                    update_hazards(ctx, &arena, elapsed, 0.3);
                    follow_camera(ctx, &mut cam_eye, &mut cam_target, &arena, player_pos, dt, 3.0);

                    draw_flash(ctx, flash, flash_color);
                    ctx.ui()
                        .rect(0.0, 0.0, 1.0, 1.0, Color::rgba(0.28, 0.02, 0.06, 0.55));
                    let fail = "DOWN";
                    let fs = 0.09;
                    ctx.ui()
                        .text(centered_x(fail, fs), 0.28, fs, Color::rgb(1.0, 0.3, 0.35), fail);

                    let best = match mode {
                        GameMode::Timed => bests.timed,
                        GameMode::Endless => bests.endless,
                    };
                    let score_line = format!("Score {score:.0}   Best {best:.0}");
                    let ss = 0.03;
                    ctx.ui().text(
                        centered_x(&score_line, ss),
                        0.42,
                        ss,
                        Color::rgb(0.95, 0.85, 0.85),
                        &score_line,
                    );

                    if mode == GameMode::Endless {
                        let time_line = format!("Survived {elapsed:.1}s  ·  Wave {}", wave_index(elapsed));
                        let ts = 0.026;
                        ctx.ui().text(
                            centered_x(&time_line, ts),
                            0.5,
                            ts,
                            Color::rgb(0.9, 0.7, 0.7),
                            &time_line,
                        );
                    }

                    let prompt = "Space / R — retry   Esc — title";
                    let ps = 0.028;
                    ctx.ui().text(
                        centered_x(prompt, ps),
                        0.6,
                        ps,
                        Color::rgb(0.9, 0.8, 0.8),
                        prompt,
                    );

                    if ctx.input().key_pressed(Key::Space) || ctx.input().key_pressed(Key::R) {
                        reset_run(
                            ctx,
                            &arena,
                            &mut player_pos,
                            &mut phase,
                            &mut elapsed,
                            &mut score,
                            &mut flash,
                        );
                    }
                }
            }
        });
}

fn reset_run(
    ctx: &mut Context<'_>,
    arena: &Arena,
    player_pos: &mut Vec3,
    phase: &mut Phase,
    elapsed: &mut f32,
    score: &mut f32,
    flash: &mut f32,
) {
    *player_pos = arena.player_start;
    *phase = Phase::Playing;
    *elapsed = 0.0;
    *score = 0.0;
    *flash = 0.2;
    apply_player_visual(ctx, arena, *player_pos);
    for h in &arena.hazards {
        if let Some(ent) = ctx.world_mut().get_mut(&h.name) {
            ent.transform.set_translation(h.rest);
            ent.transform.set_scale(h.base_scale);
        }
    }
}

fn apply_player_visual(ctx: &mut Context<'_>, arena: &Arena, pos: Vec3) {
    if let Some(player) = ctx.world_mut().get_mut("player") {
        player.transform.set_translation(pos);
        player.transform.set_scale(arena.player_base_scale);
    }
}

fn update_hazards(
    ctx: &mut Context<'_>,
    arena: &Arena,
    t: f32,
    speed_mul: f32,
) -> Vec<Vec3> {
    let mut centers = Vec::with_capacity(arena.hazards.len());
    for (i, h) in arena.hazards.iter().enumerate() {
        let ang = t * speed_mul + h.phase;
        let mut pos = h.rest;
        match h.motion {
            Motion::Orbit => {
                let r = h.amplitude.max(0.5);
                let base_angle = h.rest.z.atan2(h.rest.x);
                let a = base_angle + ang * (1.1 + i as f32 * 0.07);
                pos.x = a.cos() * r;
                pos.z = a.sin() * r;
            }
            Motion::SlideX => {
                pos.x = h.rest.x + ang.sin() * h.amplitude;
            }
            Motion::SlideZ => {
                pos.z = h.rest.z + ang.sin() * h.amplitude;
            }
        }
        let pulse = 1.0 + (ang * 2.0).cos() * 0.06;
        if let Some(ent) = ctx.world_mut().get_mut(&h.name) {
            ent.transform.set_translation(pos);
            ent.transform.set_scale(h.base_scale * pulse);
        }
        centers.push(pos);
    }
    centers
}

fn follow_camera(
    ctx: &mut Context<'_>,
    cam_eye: &mut Vec3,
    cam_target: &mut Vec3,
    arena: &Arena,
    player_pos: Vec3,
    dt: f32,
    speed: f32,
) {
    let desired_target = Vec3::new(player_pos.x, 0.4, player_pos.z);
    let mut desired_eye = desired_target + arena.cam_eye_offset;
    desired_eye.y = arena.cam_height;
    let k = (1.0 - (-speed * dt).exp()).clamp(0.0, 1.0);
    *cam_eye = cam_eye.lerp(desired_eye, k);
    *cam_target = cam_target.lerp(desired_target, k);
    let cam = ctx.camera_mut();
    cam.eye = *cam_eye;
    cam.target = *cam_target;
}

fn draw_flash(ctx: &mut Context<'_>, flash: f32, color: Color) {
    if flash <= 0.001 {
        return;
    }
    let tint = Color::rgba(0.0, 0.0, 0.0, 0.0).lerp(
        Color::rgba(color.r, color.g, color.b, 0.45),
        flash,
    );
    ctx.ui().rect(0.0, 0.0, 1.0, 1.0, tint);
}

fn draw_hud(
    ctx: &mut Context<'_>,
    mode: GameMode,
    level_index: usize,
    elapsed: f32,
    score: f32,
    wave: u32,
) {
    let arena_line = format!(
        "{}  ·  {}/{}",
        ARENA_NAMES[level_index],
        level_index + 1,
        LEVEL_FILES.len()
    );
    let score_line = format!("Score {score:.0}");
    let wave_line = format!("Wave {wave}");
    let mode_tag = match mode {
        GameMode::Timed => "TIMED",
        GameMode::Endless => "ENDLESS",
    };

    ctx.ui()
        .text(0.03, 0.03, 0.022, Color::rgb(0.75, 0.55, 0.5), mode_tag);
    ctx.ui()
        .text(0.03, 0.065, 0.028, Color::rgb(0.95, 0.88, 0.82), &arena_line);

    match mode {
        GameMode::Timed => {
            let remain = (SURVIVE_SECS - elapsed).max(0.0);
            let time_line = format!("Time {remain:.1}s");
            ctx.ui()
                .text(0.03, 0.11, 0.032, Color::rgb(1.0, 0.7, 0.35), &time_line);
            let bar_w = (remain / SURVIVE_SECS).clamp(0.0, 1.0) * 0.3;
            ctx.ui()
                .rect(0.03, 0.25, 0.3, 0.012, Color::rgba(0.2, 0.1, 0.1, 0.7));
            ctx.ui()
                .rect(0.03, 0.25, bar_w, 0.012, Color::rgb(1.0, 0.55, 0.25));
        }
        GameMode::Endless => {
            let time_line = format!("Alive {elapsed:.1}s");
            ctx.ui()
                .text(0.03, 0.11, 0.032, Color::rgb(1.0, 0.55, 0.45), &time_line);
        }
    }

    ctx.ui()
        .text(0.03, 0.16, 0.028, Color::rgb(0.95, 0.92, 0.85), &score_line);
    ctx.ui()
        .text(0.03, 0.205, 0.024, Color::rgb(0.85, 0.45, 0.4), &wave_line);
    ctx.ui().text(
        0.03,
        0.94,
        0.022,
        Color::rgb(0.7, 0.62, 0.6),
        "WASD move  R retry  Esc title",
    );
}

impl Arena {
    fn from_scene(scene: &Scene) -> Self {
        let mut player_start = Vec3::new(0.0, 0.5, 0.0);
        let mut player_base_scale = Vec3::ONE;
        let mut walls = Vec::new();
        let mut hazards = Vec::new();
        let mut platform_half = Vec2::new(7.5, 7.5);
        let mut hazard_i = 0usize;

        for e in &scene.entities {
            let half = e.scale * 0.5;
            if entity_has_role(e, "player") {
                player_start = e.at;
                player_base_scale = e.scale;
            } else if entity_has_role(e, "ground") {
                if let SceneMesh::Plane { size } = &e.mesh {
                    let edge = size * 0.5 - 0.35;
                    platform_half = Vec2::new(edge, edge);
                }
            } else if entity_has_role(e, "wall") {
                walls.push((e.at, half));
            } else if entity_has_role(e, "hazard") {
                let motion = hazard_motion(e);
                let amplitude = match motion {
                    Motion::Orbit => (e.at.x * e.at.x + e.at.z * e.at.z).sqrt().max(1.0),
                    Motion::SlideX | Motion::SlideZ => {
                        (e.at.x.abs().max(e.at.z.abs()).max(2.0) * 0.85).min(5.5)
                    }
                };
                hazards.push(Hazard {
                    name: e.name.clone(),
                    rest: e.at,
                    half,
                    base_scale: e.scale,
                    motion,
                    phase: hazard_i as f32 * 1.7 + 0.4,
                    amplitude,
                });
                hazard_i += 1;
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
            platform_half,
            cam_eye_offset,
            cam_height: scene.camera.eye.y,
        }
    }
}

fn entity_has_role(e: &SceneEntity, role: &str) -> bool {
    const KNOWN: &[&str] = &["player", "goal", "ground", "wall", "hazard"];
    let tagged = KNOWN.iter().any(|r| e.has_tag(r));
    if tagged {
        return e.has_tag(role);
    }
    match role {
        "player" | "ground" => e.name == role,
        "wall" => e.name.starts_with("wall_") || e.name.starts_with("rim_"),
        "hazard" => e.name.starts_with("hazard_"),
        _ => false,
    }
}

fn hazard_motion(e: &SceneEntity) -> Motion {
    if e.has_tag("slide_x") {
        Motion::SlideX
    } else if e.has_tag("slide_z") {
        Motion::SlideZ
    } else if e.has_tag("orbit") {
        Motion::Orbit
    } else {
        let xz = (e.at.x * e.at.x + e.at.z * e.at.z).sqrt();
        if xz > 0.75 {
            Motion::Orbit
        } else if e.at.z.abs() >= e.at.x.abs() {
            Motion::SlideX
        } else {
            Motion::SlideZ
        }
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
                metallic: 0.0,
                texture: None,
            },
            at,
            rotation: Quat::IDENTITY,
            scale,
            parent: None,
            components: Default::default(),
            extras: Default::default(),
        }
    }

    #[test]
    fn arenas_load_with_required_roles() {
        assert_eq!(LEVEL_FILES.len(), 5, "M6: five arenas");
        assert_eq!(ARENA_NAMES.len(), LEVEL_FILES.len());
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
                scene.entities.iter().any(|e| entity_has_role(e, "ground")),
                "{file}: missing ground"
            );
            let arena = Arena::from_scene(&scene);
            assert!(!arena.hazards.is_empty(), "{file}: expected hazards");
            assert!(
                arena
                    .hazards
                    .iter()
                    .any(|h| matches!(h.motion, Motion::Orbit))
                    || arena
                        .hazards
                        .iter()
                        .any(|h| matches!(h.motion, Motion::SlideX | Motion::SlideZ)),
                "{file}: expected motion tags"
            );
        }
    }

    #[test]
    fn rank_tiers() {
        assert_eq!(rank_for_score(100.0), "ROOKIE");
        assert_eq!(rank_for_score(600.0), "BRONZE");
        assert_eq!(rank_for_score(1200.0), "SILVER");
        assert_eq!(rank_for_score(1800.0), "GOLD");
        assert_eq!(rank_for_score(2500.0), "PLATINUM");
    }

    #[test]
    fn motion_tags_parse() {
        let scene = Scene {
            clear_color: Color::BLACK,
            ambient: Color::WHITE,
            camera: SceneCamera {
                fov_y: 50.0,
                eye: Vec3::new(0.0, 10.0, 12.0),
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
                entity("hero", &["player"], Vec3::new(0.0, 0.5, 0.0), Vec3::ONE),
                SceneEntity {
                    name: "floor".into(),
                    tags: vec!["ground".into()],
                    mesh: SceneMesh::Plane { size: 16.0 },
                    material: SceneMaterial {
                        color: Color::GRAY,
                        roughness: 0.9,
                        metallic: 0.0,
                        texture: None,
                    },
                    at: Vec3::ZERO,
                    rotation: Quat::IDENTITY,
                    scale: Vec3::ONE,
                    parent: None,
                    components: Default::default(),
                    extras: Default::default(),
                },
                entity(
                    "spin",
                    &["hazard", "orbit"],
                    Vec3::new(3.0, 0.45, 0.0),
                    Vec3::ONE,
                ),
                entity(
                    "sweep",
                    &["hazard", "slide_x"],
                    Vec3::new(0.0, 0.4, 3.0),
                    Vec3::ONE,
                ),
            ],
            components: Default::default(),
            extras: Default::default(),
        };
        let arena = Arena::from_scene(&scene);
        assert_eq!(arena.hazards.len(), 2);
        assert!(matches!(arena.hazards[0].motion, Motion::Orbit));
        assert!(matches!(arena.hazards[1].motion, Motion::SlideX));
    }
}
