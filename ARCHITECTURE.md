# Architecture

Kerabit is a multi-crate Cargo workspace. Game authors depend only on **`kerabit`**. Internals may use wgpu/winit; those types must never leak through the public facade.

**Summit moonshot:** M0–M9 (1.0), M10–M15 Scripting Summit (2.0), and M16–M24 Juni + render tier (3.0) live in [ROADMAP.md](ROADMAP.md). Install remains rustup + cargo.

## Crate map

| Crate | Role | Phase |
|-------|------|-------|
| `kerabit` | Public facade / builder API | P3 |
| `kerabit-math` | `glam` re-exports, `vec3`, `Deg`/`Rad`, look-at | P0 |
| `kerabit-color` | `Color`, named constants | P0 |
| `kerabit-world` | Entities, transforms, hierarchy, tags/layers/enable | P2–P4 / M2 |
| `kerabit-input` | Input snapshot, `Key`, mouse | P3 |
| `kerabit-render` | Device, pipelines, GPU meshes, shaders | P1+ |
| `kerabit-assets` | OBJ / PNG / glTF loaders | P5 |
| `kerabit-physics` | AABB / raycast / dynamics / character controller | P6 / M2 |
| `kerabit-anim` | Clip playback on transform hierarchies | M2 |
| `kerabit-audio` | Playback / spatial / buses / streaming music | P6 / M3 |
| `kerabit-juni` | Juni compiler embed (`juni-driver`) + wasmtime host API | 3.0 |
| `kerabit-editor` (`tools/`) | Dev-only egui level editor + Juni panel | E1–E2 / 3.0 |

```
kerabit
  ├── kerabit-math
  ├── kerabit-color
  ├── kerabit-world      → math
  ├── kerabit-input
  ├── kerabit-render     → math, color
  ├── kerabit-assets     → render
  ├── kerabit-physics    → math
  ├── kerabit-anim       → math, world
  ├── kerabit-audio
  └── kerabit-juni       → juni-driver / juni-check (git, Juno v13.0.0), wasmtime, world, input, math

tools/kerabit-editor → kerabit + kerabit-render (+ egui; not shipped with games)
```

**Scripting boundary (3.0):** `kerabit-juni` compiles a `.juni` file in-process against the prelude [`juni/kerabit.juni`](crates/kerabit-juni/juni/kerabit.juni) (an `export extern "kerabit":` block plus Juni helpers) and instantiates the WASM with a `wasmtime::Linker` that supplies the `kerabit` module and the pure `env` builtins (math / strings / print). Each frame the runtime snapshots the world (names → positions / scales / tags) into a `Host`, moves it into each script's `Store` for `frame(dt)`, and takes it back with queued `WorldOp`s + `ScriptEffects`, which `Context` applies afterwards. Scripts never hold engine references; entity handles are interned names. Fuel (`consume_fuel`) bounds each call so a runaway loop traps instead of hanging the frame.

**Editor boundary:** egui lives only in `tools/kerabit-editor`. The 3D viewport renders the live `Scene` through [`OffscreenLitRenderer`](crates/kerabit-render/src/offscreen.rs) (same lit path as games) into an egui paint callback. Picking helpers (`ray_from_ndc`, mesh AABB, `pick_closest`) live in `kerabit-render` so the game API stays free of UI crates.
Shaders live in `crates/kerabit-render/shaders/` as `.wgsl` files included via `include_str!`.

## Frame loop (P4+)

1. Pump window events → update [`kerabit_input::InputState`]
2. Clear UI draw list; call game `run` closure with [`Context`](API.md) (`dt`, input, world, camera, physics, audio, `ui`, quit; E0 also wires GPU + renderables for `apply_scene` / `despawn` / `spawn`)
3. Tick loaded Juni scripts (`kerabit-juni`) against the same world / input / quit
4. Clear input edges / mouse delta (`end_frame`)
5. [`World::update_world_matrices`] — dirty local TRS, then parent→child world matrices
6. Build draw list from **enabled** world entities + per-entity mesh / albedo / roughness / LOD chain (world matrix + last frame's matrix for motion vectors); despawned entities must leave the renderable map (`Context::despawn` / `clear_world`)
7. [`SceneRenderer`](crates/kerabit-render/src/scene_renderer.rs) (shared by games, headless capture, and the editor viewport): frustum-cull + pick LODs → pack instances by `MeshId` → upload lights (directional first) → **cluster** compute (froxel light lists) → **4 cascaded shadow** passes → **depth prepass + G-buffer** (normal / roughness, velocity) → **Hi-Z** pyramid → **SSAO** (half-res + depth-aware blur) → **sky** where depth is still far → **lit** (clustered PBR, CSM PCF, SH irradiance; writes color + specular weight) → **specular resolve** (SSR with prefiltered-cube fallback) → **particles** → **TAA** resolve → **bloom + ACES tonemap** to swapchain → **overlay** (UI), present

**EventLoop / reload:** One winit `EventLoop` per process (thread-local + `run_app_on_demand`). Mid-run [`Context::apply_scene`](API.md) clears world + renderables + physics and respawns a `Scene` without recreating the window — preferred for level transitions (Reach) and future editor Play. Re-entering `Kerabit::run` still works but rebuilds App/window.

**Overlay pass:** game code queues `ctx.ui().rect` / `ctx.ui().text` in normalized top-left `0..=1` space. The engine expands text into atlas-sampled quads and draws them after 3D. See [`API.md`](API.md) § UI overlay.

**P2 render harnesses** remain: `cargo run -p kerabit-render --example two_meshes`.  
**P3 flagship:** `cargo run -p kerabit --example playground`.  
**3.0 Juni:** `cargo run -p kerabit --example hello_juni` · proof game: `cargo run -p spark`.  
**M1 PBR room:** `cargo run -p kerabit --example pbr_room`.  
**P4 stress:** `cargo run -p kerabit --example many_cubes --release`.  
**P5 assets:** `cargo run -p kerabit --example load_mesh`.  
**P6 physics + audio:** `cargo run -p kerabit --example physics_audio`.  
**Flagship game:** `cargo run -p reach` (`games/reach`, 16 `.kerabit.json` levels + HUD overlay; in-process `apply_scene` between levels).
**Second game (E7/M6):** `cargo run -p surge` (`games/surge`, timed ranked + endless; 5 arenas; public API + shared tags + `orbit`/`slide_*` motion tags).
**Showcase (M6):** `cargo run -p showcase` — non-game Summit render trailer (PBR, lights, particles).
**P7 legacy slice:** `cargo run -p kerabit --example mini_game` (loads `examples/scenes/mini_game.kerabit.json`).

## GPU resource model (3.0 render tier)

- **Frame uniforms** (`shaders/frame.wgsl`, prepended to every scene shader): jittered view-proj / view / proj + inverses, previous view-proj, camera, ambient + environment intensity, near/far + light counts, screen size, cluster grid params, jitter, 4 cascade matrices + splits, shadow params, 9 SH coefficients, feature flags (`FrameUniforms`)
- **Lights**: storage buffer of up to **256** `GpuLight`s (≤4 directional first, then point). `cluster.wgsl` bins point lights into a **16×9×24** froxel grid (≤64 per cluster); `lit.wgsl` walks its cluster's list per fragment
- **Instance buffer**: model + previous model + albedo + roughness + metallic (`InstanceRaw`, 160 B); batches by mesh + albedo + normal tex (≤16384 instances/frame). `prepare_draws` frustum-culls and resolves each `DrawItem`'s LOD chain by camera distance
- **Mesh GPU cache**: CPU `Mesh` → content-hash dedupe → `MeshId` → vertex/index buffers
- **Material**: albedo / roughness / metallic in instance attrs; albedo + normal bind group (white / flat-normal defaults)
- **G-buffer** (`gbuffer.rs`): depth prepass writes `Depth32Float` + world normal / roughness (`Rgba16Float`) + screen-space velocity (`Rg16Float`); the lit pass then depth-tests only (no overdraw shading). A **Hi-Z** min-depth pyramid (5 mips) feeds SSR
- **Cascaded shadows** (`shadow.rs`): 4 × 2048² `Depth32Float` array, practical split scheme to 120 units, texel-snapped stable fit; 3×3 PCF with cascade blending; first directional light only
- **SSAO** (`ssao.rs`): half-res 16-sample hemisphere kernel over depth + normals, interleaved-gradient rotation, depth-aware 4×4 blur; multiplies ambient and SH irradiance
- **Environment / IBL** (`environment.rs`): equirect `.hdr` or the procedural sky → 128² cube → GGX-prefiltered specular mips (6) + split-sum BRDF LUT; **SH9 irradiance** projected on the CPU. Default: sky gradient at `DEFAULT_SKY_ENV_INTENSITY`; scene `environment` or `Kerabit::environment` swaps in an `.hdr`
- **Sky**: fullscreen gradient from `clear_color` (horizon) to auto zenith, drawn only where the prepass left depth = 1
- **Lit shading**: GGX + metallic workflow; optional normal maps via derivative TBN; writes HDR color (target 0) and the specular *weight* Fresnel × BRDF × AO (target 1)
- **Specular resolve / SSR** (`ssr.rs`): Hi-Z assisted view-space ray march (coarse level 2, binary refine at level 0), thickness test, edge + roughness fade; misses fall back to the prefiltered environment cube
- **TAA** (`taa.rs`): Halton(2,3) 8-sample projection jitter, motion vectors from the prepass (camera reprojection for background), 3×3 neighborhood clamp, 0.9 history blend into a stable output texture copied to history
- **Post**: resolved HDR (`Rgba16Float`) → bright extract → half-res blur → ACES tonemap + bloom composite
- **Particles**: CPU billboards, camera-facing quads, alpha blend into the resolved HDR before TAA
- **UI overlay**: after post on swapchain; 8×8 ASCII atlas
- **Authoring lights**: Scene JSON = single sun (+ optional `environment`); runtime `lights` API ≤256 (≤4 directional)
- **Debug**: `KERABIT_RENDER_DEBUG=lit|resolved,no-ssao,no-ibl,no-ssr,no-taa` selects the stage the post stack shows / disables features; `RenderSettings` exposes the same toggles in code

### Vertex layout (**frozen**)

```text
location 0: position  f32x3
location 1: normal    f32x3
location 2: uv        f32x2
```

### Instance layout (P4 / M1)

```text
location 3–6: model matrix columns  f32x4 × 4
location 7:   albedo                f32x4
location 8:   params (roughness.x, metallic.y)  f32x4
```

CPU type: `kerabit_render::Vertex` / `InstanceRaw` (`bytemuck::Pod`). Changing the mesh vertex layout is a cross-crate breaking change — coordinate here first.

### P2+ render types (in `kerabit-render`, not the public facade)

| Type | Role |
|------|------|
| `Mesh` / `MeshBuilder` | CPU geometry builders |
| `MeshId` / `MeshCache` | GPU upload + content-hash lookup |
| `Camera` | `perspective(fov)` + `look_at` + `set_aspect` |
| `Light` / `LightKind` / `MAX_LIGHTS` / `MAX_DIRECTIONAL_LIGHTS` | sun / point; ≤256 in a storage buffer, ≤4 directional |
| `SceneRenderer` / `RenderSettings` | the whole 3.0 pass chain + SSAO / IBL / SSR / TAA toggles |
| `ShadowMap` / `CascadeSet` / `fit_cascades` | 4-cascade depth array + comparison sampler |
| `EquirectImage` | HDR pixels handed to the IBL builder |
| `PostStack` | bloom + tonemap over an external HDR source |
| `ParticleSystem` / `ParticleBurst` | billboard particles (M1) |
| `DrawItem` / `LodLevel` | mesh + model (+ previous model) + albedo + roughness + metallic + textures + LOD chain |
| `InstanceRaw` | GPU instance stride (160 B) |
| `TextureId` / `TextureCache` | albedo (sRGB) + normal (linear) + material bind groups |

Harness: `cargo run -p kerabit-render --example two_meshes` (plane + cube).
**P5:** `cargo run -p kerabit --example load_mesh`.

## Phase status

| Phase | Status | Notes |
|-------|--------|-------|
| P0 Workspace & foundations | **Done** | Math/color live; stub crates compile |
| P1 Window + first pixels | **Done** | winit + wgpu; `hardcoded_cube` example (lit cube, resize-safe) |
| P2 Scene core | **Done** | World + mesh/camera/light/GPU cache; `two_meshes` harness |
| P3 Public API + playground | **Done** | `Kerabit::new` / `run`, input, `examples/playground.rs` |
| P4 Materials / hierarchy / instancing | **Done** | Roughness specular, parent/child, instance batches, `many_cubes` |
| P5 Assets | **Done** | OBJ / PNG / glTF lite; `load_mesh` example; fixtures &lt; 4KB |
| P6 Physics + audio | **Done** | AABB / ray / sphere cast / `move_and_collide`; rodio WAV play; `physics_audio` |
| P7 Scene format + mini game | **Done** | `.kerabit.json` save/load; `mini_game` vertical slice |
| E0 Runtime Scene reload | **Done** | `Context::clear_world` / `apply_scene` / synced despawn; Reach in-process level advance |
| E1 Editor shell | **Done** | `tools/kerabit-editor` egui File/hierarchy/inspector |
| E2 Viewport / gizmos | **Done** | Offscreen lit viewport, ray AABB pick, T/R/S gizmos + snap |
| E3 Entity tags | **Done** | Additive `tags` on scene entities; Reach roles via tags (+ name-prefix fallback) |
| E4 More Reach content | **Done** | 5 tagged levels under `games/reach/levels/`; hard-but-fair dodge gaps |
| M5 Reach campaign | **Done** | 16 levels / 4 chapters, best times, particles + spatial juice |
| E5 Engine depth | **Done** | Directional soft shadows (PCF) + sky gradient; single-sun authoring model |
| E6 Ship Reach | **Done** | `scripts/package-reach.sh` → `dist/Reach.app` + zip; icon + Play docs |
| E7 Second game | **Done** | `games/surge` score-attack vertical slice; 2 editor-openable arenas |
| M0 Summit foundations | **Done** | `1.0.0-alpha.2`; [ROADMAP.md](ROADMAP.md); 3-OS CI; scene `components`/`extras` |
| M1 Render leap | **Done** (superseded by 3.0 tier) | PBR-lite, ≤4 lights, tonemap/bloom, particles, `pbr_room` example |
| M2 Simulation leap | **Done** | `kerabit-anim`; dynamics + character controller; enable/tags/layers; `physics_sandbox` |
| M3 Audio leap | **Done** | Spatial `play_at`, mix buses, streaming music |
| M4 Editor professional | **Done** | Undo/redo, multi-select, align, prefabs, snap persistence, polished Play |
| M7 Product | **Done** | Docs site, Reach macOS+Windows zips, tag-triggered packaging |
| M8 Hardening | **Done** | Clippy CI, frustum cull, 16k instances, ~10k cubes |
| M9 Kerabit 1.0 | **Done** | Workspace `1.0.0`; GitHub Release; site launch |
| 1.1 Rhai | **Done** (superseded) | `kerabit-script`; scene `extras.script`; editor code panel |
| 2.0 Scripting Summit | **Done** (superseded) | Rich host API; Spark; hot-reload; Frozen for 2.0 table |
| 3.0 Juni scripting | **Done** | `kerabit-juni`: Juni → WASM in-process, wasmtime host, prelude, `check_juni`; Rhai removed |
| 3.0 Render tier | **Done** | `SceneRenderer`: G-buffer prepass, clustered lights (256), 4-cascade CSM, SSAO, HDR IBL, SSR, TAA, LODs; 10k cubes + 200 lights at 60 fps |

## Deps

Workspace-shared dependencies are declared in the root `Cargo.toml`.

| Dep | Why |
|-----|-----|
| `glam` | f32 math; re-exported via `kerabit-math` / `kerabit::math` |
| `wgpu` | GPU API (Metal on macOS); pinned `24.0` for stable surface/pipeline API |
| `winit` | Window + event loop; pinned `0.30` (`ApplicationHandler`) — note API churn across minors |
| `bytemuck` | `Pod` vertex/uniform casts for GPU uploads |
| `pollster` | Sync `block_on` for adapter/device init (no tokio in V1) |
| `anyhow` | Internal fallible init / run errors |
| `thiserror` | Typed `AssetError` / `AudioError` (lightweight, no runtime bloat) |
| `raw-window-handle` | Workspace pin for surface interop (pulled by wgpu/winit) |
| `tobj` | Lean OBJ mesh load (positions/normals/UVs) → `Mesh` |
| `gltf` | Minimal glTF import (first mesh + base color factor/texture; no animation) |
| `image` | PNG (feature-gated) decode → RGBA8 albedo textures |
| `rodio` | P6/M3 audio via cpal; **WAV-only** (`default-features = false`, `features = ["wav"]`) — spatial `SpatialSink`, mix buses, streaming music without mp3/flac/vorbis decode bloat |
| `serde` / `serde_json` | P7 `.kerabit.json` scene save/load mirroring the public spawn API (entities, transforms, mesh primitives/paths, camera, lights) |
| `juni-driver` / `juni-check` | 3.0 Juni compiler, pulled by git tag from [Juno](https://github.com/AJpro774/Juno) (`v13.0.0`); compiles scripts to WASM in-process. Host API lives in `kerabit-juni`. |
| `wasmtime` | Cranelift JIT for compiled scripts (`default-features = false`, `cranelift` + `runtime` + `std`); fuel metering bounds each call. Swap for `wasmi` if binary size ever matters more than speed. |

**Not OK:** Bevy/Unity/Godot as deps; bundling another engine; multi-GB assets; ML runtimes; Electron.

## Size budget

Entire project + deps + local build artifacts must stay **under 20GB**. Target reality: **&lt; 1GB** for toolchain + debug build. Do not commit `target/` or large binaries.
