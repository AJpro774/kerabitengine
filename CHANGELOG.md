# Changelog

All notable changes to Kerabit are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/) with alpha
prerelease tags (`1.0.0-alpha.N`).

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
