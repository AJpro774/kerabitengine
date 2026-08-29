# Changelog

All notable changes to Kerabit are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/).

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
