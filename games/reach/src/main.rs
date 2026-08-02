//! Reach — Kerabit flagship campaign.
//!
//! Title → chapter select → play → clear / fail → next level or retry.
//! HUD and juice via the public Kerabit API only (`ctx.ui()`, camera lerp,
//! squash, particles, spatial SFX).
//!
//! Level transitions use mid-run [`Context::apply_scene`] (same window / GPU /
//! EventLoop) — no App teardown between levels.
//!
//! **Controls**
//! - WASD — move
//! - Space — start / confirm / next level
//! - ←/→ or 1–3 — chapter select
//! - R — retry after fail (or mid-run)
//! - Escape — back / quit
//!
//! ```bash
//! cargo run -p reach
//! ```

use std::fs;
use std::path::PathBuf;

use kerabit::prelude::*;
use kerabit::{SceneEntity, SceneMesh};

const LEVEL_FILES: &[&str] = &[
    "01_intro.kerabit.json",
    "02_bent.kerabit.json",
    "03_gauntlet.kerabit.json",
    "04_switchback.kerabit.json",
    "05_crossfire.kerabit.json",
    "06_serpent.kerabit.json",
    "07_twin_gates.kerabit.json",
    "08_zipper.kerabit.json",
    "09_lattice.kerabit.json",
    "10_rift.kerabit.json",
    "11_crucible.kerabit.json",
    "12_summit.kerabit.json",
];

const LEVEL_NAMES: &[&str] = &[
    "Intro",
    "Bent",
    "Gauntlet",
    "Switchback",
    "Crossfire",
    "Serpent",
    "Twin Gates",
    "Zipper",
    "Lattice",
    "Rift",
    "Crucible",
    "Summit",
];

struct Chapter {
    name: &'static str,
    /// Inclusive start index into [`LEVEL_FILES`].
    start: usize,
    /// Exclusive end index.
    end: usize,
}

const CHAPTERS: &[Chapter] = &[
    Chapter {
        name: "I · Approach",
        start: 0,
        end: 4,
    },
    Chapter {
        name: "II · Pressure",
        start: 4,
        end: 8,
    },
    Chapter {
        name: "III · Summit",
        start: 8,
        end: 12,
    },
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Title,
    ChapterSelect,
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

/// Persisted campaign progress (best clears + highest unlocked chapter).
#[derive(Clone, Debug)]
struct Progress {
    /// Highest chapter index the player may start (0 = Approach only).
    unlocked_chapter: usize,
    /// Per-level best clear times (seconds); `None` = never cleared.
    best: Vec<Option<f32>>,
}

impl Progress {
    fn fresh() -> Self {
        Self {
            unlocked_chapter: 0,
            best: vec![None; LEVEL_FILES.len()],
        }
    }

    fn load() -> Self {
        let path = progress_path();
        let Ok(text) = fs::read_to_string(&path) else {
            return Self::fresh();
        };
        Self::parse(&text).unwrap_or_else(Self::fresh)
    }

    fn parse(text: &str) -> Option<Self> {
        let mut unlocked = 0usize;
        let mut best = vec![None; LEVEL_FILES.len()];
        let mut saw_header = false;
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let key = parts.next()?;
            match key {
                "v1" => saw_header = true,
                "unlock" => {
                    unlocked = parts.next()?.parse().ok()?;
                }
                "best" => {
                    let idx: usize = parts.next()?.parse().ok()?;
                    let t: f32 = parts.next()?.parse().ok()?;
                    if idx < best.len() && t > 0.0 {
                        best[idx] = Some(t);
                    }
                }
                _ => {}
            }
        }
        if !saw_header {
            return None;
        }
        unlocked = unlocked.min(CHAPTERS.len().saturating_sub(1));
        Some(Self {
            unlocked_chapter: unlocked,
            best,
        })
    }

    fn save(&self) {
        let path = progress_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let mut out = String::from("# Kerabit Reach campaign progress\nv1\n");
        out.push_str(&format!("unlock {}\n", self.unlocked_chapter));
        for (i, t) in self.best.iter().enumerate() {
            if let Some(sec) = t {
                out.push_str(&format!("best {i} {sec:.3}\n"));
            }
        }
        if let Err(err) = fs::write(&path, out) {
            eprintln!("reach: could not save progress: {err}");
        }
    }

    fn record_clear(&mut self, level_index: usize, time: f32) -> bool {
        let mut improved = false;
        if level_index < self.best.len() {
            match self.best[level_index] {
                Some(prev) if time < prev => {
                    self.best[level_index] = Some(time);
                    improved = true;
                }
                None => {
                    self.best[level_index] = Some(time);
                    improved = true;
                }
                _ => {}
            }
        }
        // Unlock next chapter when every level in the current chapter is cleared.
        for (ci, ch) in CHAPTERS.iter().enumerate() {
            if ci > self.unlocked_chapter {
                break;
            }
            let chapter_done = (ch.start..ch.end).all(|i| self.best.get(i).copied().flatten().is_some());
            if chapter_done {
                let next = (ci + 1).min(CHAPTERS.len().saturating_sub(1));
                if next > self.unlocked_chapter {
                    self.unlocked_chapter = next;
                }
            }
        }
        improved
    }

    fn chapter_best_sum(&self, chapter: usize) -> Option<f32> {
        let ch = CHAPTERS.get(chapter)?;
        let mut sum = 0.0f32;
        for i in ch.start..ch.end {
            sum += self.best.get(i).copied().flatten()?;
        }
        Some(sum)
    }
}

fn progress_path() -> PathBuf {
    if let Some(base) = progress_dir() {
        return base.join("reach_progress.txt");
    }
    root_dir().join("reach_progress.txt")
}

fn progress_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Kerabit"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".kerabit"))
    }
}

fn chapter_for_level(level_index: usize) -> usize {
    CHAPTERS
        .iter()
        .position(|c| level_index >= c.start && level_index < c.end)
        .unwrap_or(0)
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
            if mac_os.file_name().is_some_and(|n| n == "MacOS") {
                let resources = mac_os.join("../Resources");
                if resources.join("levels").is_dir() {
                    return resources.canonicalize().unwrap_or(resources);
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

fn play_sfx_at(ctx: &mut Context<'_>, name: &str, position: Vec3) {
    let path = asset_path(name);
    if let Err(err) = ctx.audio().play_at(&path, position) {
        eprintln!("reach audio: {err}");
    }
}

fn burst_win(ctx: &mut Context<'_>, at: Vec3) {
    ctx.spawn_particles(ParticleBurst {
        origin: at + Vec3::Y * 0.4,
        count: 48,
        color: Color::rgb(0.25, 0.95, 0.9),
        size: 0.1,
        speed: 3.2,
        lifetime: 0.85,
        velocity: Vec3::Y * 1.2,
        spread: 1.0,
    });
}

fn burst_fail(ctx: &mut Context<'_>, at: Vec3) {
    ctx.spawn_particles(ParticleBurst {
        origin: at + Vec3::Y * 0.3,
        count: 36,
        color: Color::rgb(0.95, 0.2, 0.18),
        size: 0.11,
        speed: 2.8,
        lifetime: 0.7,
        velocity: Vec3::Y * 0.6,
        spread: 1.0,
    });
}

fn burst_bump(ctx: &mut Context<'_>, at: Vec3) {
    ctx.spawn_particles(ParticleBurst {
        origin: at + Vec3::Y * 0.35,
        count: 10,
        color: Color::rgb(1.0, 0.75, 0.35),
        size: 0.06,
        speed: 1.6,
        lifetime: 0.35,
        velocity: Vec3::Y * 0.5,
        spread: 0.9,
    });
}

fn goal_sparkle(ctx: &mut Context<'_>, level: &Level) {
    ctx.spawn_particles(ParticleBurst {
        origin: level.goal_center + Vec3::Y * 0.5,
        count: 8,
        color: Color::rgb(0.4, 0.95, 1.0),
        size: 0.05,
        speed: 0.9,
        lifetime: 0.45,
        velocity: Vec3::Y * 0.8,
        spread: 0.7,
    });
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

fn load_level_into(
    ctx: &mut Context<'_>,
    index: usize,
    level: &mut Level,
    player_pos: &mut Vec3,
    controller: &mut CharacterController,
    phase: &mut Phase,
    elapsed: &mut f32,
    squash: &mut f32,
    flash: &mut f32,
    velocity_xz: &mut Vec2,
    cam_eye: &mut Vec3,
    cam_target: &mut Vec3,
    physics_ready: &mut bool,
    level_index: &mut usize,
) -> bool {
    if index >= LEVEL_FILES.len() {
        return false;
    }
    let path = level_path(index);
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
    *level_index = index;
    *player_pos = level.player_start;
    *controller =
        CharacterController::planar(level.player_start, level.player_half).with_max_speed(5.2);
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

fn advance_level(
    ctx: &mut Context<'_>,
    level_index: &mut usize,
    level: &mut Level,
    player_pos: &mut Vec3,
    controller: &mut CharacterController,
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
    load_level_into(
        ctx,
        next,
        level,
        player_pos,
        controller,
        phase,
        elapsed,
        squash,
        flash,
        velocity_xz,
        cam_eye,
        cam_target,
        physics_ready,
        level_index,
    )
}

fn main() {
    let path = level_path(0);
    let scene = Scene::load(&path).unwrap_or_else(|err| {
        eprintln!("reach: failed to load {}: {err}", path.display());
        std::process::exit(1);
    });

    let mut level = Level::from_scene(&scene);
    let mut level_index = 0usize;
    let mut progress = Progress::load();
    let mut chapter_cursor = progress.unlocked_chapter.min(CHAPTERS.len() - 1);

    let mut player_pos = level.player_start;
    let mut controller =
        CharacterController::planar(level.player_start, level.player_half).with_max_speed(5.2);
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
    let mut clear_improved = false;
    let mut all_clear = false;
    let mut sparkle_cd = 0.0f32;
    let mut bump_cd = 0.0f32;

    scene
        .into_kerabit("Reach")
        .unwrap_or_else(|err| {
            eprintln!("reach: {err}");
            std::process::exit(1);
        })
        .run(move |ctx| {
            time_alive += ctx.dt();
            let dt = ctx.dt();
            ctx.sync_audio_listener();
            bump_cd = (bump_cd - dt).max(0.0);
            sparkle_cd = (sparkle_cd - dt).max(0.0);

            match phase {
                Phase::Title => {
                    if ctx.input().key_pressed(Key::Escape) {
                        ctx.quit();
                        return;
                    }
                }
                Phase::ChapterSelect => {
                    if ctx.input().key_pressed(Key::Escape) {
                        phase = Phase::Title;
                        play_sfx(ctx, "ui.wav");
                        return;
                    }
                }
                Phase::Playing | Phase::Won | Phase::Failed => {
                    if ctx.input().key_pressed(Key::Escape) {
                        phase = Phase::ChapterSelect;
                        chapter_cursor = chapter_for_level(level_index)
                            .min(progress.unlocked_chapter);
                        play_sfx(ctx, "ui.wav");
                        return;
                    }
                }
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
                        0.32,
                        title_size,
                        Color::WHITE,
                        title,
                    );
                    let sub = "12 levels · 3 chapters";
                    let ss = 0.028;
                    ctx.ui().text(
                        centered_x(sub, ss),
                        0.44,
                        ss,
                        Color::rgb(0.7, 0.78, 0.88),
                        sub,
                    );
                    let hint = "Press Space";
                    let hint_size = 0.035;
                    ctx.ui().text(
                        centered_x(hint, hint_size),
                        0.54,
                        hint_size,
                        Color::rgb(0.75, 0.78, 0.85),
                        hint,
                    );

                    if ctx.input().key_pressed(Key::Space) {
                        play_sfx(ctx, "ui.wav");
                        phase = Phase::ChapterSelect;
                        chapter_cursor = progress.unlocked_chapter.min(CHAPTERS.len() - 1);
                    }
                }
                Phase::ChapterSelect => {
                    player_pos = level.player_start;
                    apply_player_visual(ctx, &level, player_pos, 0.0, 1.0);
                    spin_goal(ctx, dt * 0.5);
                    pulse_hazards(ctx, &level, time_alive);
                    follow_camera(
                        ctx,
                        &mut cam_eye,
                        &mut cam_target,
                        &level,
                        player_pos,
                        Vec2::ZERO,
                        dt,
                        2.0,
                    );

                    if ctx.input().key_pressed(Key::Left) || ctx.input().key_pressed(Key::A) {
                        if chapter_cursor > 0 {
                            chapter_cursor -= 1;
                            play_sfx(ctx, "ui.wav");
                        }
                    }
                    if ctx.input().key_pressed(Key::Right) || ctx.input().key_pressed(Key::D) {
                        let max = progress.unlocked_chapter.min(CHAPTERS.len() - 1);
                        if chapter_cursor < max {
                            chapter_cursor += 1;
                            play_sfx(ctx, "ui.wav");
                        }
                    }
                    for (i, key) in [Key::Digit1, Key::Digit2, Key::Digit3]
                        .into_iter()
                        .enumerate()
                    {
                        if ctx.input().key_pressed(key) && i <= progress.unlocked_chapter {
                            chapter_cursor = i;
                            play_sfx(ctx, "ui.wav");
                        }
                    }

                    draw_flash(ctx, flash, flash_color);
                    ctx.ui().rect(
                        0.0,
                        0.0,
                        1.0,
                        1.0,
                        Color::rgba(0.02, 0.03, 0.07, 0.62),
                    );
                    let header = "CHAPTERS";
                    let hs = 0.06;
                    ctx.ui().text(
                        centered_x(header, hs),
                        0.14,
                        hs,
                        Color::WHITE,
                        header,
                    );

                    for (i, ch) in CHAPTERS.iter().enumerate() {
                        let y = 0.30 + i as f32 * 0.14;
                        let locked = i > progress.unlocked_chapter;
                        let selected = i == chapter_cursor;
                        let label = if locked {
                            format!("{}  — locked", ch.name)
                        } else if selected {
                            format!("> {} <", ch.name)
                        } else {
                            ch.name.to_string()
                        };
                        let color = if locked {
                            Color::rgb(0.4, 0.42, 0.48)
                        } else if selected {
                            Color::rgb(0.35, 0.95, 0.85)
                        } else {
                            Color::rgb(0.8, 0.84, 0.9)
                        };
                        let size = if selected { 0.036 } else { 0.032 };
                        ctx.ui()
                            .text(centered_x(&label, size), y, size, color, &label);

                        if !locked {
                            let detail = match progress.chapter_best_sum(i) {
                                Some(sum) => format!(
                                    "Lv {}–{}  ·  best {:.1}s",
                                    ch.start + 1,
                                    ch.end,
                                    sum
                                ),
                                None => format!("Lv {}–{}", ch.start + 1, ch.end),
                            };
                            let ds = 0.022;
                            ctx.ui().text(
                                centered_x(&detail, ds),
                                y + 0.045,
                                ds,
                                Color::rgb(0.55, 0.6, 0.68),
                                &detail,
                            );
                        }
                    }

                    let prompt = "←/→ select   Space start   Esc back";
                    let ps = 0.024;
                    ctx.ui().text(
                        centered_x(prompt, ps),
                        0.88,
                        ps,
                        Color::rgb(0.65, 0.7, 0.78),
                        prompt,
                    );

                    if ctx.input().key_pressed(Key::Space)
                        && chapter_cursor <= progress.unlocked_chapter
                    {
                        play_sfx(ctx, "ui.wav");
                        let start = CHAPTERS[chapter_cursor].start;
                        let _ = load_level_into(
                            ctx,
                            start,
                            &mut level,
                            &mut player_pos,
                            &mut controller,
                            &mut phase,
                            &mut elapsed,
                            &mut squash,
                            &mut flash,
                            &mut velocity_xz,
                            &mut cam_eye,
                            &mut cam_target,
                            &mut physics_ready,
                            &mut level_index,
                        );
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
                            &mut controller,
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
                    let result = controller.move_planar(ctx.physics(), wish, speed, dt);
                    player_pos = result.position;
                    velocity_xz = Vec2::new(controller.velocity.x, controller.velocity.z);

                    if result.hit && velocity_xz.length_squared() > 0.01 {
                        squash = squash.max(0.45);
                        if bump_cd <= 0.0 {
                            burst_bump(ctx, player_pos);
                            bump_cd = 0.18;
                        }
                    }

                    apply_player_visual(ctx, &level, player_pos, squash, 1.0);
                    spin_goal(ctx, dt * 1.8);
                    pulse_hazards(ctx, &level, time_alive);
                    if sparkle_cd <= 0.0 {
                        goal_sparkle(ctx, &level);
                        sparkle_cd = 0.4;
                    }
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
                        play_sfx_at(ctx, "fail.wav", player_pos);
                        burst_fail(ctx, player_pos);
                    } else if hit_goal {
                        phase = Phase::Won;
                        clear_time = elapsed;
                        clear_improved = progress.record_clear(level_index, clear_time);
                        progress.save();
                        all_clear = level_index + 1 >= LEVEL_FILES.len();
                        squash = 0.7;
                        flash = 0.85;
                        flash_color = Color::rgb(0.2, 0.95, 0.85);
                        play_sfx_at(ctx, "win.wav", level.goal_center);
                        burst_win(ctx, level.goal_center);
                    }

                    draw_flash(ctx, flash, flash_color);
                    draw_hud(ctx, level_index, elapsed, &progress);
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
                        "CAMPAIGN CLEAR"
                    } else {
                        "CLEAR"
                    };
                    let hs = if all_clear { 0.065 } else { 0.08 };
                    ctx.ui().text(
                        centered_x(headline, hs),
                        0.30,
                        hs,
                        Color::rgb(0.35, 1.0, 0.9),
                        headline,
                    );
                    let time_line = if clear_improved {
                        format!("Time {clear_time:.2}s  ·  new best!")
                    } else {
                        format!("Time {clear_time:.2}s")
                    };
                    let ts = 0.032;
                    ctx.ui().text(
                        centered_x(&time_line, ts),
                        0.42,
                        ts,
                        Color::WHITE,
                        &time_line,
                    );
                    if let Some(best) = progress.best.get(level_index).copied().flatten() {
                        let best_line = format!("Best {best:.2}s");
                        let bs = 0.026;
                        ctx.ui().text(
                            centered_x(&best_line, bs),
                            0.48,
                            bs,
                            Color::rgb(0.7, 0.85, 0.9),
                            &best_line,
                        );
                    }
                    let prompt = if all_clear {
                        "Space — chapters"
                    } else {
                        "Space — next level"
                    };
                    let ps = 0.03;
                    ctx.ui().text(
                        centered_x(prompt, ps),
                        0.58,
                        ps,
                        Color::rgb(0.75, 0.8, 0.88),
                        prompt,
                    );

                    if ctx.input().key_pressed(Key::Space) {
                        play_sfx(ctx, "ui.wav");
                        if all_clear {
                            phase = Phase::ChapterSelect;
                            chapter_cursor = progress.unlocked_chapter.min(CHAPTERS.len() - 1);
                        } else if !advance_level(
                            ctx,
                            &mut level_index,
                            &mut level,
                            &mut player_pos,
                            &mut controller,
                            &mut phase,
                            &mut elapsed,
                            &mut squash,
                            &mut flash,
                            &mut velocity_xz,
                            &mut cam_eye,
                            &mut cam_target,
                            &mut physics_ready,
                        ) {
                            phase = Phase::ChapterSelect;
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

                    if ctx.input().key_pressed(Key::Space) || ctx.input().key_pressed(Key::R) {
                        play_sfx(ctx, "ui.wav");
                        reset_level(
                            ctx,
                            &level,
                            &mut player_pos,
                            &mut controller,
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
    controller: &mut CharacterController,
    phase: &mut Phase,
    elapsed: &mut f32,
    squash: &mut f32,
    flash: &mut f32,
    velocity_xz: &mut Vec2,
) {
    *player_pos = level.player_start;
    *controller =
        CharacterController::planar(level.player_start, level.player_half).with_max_speed(5.2);
    *phase = Phase::Playing;
    *elapsed = 0.0;
    *squash = 0.3;
    *flash = 0.0;
    *velocity_xz = Vec2::ZERO;
    apply_player_visual(ctx, level, *player_pos, *squash, 1.0);
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
    // Third-person follow: elevated chase cam with light look-ahead.
    let look_ahead = Vec3::new(velocity_xz.x, 0.0, velocity_xz.y) * 0.4;
    let desired_target = Vec3::new(player_pos.x, 0.45, player_pos.z) + look_ahead;
    let mut desired_eye = desired_target + level.cam_eye_offset;
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

fn draw_hud(ctx: &mut Context<'_>, level_index: usize, elapsed: f32, progress: &Progress) {
    let ch = chapter_for_level(level_index);
    let ch_name = CHAPTERS[ch].name;
    let level_name = LEVEL_NAMES.get(level_index).copied().unwrap_or("?");
    let level_line = format!(
        "{}  ·  {}/{}  {}",
        ch_name,
        level_index + 1,
        LEVEL_FILES.len(),
        level_name
    );
    let time_line = match progress.best.get(level_index).copied().flatten() {
        Some(best) => format!("{elapsed:.1}s  ·  best {best:.1}s"),
        None => format!("{elapsed:.1}s"),
    };
    ctx.ui()
        .text(0.03, 0.03, 0.024, Color::rgb(0.9, 0.92, 0.95), &level_line);
    ctx.ui()
        .text(0.03, 0.07, 0.026, Color::rgb(0.75, 0.9, 0.95), &time_line);
    ctx.ui().text(
        0.03,
        0.94,
        0.02,
        Color::rgb(0.65, 0.68, 0.74),
        "WASD move  R retry  Esc chapters",
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

    /// Surface gap between two AABBs along an axis (positive = separation).
    fn aabb_gap_axis(a_c: f32, a_h: f32, b_c: f32, b_h: f32) -> f32 {
        let a_min = a_c - a_h;
        let a_max = a_c + a_h;
        let b_min = b_c - b_h;
        let b_max = b_c + b_h;
        if a_max < b_min {
            b_min - a_max
        } else if b_max < a_min {
            a_min - b_max
        } else {
            0.0
        }
    }

    /// Facing wall pairs that form a narrow gate must leave ≥ 1.0 clear space.
    fn min_gate_gap(walls: &[(Vec3, Vec3)]) -> Option<f32> {
        let mut min_gap = f32::INFINITY;
        for i in 0..walls.len() {
            for j in (i + 1)..walls.len() {
                let (ac, ah) = walls[i];
                let (bc, bh) = walls[j];
                let dx = (ac.x - bc.x).abs();
                let dz = (ac.z - bc.z).abs();
                if dx < 0.15 && dz > 0.5 && ah.z >= 0.5 && bh.z >= 0.5 {
                    let gap = aabb_gap_axis(ac.z, ah.z, bc.z, bh.z);
                    if gap > 0.05 && gap < 4.0 {
                        min_gap = min_gap.min(gap);
                    }
                }
                if dz < 0.15 && dx > 0.5 && ah.x >= 0.5 && bh.x >= 0.5 {
                    let gap = aabb_gap_axis(ac.x, ah.x, bc.x, bh.x);
                    if gap > 0.05 && gap < 4.0 {
                        min_gap = min_gap.min(gap);
                    }
                }
            }
        }
        if min_gap.is_finite() {
            Some(min_gap)
        } else {
            None
        }
    }

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
            components: Default::default(),
            extras: Default::default(),
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
            components: Default::default(),
            extras: Default::default(),
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
    fn campaign_has_ten_plus_levels_and_three_chapters() {
        assert!(
            LEVEL_FILES.len() >= 10,
            "M5 accept: need at least 10 Reach levels, got {}",
            LEVEL_FILES.len()
        );
        assert_eq!(LEVEL_FILES.len(), LEVEL_NAMES.len());
        assert_eq!(CHAPTERS.len(), 3);
        assert_eq!(CHAPTERS[0].start, 0);
        assert_eq!(CHAPTERS.last().unwrap().end, LEVEL_FILES.len());
        for w in CHAPTERS.windows(2) {
            assert_eq!(w[0].end, w[1].start);
        }
    }

    #[test]
    fn all_level_files_load_with_required_roles() {
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
                (level.player_half.x - 0.5).abs() < 1e-3
                    && (level.player_half.y - 0.5).abs() < 1e-3
                    && (level.player_half.z - 0.5).abs() < 1e-3,
                "{file}: expected unit-cube player half=0.5, got {:?}",
                level.player_half
            );
            assert!(!level.hazards.is_empty(), "{file}: expected hazards");
            if let Some(gap) = min_gate_gap(&level.walls) {
                assert!(
                    gap + 1e-3 >= 1.0,
                    "{file}: gate dodge gap {gap:.3} < 1.0"
                );
            }
        }
    }

    #[test]
    fn progress_parse_roundtrip() {
        let text = "\
# comment
v1
unlock 1
best 0 12.500
best 3 20.125
";
        let p = Progress::parse(text).expect("parse");
        assert_eq!(p.unlocked_chapter, 1);
        assert_eq!(p.best[0], Some(12.5));
        assert_eq!(p.best[3], Some(20.125));
        assert!(p.best[1].is_none());
    }

    #[test]
    fn progress_record_clear_unlocks_next_chapter() {
        let mut p = Progress::fresh();
        for i in 0..4 {
            p.record_clear(i, 10.0 + i as f32);
        }
        assert_eq!(p.unlocked_chapter, 1);
        assert!(p.chapter_best_sum(0).is_some());
    }

    #[test]
    fn progress_path_is_named_reach_progress() {
        let path = progress_path();
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some("reach_progress.txt")
        );
        assert!(path.parent().is_some());
    }
}
