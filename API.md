# Public API contract

This document is the **stable target** for game-facing types. Changes require updating this file in the same change.

> **Alpha `1.0.0-alpha.2`:** frozen vs experimental surfaces are listed below. wgpu / winit types are not part of the public surface.

## Alpha freeze

Breaking a **Frozen for alpha** item requires a new alpha minor bump (`1.0.0-alpha.N` → next) and a [CHANGELOG.md](CHANGELOG.md) entry in the same change.

| Surface | Status | Notes |
|---------|--------|-------|
| `Kerabit` | **Frozen for alpha** | `new` / `clear_color` / `spawn` / `camera` / `light` / `lights` / `ambient` / `scene` / `load_scene` / `run` |
| `Entity` (spawn builder) | **Frozen for alpha** | `new` / `mesh` / `material` / `at` / `rotation` / `scale` / `parent` (+ M2 additive `tag` / `tags` / `layer` / `enabled`) |
| `Mesh` | **Frozen for alpha** | `cube` / `plane` / `load_obj` |
| `Material` | **Frozen for alpha** | `color` / `roughness` / `metallic` / texture + normal-map helpers (M1 additive) |
| `Scene`, `SceneError`, `SCENE_VERSION`, `SceneMap` | **Frozen for alpha** | `.kerabit.json` load/save; additive `components`/`extras` / `metallic`; `into_kerabit` |
| `Context` | **Frozen for alpha** | `dt` / `input` / `world` / `camera` / `physics` / `audio` / `ui` / `quit` / `apply_scene` / `load_scene` / spawn helpers; M1 `lights` / `set_lights` / `spawn_particles`; M3 `sync_audio_listener` |
| `Ui` | **Frozen for alpha** | `text` / `rect` (normalized top-left coords) |
| Physics (`PhysicsWorld`, `Aabb`, casts, `move_and_collide`) | **Frozen for alpha** + **M2 additive** | Static AABBs; dynamics + `CharacterController` additive |
| `kerabit-anim` (`AnimationClip`, `AnimationPlayer`) | **Additive (M2)** | Clip playback on hierarchy; glTF anim import stretch/minimal |
| Audio (`AudioEngine`, `SoundId`) | **Frozen for alpha** + **M3 additive** | WAV play / volume / null fallback; spatial `play_at`, `MixBus`, streaming `play_music`, `AudioListener` |
| Math / color (`Vec3`, `Quat`, `Color`, …) | **Frozen for alpha** | Via prelude |
| `Camera`, `Light`, `LightKind`, `ParticleBurst`, `Key`, `InputState` | **Frozen for alpha** | View/light/particles + input; multi-light ≤4 (M1) |
| `kerabit-editor` crate / UI | **Experimental** | Dev tool; Play/viewport may change without alpha bump. M4: undo/redo, multi-select, prefabs, snap persistence, polished child-process Play |
| Surge motion tags (`orbit`, `slide_x`, `slide_z`) | **Experimental** | Game convention used by Surge; not a general engine contract |
| Anything marked unstable / internal | **Experimental** | Do not depend on from published games without pinning |

## Intended usage (P3+)

```rust
use kerabit::prelude::*;

fn main() {
    Kerabit::new("Playground")
        .clear_color(Color::rgb(0.08, 0.09, 0.12))
        .spawn(
            Entity::new("cube")
                .mesh(Mesh::cube())
                .material(Material::color(Color::ORANGE).roughness(0.35))
                .at(Vec3::new(0.0, 0.5, 0.0)),
        )
        .spawn(
            Entity::new("satellite")
                .mesh(Mesh::cube())
                .material(Material::color(Color::rgb(0.4, 0.7, 1.0)).roughness(0.2))
                .at(Vec3::new(1.25, 0.0, 0.0))
                .parent("cube"),
        )
        .spawn(
            Entity::new("ground")
                .mesh(Mesh::plane(40.0))
                .material(Material::color(Color::GRAY).roughness(0.9))
                .at(Vec3::ZERO),
        )
        .camera(Camera::perspective(60.0).look_at(vec3(5.0, 3.0, 7.0), Vec3::ZERO))
        .light(Light::sun(vec3(-0.35, -1.0, -0.25)).intensity(1.2))
        .ambient(Color::rgb(0.15, 0.16, 0.18))
        .run(|ctx| {
            let dt = ctx.dt();
            if ctx.input().key_pressed(Key::Escape) {
                ctx.quit();
            }
            if let Some(cube) = ctx.world_mut().get_mut("cube") {
                cube.rotate_y(1.1 * dt);
            }
            // Optional: ctx.camera_mut() for orbit / WASD
        });
}
```

## Types the user may see

| Type | Status | Notes |
|------|--------|-------|
| `Color` | **P0** | `rgb` / `rgba`, `ORANGE`, `GRAY`, `WHITE`, `BLACK`, `lerp` |
| `Vec3`, `Quat`, `Mat4`, `vec3`, `Deg`, `Rad` | **P0** | via `kerabit` / `kerabit::math` / prelude |
| `Kerabit` | **P3/P7/M1** | `new` / `clear_color` / `spawn` / `camera` / `light` / `lights` / `ambient` / `scene` / `load_scene` / `run` |
| `Entity` | **P3/P4/P7/M2** | Spawn builder: `new` / `mesh` / `material` / `at` / `rotation` / `scale` / `parent` / `tag` / `tags` / `layer` / `enabled` |
| `Mesh` | **P5** | `cube()` / `plane(size)` / `load_obj(path)` |
| `Material` | **P5/M1** | `color(Color)` + `.roughness` / `.metallic` + `.with_texture` / `.with_normal_map` / `load_png` |
| `load_gltf` / `Texture` / `AssetError` | **P5** | First mesh + base color factor/texture |
| `Camera`, `Light`, `LightKind`, `MAX_LIGHTS` | **P3/M1** | `perspective` + `look_at`; `Light::sun` / `point` + intensity/range; up to **4** lights |
| `ParticleBurst` | **M1** | Billboard burst via `ctx.spawn_particles` |
| `Key`, `MouseButton`, `InputState` | **P3** | `key_down` / `key_pressed`; mouse pos / delta / buttons |
| `Context` | **P3/P6/UI/E0/M1/M3** | `dt`, `input`, `world` / `world_mut`, `camera` / `camera_mut`, `physics`, `audio`, `ui`, `quit`; runtime `clear_world` / `despawn` / `spawn` / `apply_scene` / `load_scene`; `lights` / `set_lights` / `spawn_particles`; `sync_audio_listener` |
| `Ui` | **UI** | Immediate-mode overlay: `text` / `rect` via `ctx.ui()` |
| `World`, `Transform`, `EntityId`, `LAYER_DEFAULT` | **P4/E0/M2** | Hierarchy; enable/disable; tags / layer queries |
| `PhysicsWorld`, `Aabb`, `ColliderId`, `RayHit`, `SphereCastHit`, `MoveResult` | **P6/E0** | Static AABBs; ray/sphere cast; kinematic block; `clear` |
| `DynamicBody`, `BodyId`, `BodyShape`, `CharacterController`, `CharacterMove` | **M2** | Gravity dynamics vs statics; character controller |
| `AnimationClip`, `AnimationPlayer`, `AnimChannel`, `translation_clip` | **M2** | Clip playback on named transform hierarchy |
| `AudioEngine`, `SoundId`, `AudioError`, `MixBus`, `AudioListener` | **P6/M3** | `play` / `play_with`; `play_at` / `play_at_with`; `play_music` / `play_music_with`; buses; listener; null fallback |
| `Scene`, `SceneError`, `SCENE_VERSION`, `SceneMap` | **P7** / M0 | `.kerabit.json` load/save; additive `components`/`extras`; `into_kerabit` / `Kerabit::scene` |

Live spawned objects are `kerabit::world::Entity` (transform + name + parent/children). The top-level [`Entity`](crate) type is only the spawn descriptor.

### Material

- `Material::color(c)` — albedo with mid roughness `0.5`, dielectric (`metallic = 0`)
- `.roughness(r)` — clamp to `0.0..=1.0`; lower = sharper specular
- `.metallic(m)` — clamp to `0.0..=1.0`; PBR-lite metalness (M1)
- `.with_texture(Texture)` / `Material::load_png(path)` — albedo map (multiplied by tint)
- `.with_normal_map(Texture)` — optional tangent-space normal map (derivative TBN; vertex layout unchanged)
- Accessors: `albedo()`, `roughness_factor()`, `metallic_factor()`, `albedo_texture()`, `normal_texture()`

### Lights (M1)

- **Limit:** at most **4** lights (`MAX_LIGHTS`), any mix of directional + point
- `Light::sun(dir)` / `Light::directional(dir)` — parallel rays; soft **shadows from the first directional only**
- `Light::point(pos).range(r)` — local omni with distance falloff (unshadowed)
- Builder: `Kerabit::light(…)` sets slot 0; `Kerabit::lights([...])` replaces the list
- Runtime: `ctx.lights()` / `ctx.set_lights([...])` / `ctx.light_mut()` (slot 0)
- Scene JSON still authors a **single sun** (`light` field); multi-light is a code/runtime API

### Post + particles (M1)

- Every frame: HDR lit → cheap bloom extract/blur → ACES tonemap to the swapchain (always on)
- `ctx.spawn_particles(ParticleBurst { origin, count, color, size, speed, lifetime, velocity, spread })` — CPU billboards, soft circle alpha, max 1024 live
- Example: `cargo run -p kerabit --example pbr_room`

### Assets (P5)

- `Mesh::load_obj(path)` — first OBJ mesh (pos/normals/UVs)
- `load_gltf(path) -> (Mesh, Material)` — first mesh + base color factor/texture (no animation)
- Tiny fixtures: `crates/kerabit-assets/fixtures/` (`box.obj`, `box.gltf`, `checker.png`)
- Example: `cargo run -p kerabit --example load_mesh`

### Physics (P6 / M2)

- `ctx.physics()` → [`PhysicsWorld`](crate::PhysicsWorld): register static AABBs, query overlaps
- `Aabb::from_center_half_extents(center, half)` / `overlaps`
- `raycast(origin, dir, max_t)` / `sphere_cast(origin, radius, dir, max_dist)`
- `move_and_collide(pos, velocity, half_extents, dt)` — kinematic slide + block
- **M2 dynamics:** `add_dynamic(DynamicBody::aabb|sphere)` / `step(dt)` — gravity + resolve vs statics (no body–body yet)
- **M2 character:** `CharacterController::new` / `::planar` → `move_wish` (gravity + jump) or `move_planar` (Reach-style)
- Example: `cargo run -p kerabit --example physics_sandbox`

### Animation (M2)

- `AnimationClip` + `AnimChannel` (translation / rotation / scale keys by entity name)
- `AnimationPlayer::new(clip).play()` → `update(world, dt)` samples into the hierarchy
- Helper: `translation_clip(name, target, from, to, duration)` for demos
- glTF animation import is stretch / minimal — author clips in Rust for now

### World queries (M2)

- Live entities: `set_enabled` / `is_enabled`; `add_tag` / `has_tag` / `set_layer`
- Spawn builder: `.tag("…")` / `.tags([...])` / `.layer(mask)` / `.enabled(bool)`
- World: `entities_with_tag`, `entities_on_layer`, `enabled_with_tag`, `iter_enabled`
- Disabled entities are skipped by the draw list
- Scene JSON `tags` are applied to live entities on spawn / `apply_scene`

### Audio (P6 / M3)

- `ctx.audio()` → [`AudioEngine`](crate::AudioEngine)
- Non-spatial SFX: `play(path)` / `play_with(path, volume, loop)` → `SoundId` (sfx bus)
- Spatial SFX: `play_at(path, position)` / `play_at_with(path, position, volume, loop)` — stereo distance attenuation via listener ears
- Listener: `follow_look_at(eye, target, up)` / `set_listener` / `AudioListener`; frame helper `ctx.sync_audio_listener()` (camera → listener)
- Mix buses: `MixBus::{Sfx, Music}` with `set_bus_volume` / `bus_volume`; overall `set_master_volume` (gain = voice × bus × master)
- Streaming music: `play_music` / `play_music_with(path, volume, loop)` — WAV decode without full-file buffer; loop re-opens the file each cycle (`maintain` runs each frame)
- `set_volume` / `stop` / `stop_bus` / `stop_all` / `set_emitter_position`; silent `AudioEngine::null` if no device
- Lean format: **WAV only** (rodio `wav` feature; no mp3/flac/vorbis)
- WAV fixture: `examples/assets/beep.wav` (~7KB)
- Example: `cargo run -p kerabit --example physics_audio` (WASD collide + Space beep)

### Scenes (P7)

- `Scene::load(path)` / `Scene::save(path)` / `from_json` / `to_json` — round-trip `.kerabit.json`
- Format mirrors spawn: entities (mesh primitive or `obj`/`gltf` path, material, `at` / `rotation` / `scale` / `parent` / optional `tags`), camera, light, clear/ambient
- **Lighting / sky (E5 + M1):** Scene authors one directional **sun** (`light.direction` / `intensity` / `color`) plus `ambient` and `clear_color`. Runtime code may set up to **4** lights via `Kerabit::lights` / `ctx.set_lights` (dir + point); soft shadows still follow the first directional only. The renderer paints a **sky gradient** using `clear_color` as the horizon (zenith is derived).
- **Materials (M1):** optional additive `"metallic"` on scene materials (default `0`); `SCENE_VERSION` stays **1**.
- **Entity tags (E3):** each entity may include `"tags": ["player", "wall", …]` (string list). Omitted or `[]` means no tags. `SCENE_VERSION` stays **1** (additive field). Shared roles: `player`, `goal`, `ground`, `wall`, `hazard` — prefer tags; legacy name exact match / `wall_*` / `hazard_*` prefixes still work for one version. `SceneEntity::has_tag`
- **Reserved bags (M0):** optional `"components"` and `"extras"` JSON objects on the **scene root** and each **entity**. Omitted or `{}` today; preserved on load/save; ignored at spawn. `SCENE_VERSION` stays **1**. Types: `SceneMap` / `Scene::{components,extras}` / `SceneEntity::{components,extras}`.
- **Surge motion tags (E7):** on `hazard` entities, optional `orbit` / `slide_x` / `slide_z` select patrol style for the score-attack arenas (`games/surge`)
- `Kerabit::load_scene(path)` / `Kerabit::scene(Scene)` / `Scene::into_kerabit(title)`
- **Prefabs (M4):** `Prefab::load` / `save` / `from_json` / `to_json` / `instantiate(scene, offset)` — `.kerabit.prefab.json` (version + entities only; same entity wire format as scenes). Editor: File → Save Prefab / Instance Prefab. Samples under `games/reach/prefabs/`.
- Checked-in levels: `games/reach/levels/*.kerabit.json` (flagship, 5 levels); `games/surge/levels/*.kerabit.json` (score-attack, 2 arenas); author in `kerabit-editor`; `examples/scenes/mini_game.kerabit.json` (legacy)
- Play: `cargo run -p reach` · `cargo run -p surge` · legacy: `cargo run -p kerabit --example mini_game` · editor: `cargo run -p kerabit-editor`

### Runtime Scene reload (E0)

Mid-run APIs on [`Context`](crate::Context) — clear / apply a scene **without** ending the demand-run (same window + GPU + EventLoop). Soft window recreate is **not** required for level transitions.

```rust
// Inside Kerabit::run(|ctx| { ... })
ctx.apply_scene(&Scene::load("levels/02.kerabit.json")?)?;
// Re-register any physics statics — apply_scene clears PhysicsWorld.
for (center, half) in &walls {
    ctx.physics().add_aabb(Aabb::from_center_half_extents(*center, *half));
}
```

| Method | Notes |
|--------|-------|
| `ctx.clear_world()` | Drop all entities + GPU draw entries + physics colliders; camera/light/ambient/clear unchanged |
| `ctx.despawn(name)` / `ctx.despawn_id(id)` | World remove **and** renderable sync (prefer over raw `world_mut().despawn`) |
| `ctx.spawn(Entity)` | Mid-run spawn with mesh upload + draw entry |
| `ctx.apply_scene(&Scene)` | `clear_world` + camera/light/ambient/clear + spawn scene entities |
| `ctx.load_scene(path)` | `Scene::load` then `apply_scene` |

Prefer `apply_scene` for level transitions. Calling `Kerabit::run` again still works (EventLoop is reused) but tears down the window — use that only for full app restart.

### UI overlay

Immediate-mode screen overlay drawn **after** the 3D pass. Cleared at the start of every frame.

```rust
ctx.ui().rect(0.0, 0.0, 1.0, 1.0, Color::rgba(0.0, 0.0, 0.0, 0.55));
ctx.ui().text(0.35, 0.42, 0.06, Color::WHITE, "REACH");
ctx.ui().text(0.28, 0.52, 0.03, Color::GRAY, "Press Space");
```

| Method | Notes |
|--------|-------|
| `ctx.ui().rect(x, y, w, h, color)` | Solid quad |
| `ctx.ui().text(x, y, size, color, &str)` | Embedded 8×8 ASCII bitmap font; `\n` advances a line |

**Coordinate system (locked):** normalized `0..=1`, origin **top-left**.
- `(0, 0)` = top-left of the window; `(1, 1)` = bottom-right
- `w` / `h` / text `size` are fractions of the framebuffer (width and height independently)
- Glyph cells are square: width equals `size`

No wgpu / winit types. No FreeType — glyphs come from an in-repo 8×8 atlas.

### Hierarchy

- Spawn: `.parent("parent_name")` — local `at(...)` is relative to the parent
- Runtime: `world.attach("child", "parent")`, `world.detach("child")`, `world.set_parent(id, Some(parent))`
- Helpers: `transform_mut("name")`, `parent_of` / `children_of`
- Each frame the engine calls `update_world_matrices()` before drawing

## Types the user must never see

- `wgpu::Device`, `Queue`, `RenderPipeline`, `Buffer`, `BindGroup`
- `winit::Window`, raw winit event enums

## Examples

| Example | Command |
|---------|---------|
| **Reach** (flagship) | `cargo run -p reach` · ship: `./scripts/package-reach.sh` → `dist/Reach.app` |
| Playground | `cargo run -p kerabit --example playground` |
| PBR room (M1) | `cargo run -p kerabit --example pbr_room` |
| Many cubes (instancing) | `cargo run -p kerabit --example many_cubes --release` |
| Load mesh (OBJ/glTF) | `cargo run -p kerabit --example load_mesh` |
| Physics + audio | `cargo run -p kerabit --example physics_audio` |
| Physics sandbox (M2) | `cargo run -p kerabit --example physics_sandbox` |
| Mini game (legacy) | `cargo run -p kerabit --example mini_game` |

## Stability notes

- **Alpha freeze:** see the table at the top of this file. Breaking frozen items needs a new alpha bump + CHANGELOG.
- Do not expand the public API without updating this file.
- Examples must compile against the public API only.
- wgpu leakage in public rustdoc is an accept-gate failure.
- Prefer extending `Material` in a backward-compatible way (albedo + optional fields) so playground keeps building.
