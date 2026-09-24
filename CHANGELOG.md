# Changelog

All notable changes to Kerabit are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- **Community mods** — folder packs with `mod.kerabit.json`, discovered from `./mods`, `~/.kerabit/mods`, and `KERABIT_MODS`. Enable list in `~/.kerabit/mods.json`. `ModIndex` (`discover`, `extra_scenes`, `resolve`, `scaffold`). Editor Mods window (Play / Open). Sample `mods/hello-cube`. Git catalog `community/catalog.json`. MCP `kerabit_list_mods` / `kerabit_scaffold_mod`.
- **Windows parity** — one user-data dir (`user_data_dir`: `~/.kerabit` / `%USERPROFILE%\.kerabit`, `KERABIT_HOME`). Games resolve data next to the `.exe` (`packaged_data_root`). `scripts/package-windows.ps1` + `package-windows.yml` zip Reach, Surge, Spark, Strike, Showcase, and the editor. MCP `kerabit_stop` uses `taskkill` on Windows. Move guide: [WINDOWS.md](WINDOWS.md).

## [3.0.0] — 2026-09-18

**Juni + UE5-class render tier.** Scripting moves from Rhai to compiled [Juni](https://github.com/AJpro774/Juno) (**breaking**: `.rhai` files no longer load), and the renderer gains the pillars of a modern real-time pipeline on the same tiny footprint. Frozen 1.0 Rust APIs are unchanged; new surfaces are additive.

### Added

- **Juni scripting (`kerabit-juni`, Frozen for 3.0)** — `.juni` scripts (statically typed, Python-like indentation) compile in-process via Juno `v13.0.0` `extern` host imports against the prelude [`crates/kerabit-juni/juni/kerabit.juni`](crates/kerabit-juni/juni/kerabit.juni) and run in a fuel-limited wasmtime store. `fn main() -> i32` once at load, `fn frame(dt: f32) -> i32` per frame, `state:` for persistence, entities as `i32` handles (`entity("name")`, `self_entity()`, `tag_count` / `tag_at`). Type errors fail load with `file:line:col`; a runaway loop traps and pauses that script (sticky `ctx.script_error()` until the file hot-reloads). Juno browser builtins are rejected with a hint. `cargo run -p kerabit-juni --bin check_juni -- script.juni`.
- **Depth prepass + G-buffer** — world normal / roughness and motion vectors; the lit pass depth-tests only (no overdraw shading).
- **Clustered lighting** — `MAX_LIGHTS` 4 → **256** (≤4 directional) in a storage buffer; a compute pass bins point lights into a 16×9×24 froxel grid. 10k cubes + 200 point lights hold 60 fps on Apple Silicon (`cargo run -p kerabit --example many_cubes --release`).
- **Cascaded shadow maps** — four 2048² cascades to 120 units, practical splits, texel-snapped stable fit, PCF + cascade blending.
- **SSAO** — half-res hemisphere kernel + depth-aware blur on ambient / irradiance.
- **Image-based lighting** — scene `"environment": { "hdr", "intensity" }`, `Kerabit::environment`, `Context::set_environment` / `clear_environment`: equirect Radiance `.hdr` → prefiltered specular cube + split-sum BRDF LUT + SH9 irradiance. Without an environment the procedural sky lights the scene.
- **Screen-space reflections** — Hi-Z assisted march with prefiltered-environment fallback, composited through a per-pixel specular weight.
- **TAA** — Halton jitter, prepass motion vectors (camera reprojection for background), neighborhood-clamped history.
- **Mesh LODs** — `Entity::lod(mesh, distance)` and scene `"lods": [{ "mesh", "distance" }]`; chosen per frame by camera distance, shared by shadows and the prepass.
- **`RenderSettings`** / `KERABIT_RENDER_DEBUG` — toggle SSAO / IBL / SSR / TAA and view intermediate stages (`lit`, `resolved`).
- **`HdrImage`** loader in `kerabit-assets` (`image` `hdr` feature).
- **Kerabit MCP** (`tools/kerabit-mcp`) — Node/TS stdio MCP for docs, scene/script IO + validate, `kerabit_check_juni`, Juni scaffold, and `cargo run` / stop.
- **Strike** — `cargo run -p strike`; first-person arena with hitscan, chasing AI dummies, and Sketchfab CC-BY props (`games/strike`).

### Changed

- Workspace version → **`3.0.0`**; `SceneRenderer` now backs games, headless capture, and the editor viewport (one pass chain, bloom + ACES on top).
- `hello_rhai` → `hello_juni`; `games/spark/scenes/spark.juni` replaces `spark.rhai` (same gameplay); editor Script panel edits `.juni` and **Check** type-checks against the host API.
- Showcase draws a 48-light clustered ring and reads the new tier's features in its HUD.
- `InstanceRaw` grew to 160 B (previous model matrix); `FrameUniforms` is a new layout — `kerabit-render` internals, not the game API.

### Removed

- `crates/kerabit-script` and the `rhai` dependency. Rhai host functions have Juni equivalents (see API.md § Juni); `set` / `get` / `has` become a `state:` block.

## [2.0.0] — 2026-08-28

**Scripting Summit** — rich Rhai host API so authors can ship small games mostly in `.rhai`. Frozen 1.0 Rust APIs unchanged.

### Added

- **Rich Rhai host (Frozen for 2.0)** — reads (`pos` / `get_pos`, `scale`, `enabled`, `has_tag`), world writes (`set_scale`, `set_enabled`, `add_tag` / `remove_tag`, `despawn`, `spawn_cube` / `spawn_cube_ex` / `spawn_plane`, `spawn_prefab`), input (`mouse_pos`, `mouse_button_down` / `pressed`), sim (`move_planar`, `register_box`), juice (`play` / `play_at`, `spawn_particles`, `set_camera`), UI (`ui_text` / `ui_rect`, ASCII overlay), persistence (`set` / `get` / `has`), `reload_scripts()`, optional `fn init()` / `fn update()`.
- Auto **mtime hot-reload** of loaded `.rhai` files; editor Script panel **Check** + **Reload**.
- **`apply_scene`** resolves script paths via the last scene directory (same rule as `load_scene`).
- **Spark** — `cargo run -p spark`; logic in `games/spark/scenes/spark.rhai`.
- Site scripting guide expanded for the 2.0 host table.

### Changed

- Workspace version → **`2.0.0`**.
- Rhai baseline (runtime, scene `extras.script`, editor panel, `hello_rhai`) lands as part of 2.0.

## [1.1.0] — 2026-08-12

Additive **Rhai** scripting + in-engine code editor notes (superseded / folded into 2.0.0 ship). Frozen 1.0 Rust APIs unchanged.

### Added

- **`kerabit-script`** — Rhai runtime with a tiny host API (`dt`, `key_down` / `key_pressed`, `rotate_*`, `translate` / `set_pos`, `exists`, `names_with_tag`, `quit`).
- Scene hook: `extras.script` or `components.script` (path relative to the `.kerabit.json`) on the scene root or an entity (`self` binds to that entity name).
- Scripts tick automatically after the Rust `run` closure. `Kerabit::load_scene` / `Context::load_scene` compile them; `Kerabit::script` attaches a file without a scene bag. `ctx.script_error()` for overlay text.
- Example: `cargo run -p kerabit --example hello_rhai` (`examples/scenes/hello.rhai`).
- **Editor** — Script menu + bottom code panel (egui `TextEdit`); inspector / environment `extras.script` fields; Play loads scripts. egui stays in `tools/`.
- **Reach** — Summit (level 12) dodge path opened. Campaign is **16 levels / 4 chapters** (IV · Afterglow).

### Changed

- Workspace version → **`1.1.0`** (historical; current line is 2.0.0).

## [1.0.0] — 2026-08-10

Kerabit **1.0** — Summit ship. Install stays rustup + cargo. Reach player zips on GitHub Releases.

### Added

- **Reach players** — `Reach-macos.zip` (Reach.app) and `Reach-windows.zip` (`reach.exe` + `levels/` + `assets/`); tag-triggered `package-reach` workflow.
- **Hardening (M8)** — Clippy in CI; `MAX_INSTANCES` raised to **16384**; frustum culling; `many_cubes` interactive ~10k cubes.
- **Editor** — undo/redo, multi-select, prefabs, snap persistence, in-viewport Play (egui remains in `tools/` only).
- **Games** — Reach 12-level / 3-chapter campaign; Surge timed + endless; Showcase trailer crate.
- **Engine depth (Summit)** — PBR-lite, ≤4 lights, tonemap/bloom, particles; dynamics + character controller; spatial audio + mix buses; scene `components` / `extras`.
- Docs site (Getting Started, API tour, Editor) at [kerabitengine.vercel.app](https://kerabitengine.vercel.app).

### Changed

- Workspace version → **`1.0.0`**.
- Public API freeze table in [API.md](API.md) retargeted from alpha to **1.0** (experimental surfaces unchanged: editor UI, Surge motion tags).
- Site / README / roadmap flipped from alpha.2 working branch to Kerabit 1.0.

### Notes

- **No scripting in 1.0** — Rust + scenes + tags only. Post-1.0: scripting language = **Rhai**.
- Breaking changes to **Frozen for 1.0** APIs require a semver bump and a CHANGELOG entry.
## [1.0.0-alpha.2] — 2026-08-01

Summit M0–M7 working branch toward 1.0 (install stays rustup + cargo).

### Added

- [ROADMAP.md](ROADMAP.md) — Summit moonshot phases M0–M9 (install stays rustup + cargo).
- CI: `cargo check` + `cargo test` on **macOS**, **Windows**, and **Ubuntu** (no GPU smoke).
- Scene schema: reserved additive `components` / `extras` JSON objects on scene root and entities (`SceneMap`); `SCENE_VERSION` remains **1**.
- **M1 render:** PBR-lite (`Material::metallic`, optional normal maps), up to **4** lights (`Light::point`, `Kerabit::lights` / `ctx.set_lights`), HDR tonemap + cheap bloom, `ctx.spawn_particles(ParticleBurst)`, example `pbr_room`; scene `"metallic"` additive (default `0`).
- **M2 sim:** `kerabit-anim` clip playback; physics dynamics (`DynamicBody`, `step`) + `CharacterController`; world enable/tags/layers; `physics_sandbox` example; Reach uses planar controller.
- **M3 audio:** stereo positional `play_at` / listener (`follow_look_at`, `ctx.sync_audio_listener`), mix buses (`MixBus::{Sfx,Music}`), streaming `play_music` (WAV); null-safe when no device. Reach/Surge spatial hit cues.
- **M4 editor:** undo/redo for scene mutations; multi-select + duplicate/align; `.kerabit.prefab.json` (`Prefab` API + editor Save/Instance); gizmo snap settings persisted to `~/.kerabit/editor.json`; Play child-process polish (temp snapshot for dirty scenes, selection restored on return). Sample prefabs under `games/reach/prefabs/`.
- **M5 Reach campaign:** 12 tagged levels across 3 chapters, chapter select + best-time persistence (`~/.kerabit/reach_progress.txt`), win/fail/bump particles + spatial SFX.
- **M6 Surge + Showcase:** Surge timed ranked + endless modes, 5 arenas, best-score persistence; new `games/showcase` trailer (`cargo run -p showcase`) — PBR room, multi-light, particles.
- **M7 product:** static docs under `site/docs/` (Getting Started, API tour, Editor guide); site download/changelog/docs links; `scripts/package-reach-windows.ps1`; optional CI workflow `.github/workflows/package-reach.yml` (`workflow_dispatch` artifacts for macOS + Windows zips).

### Changed

- Workspace version → `1.0.0-alpha.2`.
- README / ARCHITECTURE briefly point at the Summit roadmap.
- Lit path: HDR → tonemap/bloom; soft shadows still from the first directional light only.
- Marketing site: stranger ≤30-min path, docs hub, Reach zip notes for macOS + Windows.

## [1.0.0-alpha.1] — 2026-07-31

First Alpha v1.0 cut for authors cloning the engine.

### Added

- Dual-license texts: `LICENSE-MIT` and `LICENSE-APACHE` (MIT OR Apache-2.0).
- GitHub Actions CI: `cargo check` + `cargo test` on macOS; `cargo check` on Ubuntu.
- Alpha API freeze table in [API.md](API.md) (frozen vs experimental surfaces).
- Marketing site clone/install section matching README cargo commands.

### Changed

- Workspace version set to `1.0.0-alpha.1`.
- `repository` metadata corrected to `https://github.com/AJpro774/kerabitengine`.
- CONTRIBUTING trimmed for alpha newcomers (stale P0–P7 session table removed).

### Notes

- **Install unchanged:** Rust stable via `rustup`, then `git clone` + `cargo run -p …`.
- Breaking changes to **Frozen for alpha** APIs require a new alpha minor bump and a CHANGELOG entry.
- Not in this alpha: crates.io publish, large renderer features. Windows player packaging lands in alpha.2 (M7).
