# Kerabit Summit Roadmap

Moonshot plan to level Kerabit into a serious tiny-engine competitor for small teams — still one composition for authors (`spawn` / `run` / scenes / editor / Juni), deep enough to ship ambitious games.

**Install stays the same:** [rustup](https://rustup.rs/) + `git clone` + `cargo run -p …` (no new installer).

**Version path:** **`1.0.0`** Summit M0–M9 · **`2.0.0`** Scripting Summit (M10–M15) — rich Rhai host API + script-first proof game **Spark** · **`3.0.0`** Juni + UE5-class render tier (M16–M24) — Rhai replaced by compiled **Juni**, clustered lights / CSM / SSAO / IBL / SSR / TAA / LOD.

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

## Phase map (M16–M24) — Juni + UE5-class render tier (3.0)

Scripting moves to **Juni** (statically typed, compiled to WASM in-process via [Juno](https://github.com/AJpro774/Juno) `v13.0.0` `extern` host imports) and the renderer grows the pillars a UE5 user expects, on the same tiny footprint.

| Phase | Focus | Accept |
|-------|--------|--------|
| **M16** — Juni host | `kerabit-juni`: prelude, handle-based host table, wasmtime, `check_juni`; Rhai removed | `hello_juni` + Spark run from `.juni`; `cargo test` green |
| **M17** — Author port | Editor Juni panel, MCP `check_juni` / scaffold, docs + site | Stranger path uses Juni end to end |
| **M18** — G-buffer | Depth prepass + thin normal / roughness buffer shared by lit + post | Reach / Surge / Spark / editor render unchanged |
| **M19** — Clustered lights | Storage-buffer lights (256) + froxel light lists | 200 point lights at 60 fps |
| **M20** — Cascaded shadows | 4-cascade atlas, stable fit, cascade blend | No shimmer on Reach grounds |
| **M21** — SSAO + IBL | Half-res AO; HDR equirect → prefiltered cubemap + irradiance; scene `environment` | Showcase reads an `.hdr` |
| **M22** — SSR + TAA | Hierarchical-Z reflections with IBL fallback; motion vectors + jittered TAA resolve | Stable edges in motion |
| **M23** — LOD | Author-supplied LOD chain + distance selection on top of frustum culling | 10k cubes + 200 lights at 60 fps |
| **M24** — Kerabit 3.0 | Version `3.0.0`, CHANGELOG / site, Showcase exercises IBL + CSM + TAA, GitHub Release | Tagged on `main`; site live |

## Tracks

Parallel work owns a track, not the whole monorepo:

- **Engine-Render** — `kerabit-render`, shaders
- **Engine-Simulation** — `kerabit-world`, `kerabit-physics`, `kerabit-anim`
- **Engine-Audio** — `kerabit-audio`
- **Engine-Script** — `kerabit-juni` (Juni host + prelude)
- **Editor** — `tools/kerabit-editor`
- **Games** — `games/reach`, `games/surge`, `games/showcase`, `games/spark`, `games/strike`
- **Product** — `site/`, `.github/`, docs, packaging, `tools/kerabit-mcp`

## Locked decisions (1.0 / 2.0 / 3.0)

- Public game API stays tiny; breaks only with semver + CHANGELOG
- Editor stays egui in `tools/`; never leak egui into `kerabit`
- Platforms: macOS + Windows + Linux compile/run; player zips at least macOS + Windows
- Scripting language = **Juni** (`kerabit-juni`); 3.0 freezes the host table in [API.md](API.md) and the prelude `crates/kerabit-juni/juni/kerabit.juni`. The compiler is pinned by git tag to Juno; language changes land in Juno first.
- Rendering stays forward+ (clustered) on wgpu; no hardware ray tracing or virtualized geometry in 3.x

## Non-goals

Full ECS/Bevy layer, visual scripting, networking, mobile/console stores, bundling a DCC, rewriting Reach into Juni, Nanite-style geometry, hardware RT / full Lumen GI, an Actor/Component object model (3.x candidates).

## Status

**M0–M9** — done (**Kerabit 1.0.0**)  
**M10–M15** — done (**Kerabit 2.0.0** Scripting Summit)  
**M16–M24** — done (**Kerabit 3.0.0** Juni + render tier)

## After 3.0 — community

Git-folder mods (`mod.kerabit.json`, `ModIndex`, editor Mods window, `community/catalog.json`). No workshop store. See [API.md](API.md) and [site/docs/modding.html](site/docs/modding.html).

Windows player + editor zips: `scripts/package-windows.ps1`. User data is `~/.kerabit` / `%USERPROFILE%\.kerabit`. See [WINDOWS.md](WINDOWS.md).

See also [ARCHITECTURE.md](ARCHITECTURE.md), [API.md](API.md), and [README.md](README.md).
