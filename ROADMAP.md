# Kerabit Summit Roadmap

Moonshot plan to level Kerabit into a serious tiny-engine competitor for small teams — still one composition for authors (`spawn` / `run` / scenes / editor), deep enough to ship ambitious games.

**Install stays the same:** [rustup](https://rustup.rs/) + `git clone` + `cargo run -p …` (no new installer).

**Version path:** closed at **`1.0.0`** (Summit M0–M9 complete). Post-1.0 work follows the locked decisions below.

## Phase map (M0–M9)

| Phase | Focus | Accept (summary) |
|-------|--------|------------------|
| **M0** — Foundations | ROADMAP, alpha.2, 3-OS CI, scene `components` / `extras` prep | CI green on macOS + Windows + Ubuntu; docs link this roadmap |
| **M1** — Render leap | PBR-lite, multi-light, tonemap/bloom, particles | **Done** — `pbr_room`; Reach still builds |
| **M2** — Simulation | `kerabit-anim`, dynamics + character controller, entity queries | Unit tests + `physics_sandbox`; Reach optional controller |
| **M3** — Audio | Spatial attenuation, mix buses, streaming music | Surge/Reach spatial cues; null-safe without a device |
| **M4** — Editor | In-viewport play, undo, multi-select, prefabs | Author a Reach level entirely in the editor |
| **M5** — Reach campaign | 10+ levels, chapters, juice | **Done** — 12 levels / 3 chapters; `cargo run -p reach` |
| **M6** — Surge + Showcase | Surge modes + `games/showcase` trailer scene | **Done** — timed + endless; `cargo run -p showcase` |
| **M7** — Product | Docs site, downloads, Windows packaging | Stranger: site → clone → hello → editor in &lt; 30 min |
| **M8** — Hardening | Clippy CI, frustum cull, 10k cube perf, bug sweep | Interactive 10k cubes; no known P0s |
| **M9** — Kerabit 1.0 | `1.0.0` freeze, GitHub Release, site launch | All prior gates green; tagged on `main` |

## Tracks

Parallel work owns a track, not the whole monorepo:

- **Engine-Render** — `kerabit-render`, shaders
- **Engine-Simulation** — `kerabit-world`, `kerabit-physics`, `kerabit-anim`
- **Engine-Audio** — `kerabit-audio`
- **Editor** — `tools/kerabit-editor`
- **Games** — `games/reach`, `games/surge`, `games/showcase`
- **Product** — `site/`, `.github/`, docs, packaging

Merge spine: **engine foundations → editor → games → product.**

## Locked decisions (1.0)

- Public game API stays tiny; breaks only with semver + CHANGELOG
- Editor stays egui in `tools/`; never leak egui into `kerabit`
- Platforms: macOS + Windows + Linux compile/run; player zips at least macOS + Windows
- **No scripting in 1.0** — Rust + scenes + tags only (no Lua/JS/Rhai runtime in this release)

## Locked decisions (post-1.0)

- Scripting language = **Rhai**
- In-engine code editor deferred (follows Rhai runtime)
- egui stays in `tools/` (never in the game crate)

## Non-goals (even for moonshot 1.0)

Full ECS/Bevy layer, visual scripting, networking, mobile/console stores, bundling a DCC.

## Status

**M0** — done on `1.0.0-alpha.2`  
**M1** — done (PBR-lite, lights, post, particles)  
**M2** — done (`kerabit-anim`, dynamics + character controller, entity queries, `physics_sandbox`)  
**M3** — done (spatial audio, buses, music stream)  
**M4** — done (editor undo/multi-select/prefabs/snap/Play polish)  
**M5** — done (Reach 12-level / 3-chapter campaign)  
**M6** — done (Surge timed + endless modes, 5 arenas; `games/showcase` trailer)  
**M7** — done (site downloads; Reach macOS+Windows zips; tag-triggered `package-reach`)  
**M8** — done (Clippy CI, frustum cull, `MAX_INSTANCES=16384`, interactive ~10k cubes)  
**M9** — done (**Kerabit 1.0.0** freeze, GitHub Release, site launch)

See also [ARCHITECTURE.md](ARCHITECTURE.md), [API.md](API.md), and [README.md](README.md).
