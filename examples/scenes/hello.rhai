// Hello Rhai — runs every frame after the Rust `run` closure.
// `dt()` is seconds since the last frame.

if key_pressed("Escape") {
    quit();
}

rotate_y("cube", 1.1 * dt());
