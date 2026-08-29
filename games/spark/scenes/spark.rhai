// Spark — Kerabit 2.0 script-first micro-game.
// Prefer fn init() / fn update(); use set/get/has for persistent state.

fn init() {
    set("phase", "play"); // play | win | fail
    set("elapsed", 0.0);
    set("physics_ready", false);
    set("start_x", -5.0);
    set("start_z", 0.0);
}

fn reset_player() {
    set_pos("player", get("start_x"), 0.5, get("start_z"));
    set("elapsed", 0.0);
    set("phase", "play");
}

fn ensure_physics() {
    if get("physics_ready") {
        return;
    }
    let walls = names_with_tag("wall");
    for w in walls {
        register_box(w);
    }
    set("physics_ready", true);
}

fn dist_xz(ax, az, bx, bz) {
    let dx = ax - bx;
    let dz = az - bz;
    return sqrt(dx * dx + dz * dz);
}

fn update() {
    if key_pressed("Escape") {
        quit();
        return;
    }

    ensure_physics();

    let phase = get("phase");
    let p = pos("player");

    // Soft camera follow
    set_camera(p[0] * 0.15, 11.0, 14.0, p[0] * 0.2, 0.4, p[2] * 0.2);

    if phase == "win" {
        ui_rect(0.0, 0.0, 1.0, 1.0, 0.08, 0.1, 0.08, 0.55);
        ui_text(0.38, 0.36, 0.07, 0.91, 1.0, 0.29, "CLEAR");
        ui_text(0.28, 0.48, 0.028, 0.9, 0.9, 0.85, "Space - retry   Esc - quit");
        if key_pressed("Space") {
            reset_player();
        }
        return;
    }

    if phase == "fail" {
        ui_rect(0.0, 0.0, 1.0, 1.0, 0.18, 0.06, 0.05, 0.55);
        ui_text(0.36, 0.36, 0.07, 1.0, 0.35, 0.35, "RETRY");
        ui_text(0.28, 0.48, 0.028, 0.9, 0.82, 0.76, "Space - retry   Esc - quit");
        if key_pressed("Space") || key_pressed("R") {
            reset_player();
        }
        return;
    }

    // --- play ---
    set("elapsed", get("elapsed") + dt());

    let speed = 6.5;
    let dx = 0.0;
    let dz = 0.0;
    if key_down("W") || key_down("Up") { dz -= 1.0; }
    if key_down("S") || key_down("Down") { dz += 1.0; }
    if key_down("A") || key_down("Left") { dx -= 1.0; }
    if key_down("D") || key_down("Right") { dx += 1.0; }

    let len = sqrt(dx * dx + dz * dz);
    if len > 0.001 {
        dx = dx / len * speed * dt();
        dz = dz / len * speed * dt();
        move_planar("player", dx, dz);
    }

    // Spin hazards for juice
    rotate_y("hazard_a", 1.4 * dt());
    rotate_y("hazard_b", -1.1 * dt());

    p = pos("player");

    // Fell off platform?
    if p[1] < -2.0 {
        set("phase", "fail");
        spawn_particles(p[0], p[1], p[2], 28, 1.0, 0.4, 0.2);
        return;
    }

    // Hazard touch (XZ radius)
    let hazards = names_with_tag("hazard");
    for h in hazards {
        let hp = pos(h);
        if dist_xz(p[0], p[2], hp[0], hp[2]) < 1.05 {
            set("phase", "fail");
            spawn_particles(p[0], 0.8, p[2], 32, 1.0, 0.3, 0.15);
            return;
        }
    }

    // Goal
    let g = pos("goal");
    if dist_xz(p[0], p[2], g[0], g[2]) < 1.15 {
        set("phase", "win");
        spawn_particles(g[0], 1.0, g[2], 40, 0.3, 0.9, 1.0);
        return;
    }

    // HUD (ASCII only — overlay atlas)
    ui_text(0.03, 0.03, 0.028, 0.91, 1.0, 0.29, "SPARK");
    ui_text(0.03, 0.07, 0.022, 0.85, 0.82, 0.75, "WASD move  reach cyan  avoid orange");
    ui_text(0.03, 0.11, 0.02, 0.65, 0.62, 0.55, "Space retries after win/fail");
}
