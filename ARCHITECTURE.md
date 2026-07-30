# Architecture

Kerabit is a multi-crate Cargo workspace. Game authors depend only on **`kerabit`**. Internals may use wgpu/winit; those types must never leak through the public facade.

## Crate map

| Crate | Role | Phase |
|-------|------|-------|
| `kerabit` | Public facade / builder API | P3 |
| `kerabit-math` | `glam` re-exports, `vec3`, `Deg`/`Rad`, look-at | P0 |
| `kerabit-color` | `Color`, named constants | P0 |
| `kerabit-world` | Entities, transforms, hierarchy | P2–P4 |
| `kerabit-input` | Input snapshot, `Key`, mouse | P3 |
| `kerabit-render` | Device, pipelines, GPU meshes, shaders | P1+ |
| `kerabit-assets` | OBJ / PNG / glTF loaders | P5 |
| `kerabit-physics` | AABB / raycast / simple dynamics | P6 |
| `kerabit-audio` | Playback wrapper | P6 |
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
5. Build draw list from world + per-entity mesh / albedo / roughness (world matrix); despawned entities must leave the renderable map (`Context::despawn` / `clear_world`)
6. Pack instances by `MeshId`, write instance buffer, then encode **shadow map** (directional depth) → **sky gradient** → color+depth **lit pass** (PCF soft shadows) → **overlay pass** (screen-space UI quads + bitmap font, alpha-blended, no depth), present

**EventLoop / reload:** One winit `EventLoop` per process (thread-local + `run_app_on_demand`). Mid-run [`Context::apply_scene`](API.md) clears world + renderables + physics and respawns a `Scene` without recreating the window — preferred for level transitions (Reach) and future editor Play. Re-entering `Kerabit::run` still works but rebuilds App/window.

**Overlay pass:** game code queues `ctx.ui().rect` / `ctx.ui().text` in normalized top-left `0..=1` space. The engine expands text into atlas-sampled quads and draws them after 3D. See [`API.md`](API.md) § UI overlay.

**P2 render harnesses** remain: `cargo run -p kerabit-render --example two_meshes`.  
**P3 flagship:** `cargo run -p kerabit --example playground`.  
**P4 stress:** `cargo run -p kerabit --example many_cubes --release`.  
**P5 assets:** `cargo run -p kerabit --example load_mesh`.  
**P6 physics + audio:** `cargo run -p kerabit --example physics_audio`.  
**Flagship game:** `cargo run -p reach` (`games/reach`, 5 `.kerabit.json` levels + HUD overlay; in-process `apply_scene` between levels).  
**Second game (E7):** `cargo run -p surge` (`games/surge`, score-attack arenas; public API + shared tags + `orbit`/`slide_*` motion tags).  
**P7 legacy slice:** `cargo run -p kerabit --example mini_game` (loads `examples/scenes/mini_game.kerabit.json`).

## GPU resource model (P1–P4, E5)

- **Frame uniforms**: view-proj, camera pos, light dir/color, ambient, light view-proj, shadow bias (`FrameUniforms` / `lit.wgsl`)
- **Instance buffer**: per-draw model matrix + albedo + roughness (`InstanceRaw`); identical meshes share one `draw_indexed` (up to 2048 instances/frame)
- **Mesh GPU cache**: CPU `Mesh` → content-hash dedupe → `MeshId` → vertex/index buffers (many `Mesh::cube()` uploads share one GPU mesh)
- **Material**: albedo + roughness in instance attrs; albedo texture bind group (1×1 white default)
- **Depth texture** resized on rescale
- **Shadow map (E5)**: 2048² `Depth32Float` directional cascade centered on camera target; depth-only pass (`shadow.wgsl`) then 3×3 PCF compare sample in lit (`group(2)`)
- **Sky (E5)**: fullscreen gradient from `clear_color` (horizon) to an auto-derived zenith — no extra Scene field
- **Lit shading**: Lambertian diffuse + roughness-lite Blinn-Phong specular, modulated by soft sun shadows
- **UI overlay**: separate alpha-blended pass after lit; embedded 8×8 ASCII atlas; no depth test
- **Authoring lights**: single directional sun + ambient only (no multi-light array in Scene / frame uniforms)

### Vertex layout (**frozen**)

```text
location 0: position  f32x3
location 1: normal    f32x3
location 2: uv        f32x2   // sampled for albedo textures (P5)
```

### Instance layout (P4)

```text
location 3–6: model matrix columns  f32x4 × 4
location 7:   albedo                f32x4
location 8:   params (roughness.x)  f32x4
```

CPU type: `kerabit_render::Vertex` / `InstanceRaw` (`bytemuck::Pod`). Changing the mesh vertex layout is a cross-crate breaking change — coordinate here first.

### P2+ render types (in `kerabit-render`, not the public facade)

| Type | Role |
|------|------|
| `Mesh` / `MeshBuilder` | CPU geometry builders |
| `MeshId` / `MeshCache` | GPU upload + content-hash lookup |
| `Camera` | `perspective(fov)` + `look_at` + `set_aspect` |
| `Light` | `sun(dir).intensity(…)` — single directional sun |
| `ShadowMap` | directional depth map + comparison sampler (E5) |
| `DrawItem` | mesh + model + albedo + roughness + optional texture for one instance |
| `InstanceRaw` | GPU instance stride |
| `TextureId` / `TextureCache` | GPU albedo upload + bind groups |

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
| E5 Engine depth | **Done** | Directional soft shadows (PCF) + sky gradient; single-sun authoring model |
| E6 Ship Reach | **Done** | `scripts/package-reach.sh` → `dist/Reach.app` + zip; icon + Play docs |
| E7 Second game | **Done** | `games/surge` score-attack vertical slice; 2 editor-openable arenas |

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
| `rodio` | P6 audio playback via cpal; **WAV-only** (`default-features = false`, `features = ["wav"]`) to avoid mp3/flac/vorbis decode bloat |
| `serde` / `serde_json` | P7 `.kerabit.json` scene save/load mirroring the public spawn API (entities, transforms, mesh primitives/paths, camera, lights) |

**Not OK:** Bevy/Unity/Godot as deps; bundling another engine; multi-GB assets; ML runtimes; Electron.

## Size budget

Entire project + deps + local build artifacts must stay **under 20GB**. Target reality: **&lt; 1GB** for toolchain + debug build. Do not commit `target/` or large binaries.
