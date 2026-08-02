# Architecture

Kerabit is a multi-crate Cargo workspace. Game authors depend only on **`kerabit`**. Internals may use wgpu/winit; those types must never leak through the public facade.

**Summit moonshot:** phases M0–M9 (PBR, dynamics, editor polish, campaign, 1.0) live in [ROADMAP.md](ROADMAP.md). Install remains rustup + cargo.

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
| `kerabit-editor` (`tools/`) | Dev-only egui level editor | E1–E2 |

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
  └── kerabit-audio

tools/kerabit-editor → kerabit + kerabit-render (+ egui; not shipped with games)
```

**Editor boundary:** egui lives only in `tools/kerabit-editor`. The 3D viewport renders the live `Scene` through [`OffscreenLitRenderer`](crates/kerabit-render/src/offscreen.rs) (same lit path as games) into an egui paint callback. Picking helpers (`ray_from_ndc`, mesh AABB, `pick_closest`) live in `kerabit-render` so the game API stays free of UI crates.
Shaders live in `crates/kerabit-render/shaders/` as `.wgsl` files included via `include_str!`.

## Frame loop (P4+)

1. Pump window events → update [`kerabit_input::InputState`]
2. Clear UI draw list; call game `run` closure with [`Context`](API.md) (`dt`, input, world, camera, physics, audio, `ui`, quit; E0 also wires GPU + renderables for `apply_scene` / `despawn` / `spawn`)
3. Clear input edges / mouse delta (`end_frame`)
4. [`World::update_world_matrices`] — dirty local TRS, then parent→child world matrices
5. Build draw list from **enabled** world entities + per-entity mesh / albedo / roughness (world matrix); despawned entities must leave the renderable map (`Context::despawn` / `clear_world`)
6. Pack instances by `MeshId`, write instance buffer, then encode **shadow map** (directional depth) → **sky** + **lit** into HDR (PBR-lite, ≤4 lights, PCF soft shadows) → **particles** → **tonemap + bloom** to swapchain → **overlay** (UI), present

**EventLoop / reload:** One winit `EventLoop` per process (thread-local + `run_app_on_demand`). Mid-run [`Context::apply_scene`](API.md) clears world + renderables + physics and respawns a `Scene` without recreating the window — preferred for level transitions (Reach) and future editor Play. Re-entering `Kerabit::run` still works but rebuilds App/window.

**Overlay pass:** game code queues `ctx.ui().rect` / `ctx.ui().text` in normalized top-left `0..=1` space. The engine expands text into atlas-sampled quads and draws them after 3D. See [`API.md`](API.md) § UI overlay.

**P2 render harnesses** remain: `cargo run -p kerabit-render --example two_meshes`.  
**P3 flagship:** `cargo run -p kerabit --example playground`.  
**M1 PBR room:** `cargo run -p kerabit --example pbr_room`.  
**P4 stress:** `cargo run -p kerabit --example many_cubes --release`.  
**P5 assets:** `cargo run -p kerabit --example load_mesh`.  
**P6 physics + audio:** `cargo run -p kerabit --example physics_audio`.  
**Flagship game:** `cargo run -p reach` (`games/reach`, 12 `.kerabit.json` levels + HUD overlay; in-process `apply_scene` between levels).
**Second game (E7/M6):** `cargo run -p surge` (`games/surge`, timed ranked + endless; 5 arenas; public API + shared tags + `orbit`/`slide_*` motion tags).
**Showcase (M6):** `cargo run -p showcase` — non-game Summit render trailer (PBR, lights, particles).
**P7 legacy slice:** `cargo run -p kerabit --example mini_game` (loads `examples/scenes/mini_game.kerabit.json`).

## GPU resource model (P1–P4, E5, M1)

- **Frame uniforms**: view-proj, camera, ambient, light view-proj, shadow params + **up to 4 lights** (`FrameUniforms` / `lit.wgsl`)
- **Instance buffer**: model + albedo + roughness + metallic (`InstanceRaw`); batches by mesh + albedo + normal tex (≤2048 instances/frame)
- **Mesh GPU cache**: CPU `Mesh` → content-hash dedupe → `MeshId` → vertex/index buffers
- **Material**: albedo / roughness / metallic in instance attrs; albedo + normal bind group (white / flat-normal defaults)
- **Depth texture** resized on rescale
- **Shadow map (E5)**: 2048² directional cascade; first directional light only; 3×3 PCF in lit
- **Sky (E5)**: fullscreen gradient from `clear_color` (horizon) to auto zenith
- **Lit shading (M1)**: PBR-lite GGX + metallic workflow; optional normal maps via derivative TBN
- **Post (M1)**: HDR (`Rgba16Float`) → bright extract → half-res blur → ACES tonemap + bloom composite
- **Particles (M1)**: CPU billboards, camera-facing quads, alpha blend into HDR before post
- **UI overlay**: after post on swapchain; 8×8 ASCII atlas
- **Authoring lights**: Scene JSON = single sun; runtime `lights` API ≤4 (dir + point)

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
| `Light` / `LightKind` / `MAX_LIGHTS` | sun / point; ≤4 packed into frame uniforms |
| `ShadowMap` | directional depth map + comparison sampler (E5) |
| `PostStack` | HDR + bloom + tonemap (M1) |
| `ParticleSystem` / `ParticleBurst` | billboard particles (M1) |
| `DrawItem` | mesh + model + albedo + roughness + metallic + textures |
| `InstanceRaw` | GPU instance stride |
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
| M5 Reach campaign | **Done** | 12 levels / 3 chapters, best times, particles + spatial juice |
| E5 Engine depth | **Done** | Directional soft shadows (PCF) + sky gradient; single-sun authoring model |
| E6 Ship Reach | **Done** | `scripts/package-reach.sh` → `dist/Reach.app` + zip; icon + Play docs |
| E7 Second game | **Done** | `games/surge` score-attack vertical slice; 2 editor-openable arenas |
| M0 Summit foundations | **Done** | `1.0.0-alpha.2`; [ROADMAP.md](ROADMAP.md); 3-OS CI; scene `components`/`extras` |
| M1 Render leap | **Done** | PBR-lite, ≤4 lights, tonemap/bloom, particles, `pbr_room` example |
| M2 Simulation leap | **Done** | `kerabit-anim`; dynamics + character controller; enable/tags/layers; `physics_sandbox` |
| M3 Audio leap | **Done** | Spatial `play_at`, mix buses, streaming music |
| M4 Editor professional | **Done** | Undo/redo, multi-select, align, prefabs, snap persistence, polished Play |

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

**Not OK:** Bevy/Unity/Godot as deps; bundling another engine; multi-GB assets; ML runtimes; Electron.

## Size budget

Entire project + deps + local build artifacts must stay **under 20GB**. Target reality: **&lt; 1GB** for toolchain + debug build. Do not commit `target/` or large binaries.
