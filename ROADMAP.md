# Kerabit Summit Roadmap

Moonshot plan to level Kerabit into a serious tiny-engine competitor for small teams — still one composition for authors (`spawn` / `run` / scenes / editor / Rhai), deep enough to ship ambitious games.

**Install stays the same:** [rustup](https://rustup.rs/) + `git clone` + `cargo run -p …` (no new installer).

**Version path:** **`1.0.0`** Summit M0–M9 · **`2.0.0`** Scripting Summit (M10–M15) — rich Rhai host API + script-first proof game **Spark**.

## Phase map (M0–M9) — complete

| Phase | Focus | Accept (summary) |
|-------|--------|------------------|
| **M0** — Foundations | ROADMAP, alpha.2, 3-OS CI, scene `components` / `extras` prep | CI green on macOS + Windows + Ubuntu; docs link this roadmap |
| **M1** — Render leap | PBR-lite, multi-light, tonemap/bloom, particles | **Done** — `pbr_room`; Reach still builds |
| **M2** — Simulation | `kerabit-anim`, dynamics + character controller, entity queries | Unit tests + `physics_sandbox`; Reach optional controller |
| **M3** — Audio | Spatial attenuation, mix buses, streaming music | Surge/Reach spatial cues; null-safe without a device |
| **M4** — Editor | In-viewport play, undo, multi-select, prefabs | Author a Reach level entirely in the editor |
| **M5** — Reach campaign | 10+ levels, chapters, juice | **Done** — 16 levels / 4 chapters; `cargo run -p reach` |
| **M6** — Surge + Showcase | Surge modes + `games/showcase` trailer scene | **Done** — timed + endless; `cargo run -p showcase` |
| **M7** — Product | Docs site, downloads, Windows packaging | Stranger: site → clone → hello → editor in &lt; 30 min |
| **M8** — Hardening | Clippy CI, frustum cull, 10k cube perf, bug sweep | Interactive 10k cubes; no known P0s |
| **M9** — Kerabit 1.0 | `1.0.0` freeze, GitHub Release, site launch | All prior gates green; tagged on `main` |

## Phase map (M10–M15) — Scripting Summit (2.0)

| Phase | Focus | Accept |
|-------|--------|--------|
| **M10** — Baseline | Rhai runtime + getters + mouse + `apply_scene` path fix | `hello_rhai` green; docs linked |
| **M11** — World authorship | spawn/despawn/tags/enable/scale + prefab | Script creates/destroys entities |
| **M12** — Sim & juice | `move_planar`, audio, particles, camera | Script-driven mover |
| **M13** — Author UX | hot-reload, UI host, editor Check/Reload | Edit `.rhai` → Play sees change |
| **M14** — Proof game | `games/spark` script-first playable | `cargo run -p spark` |
| **M15** — Product | Version `2.0.0`, docs/site, freeze host table | Release notes match host API |

## Tracks

Parallel work owns a track, not the whole monorepo:

- **Engine-Render** — `kerabit-render`, shaders
- **Engine-Simulation** — `kerabit-world`, `kerabit-physics`, `kerabit-anim`
- **Engine-Audio** — `kerabit-audio`
- **Engine-Script** — `kerabit-script` (Rhai host)
- **Editor** — `tools/kerabit-editor`
- **Games** — `games/reach`, `games/surge`, `games/showcase`, `games/spark`
- **Product** — `site/`, `.github/`, docs, packaging

## Locked decisions (1.0 / 2.0)

- Public game API stays tiny; breaks only with semver + CHANGELOG
- Editor stays egui in `tools/`; never leak egui into `kerabit`
- Platforms: macOS + Windows + Linux compile/run; player zips at least macOS + Windows
- Scripting language = **Rhai** (`kerabit-script`); 2.0 freezes the rich host table in [API.md](API.md)

## Non-goals

Full ECS/Bevy layer, visual scripting, networking, mobile/console stores, bundling a DCC, rewriting Reach into Rhai.

## Status

**M0–M9** — done (**Kerabit 1.0.0**)  
**M10–M15** — done (**Kerabit 2.0.0** Scripting Summit)

See also [ARCHITECTURE.md](ARCHITECTURE.md), [API.md](API.md), and [README.md](README.md).
