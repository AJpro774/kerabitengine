# Kerabit

Lean native Rust 3D engine: **simple for the game author, deep in the engine**.

**Site:** [kerabitengine.vercel.app](https://kerabitengine.vercel.app) · **Repo:** [github.com/AJpro774/kerabitengine](https://github.com/AJpro774/kerabitengine)

> **Status:** **Kerabit 3.0** (`3.0.0`) — Juni scripting + UE5-class render tier. Flagship: **Reach**. Script-first proof: **Spark**. FPS slice: **Strike**. Also **Surge** + **Showcase**. Roadmap: [ROADMAP.md](ROADMAP.md).

## Install

Clone-and-cargo for authors. Install is unchanged:

1. Install [Rust stable](https://rustup.rs/) via `rustup` (toolchain pinned in `rust-toolchain.toml`)
2. Working GPU drivers (Metal on macOS; Vulkan/Metal/DX12 via wgpu elsewhere)
3. Clone and run:

```bash
git clone https://github.com/AJpro774/kerabitengine.git
cd kerabitengine
cargo run -p spark
cargo run -p strike
cargo run -p reach
cargo run -p surge
cargo run -p showcase
cargo run -p kerabit-editor
```

Frozen vs experimental public APIs: [API.md](API.md). Release notes: [CHANGELOG.md](CHANGELOG.md).

## Kerabit MCP

Agents can drive the repo via a local stdio MCP (docs search, validate scenes / check Juni scripts, scaffold, run/stop packages):

```bash
cd tools/kerabit-mcp && npm install && npm run build
```

Cursor wiring and tool list: [tools/kerabit-mcp/README.md](tools/kerabit-mcp/README.md).

## Goals

- Tiny game-facing API (builder + `run` closure; no wgpu in user code)
- Real wgpu renderer, scene graph, assets, physics/audio, `.kerabit.json` scenes
- Juni scripting (statically typed, compiled to WASM in-process) rich enough to ship small games mostly in `.juni` (3.0)
- UE5-class rendering on a tiny footprint: clustered lights, cascaded shadows, SSAO, IBL, SSR, TAA, LOD (3.0)
- Install/build footprint far under a 20GB budget (target: &lt; 1GB toolchain + debug build)

## Play Reach (release)

**Players:** unzip a published player zip — no Rust required.

| Platform | Artifact | How to run |
|----------|----------|------------|
| macOS | `Reach-macos.zip` | Double-click **Reach.app** |
| Windows | `Reach-windows.zip` (and Surge / Spark / Strike / Showcase / editor) | Run the `.exe` next to its `levels/` or `scenes/` folder |

Zips appear on [GitHub Releases](https://github.com/AJpro774/kerabitengine/releases) when cut, or as CI artifacts from the `package-reach` workflow (`workflow_dispatch`). Site download notes: [kerabitengine.vercel.app/#download](https://kerabitengine.vercel.app/#download).

**Build the zip yourself** (needs Rust; macOS Xcode CLT only if you pass `--rebuild-icon`):

```bash
# macOS
./scripts/package-reach.sh
# → dist/Reach.app and dist/Reach-macos.zip
open dist/Reach.app

# Windows (PowerShell) — Reach only, or everything:
pwsh ./scripts/package-reach-windows.ps1
pwsh ./scripts/package-windows.ps1
# → dist/*-windows.zip (Reach, Surge, Spark, Strike, Showcase, editor)
```

Moving the whole checkout to a PC: [WINDOWS.md](WINDOWS.md) (`%USERPROFILE%\.kerabit` is the same folder as Mac `~/.kerabit`).

**Dev run** (source tree):

```bash
cargo run -p reach
# release binary (Windows: target\release\reach.exe)
cargo build -p reach --release && ./target/release/reach
```

Controls: **Space** start / next · **WASD** move · **R** retry · **Escape** quit.

## Docs (site)

| Guide | URL |
|-------|-----|
| Getting Started (≤30 min stranger path) | [docs/getting-started](https://kerabitengine.vercel.app/docs/getting-started) |
| API tour | [docs/api-tour](https://kerabitengine.vercel.app/docs/api-tour) |
| Juni scripting (3.0) | [docs/scripting](https://kerabitengine.vercel.app/docs/scripting) |
| Editor guide | [docs/editor](https://kerabitengine.vercel.app/docs/editor) |
| Community mods | [docs/modding](https://kerabitengine.vercel.app/docs/modding) |
| Windows (Mac → PC) | [WINDOWS.md](WINDOWS.md) |

## Quick start (engine / authors)

```bash
# Requires a recent stable Rust toolchain (pinned in rust-toolchain.toml)
cargo run -p kerabit --example hello
cargo run -p kerabit --example hello_juni
cargo run -p spark
cargo run -p strike
cargo run -p reach
cargo run -p surge
cargo run -p showcase
cargo run -p kerabit-editor
cargo build -p kerabit --examples
cargo run -p kerabit --example playground
```

### Level editor

```bash
cargo run -p kerabit-editor
```

Community mods live under `mods/` (sample: `mods/hello-cube`) or `~/.kerabit/mods`. Each pack is a folder with `mod.kerabit.json`. Editor **Mods** window: enable, Open, Play. Share via git; catalog is `community/catalog.json`.

Open a Reach or Surge level under `games/*/levels/`. Central 3D viewport (orbit RMB, pan MMB, zoom scroll), click to select (**Shift+click** multi-select), **W/E/R** for move/rotate/scale gizmos, configurable snap (persisted in `~/.kerabit/editor.json`), **Place cube** then click the ground plane. **Ctrl+Z / Ctrl+Shift+Z** undo/redo; Edit → Align X/Y/Z; File → Save Prefab / Instance Prefab (`.kerabit.prefab.json`, samples in `games/reach/prefabs/`). File → Save writes `.kerabit.json`. **Script** menu opens a Juni panel (`extras.script` on the scene or an entity; **Check** type-checks against the host API). **Play** runs the scene (and its scripts) in a child window with no builtin HUD or fly camera — the scene camera and Juni UI are the game (dirty scenes use a temp snapshot; Esc returns with selection intact). Editor is a **dev tool** — not bundled inside the shipped Reach.app.

### Juni scripting (3.0)

```bash
cargo run -p kerabit --example hello_juni
cargo run -p spark
cargo run -p kerabit-juni --bin check_juni -- games/spark/scenes/spark.juni
```

Attach a `.juni` file with scene/entity `extras.script` (path relative to the `.kerabit.json`). [Juni](https://github.com/AJpro774/Juno) is statically typed with Python-like indentation; Kerabit compiles it to WASM in-process and runs it in an embedded, fuel-limited runtime — type errors show `file:line:col` before Play, and a runaway loop traps instead of hanging a frame. `fn main() -> i32` runs once at load, `fn frame(dt: f32) -> i32` every frame; keep values in a `state:` block; entities are `i32` handles from `entity("name")`. The **Frozen for 3.0** host API covers world read/write, spawn/despawn, `move_planar`, audio, particles, camera, UI overlay, mouse, and hot-reload — see [API.md](API.md), [docs/scripting](https://kerabitengine.vercel.app/docs/scripting), and the prelude [`crates/kerabit-juni/juni/kerabit.juni`](crates/kerabit-juni/juni/kerabit.juni). **Spark** is the script-first proof game.

### Reach (flagship)

```bash
cargo run -p reach
```

Campaign of 16 levels across 4 chapters: title → chapter select → WASD to the cyan pad → avoid red hazards → CLEAR / RETRY. Levels live in `games/reach/levels/` (edit with `kerabit-editor`). Release packaging: `./scripts/package-reach.sh` / `package-reach-windows.ps1` (see **Play Reach** above).

### Surge (score-attack)

```bash
cargo run -p surge
```

**Timed ranked** (survive 60s per arena, clear bonus + rank tier) or **Endless** (no time limit, waves keep ramping). Five arenas in `games/surge/levels/`. Title: **1/2** mode · **←/→** arena · **Space** start; **R** retries; **Esc** title / quit. Best scores in `~/.kerabit/surge_best.txt`.

### Showcase (engine trailer)

```bash
cargo run -p showcase
```

Non-game visual proof of Summit render: PBR-lite room, multi-light, bloom, particles, orbiting camera. Escape quits.

### Strike (FPS slice)

```bash
cargo run -p strike
```

Compact indoor arena: **WASD** move, **mouse** look (click the window), **left-click** hitscan, **Space** jump / start, **R** retry, **Esc** quit. Hostile dummies patrol, chase on line of sight, and return fire (2 hits each; you have 5 HP). Sketchfab CC-BY props live in `games/strike/assets/` (see `ATTRIBUTION.md`).

### Mini game (legacy slice)

```bash
cargo run -p kerabit --example mini_game
```

Earlier single-scene demo (`examples/scenes/mini_game.kerabit.json`). Prefer **Reach** for the full play loop + HUD.

Hello cube (under ~40 lines):

```rust
use kerabit::prelude::*;

fn main() {
    Kerabit::new("Hello")
        .clear_color(Color::rgb(0.08, 0.09, 0.12))
        .spawn(
            Entity::new("cube")
                .mesh(Mesh::cube())
                .material(Material::color(Color::ORANGE))
                .at(Vec3::new(0.0, 0.5, 0.0)),
        )
        .spawn(
            Entity::new("ground")
                .mesh(Mesh::plane(40.0))
                .material(Material::color(Color::GRAY))
                .at(Vec3::ZERO),
        )
        .camera(Camera::perspective(60.0).look_at(vec3(5.0, 3.0, 7.0), Vec3::ZERO))
        .light(Light::sun(vec3(-0.35, -1.0, -0.25)).intensity(1.2))
        .ambient(Color::rgb(0.15, 0.16, 0.18))
        .run(|ctx| {
            if ctx.input().key_pressed(Key::Escape) {
                ctx.quit();
            }
            if let Some(cube) = ctx.world_mut().get_mut("cube") {
                cube.rotate_y(1.1 * ctx.dt());
            }
        });
}
```

**Playground controls:** WASD + Q/E move, right-drag orbit, Escape quit.

## Examples

| Example | Command |
|---------|---------|
| Hello cube | `cargo run -p kerabit --example hello` |
| Hello Juni (3.0) | `cargo run -p kerabit --example hello_juni` |
| **Spark** (script-first) | `cargo run -p spark` |
| **Strike** (FPS arena) | `cargo run -p strike` |
| **Reach** (flagship) | `cargo run -p reach` |
| **Surge** (score-attack) | `cargo run -p surge` |
| **Showcase** (trailer) | `cargo run -p showcase` |
| Playground | `cargo run -p kerabit --example playground` |
| Many cubes (~10k, release) | `cargo run -p kerabit --example many_cubes --release` |
| Load mesh | `cargo run -p kerabit --example load_mesh` |
| Physics + audio | `cargo run -p kerabit --example physics_audio` |
| Physics sandbox (M2) | `cargo run -p kerabit --example physics_sandbox` |
| Mini game (legacy) | `cargo run -p kerabit --example mini_game` |

## Docs

| Doc | Purpose |
|-----|---------|
| [Site docs](https://kerabitengine.vercel.app/docs/) | Getting Started, API tour, Juni scripting, Editor |
| [ROADMAP.md](ROADMAP.md) | Summit M0–M9, Scripting Summit M10–M15 (2.0), Juni + Render tier M16–M24 (3.0) |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Crates, frame loop, GPU model, phase status |
| [API.md](API.md) | Public surface contract + 1.0 freeze + Frozen for 3.0 Juni host |
| [CHANGELOG.md](CHANGELOG.md) | Release notes |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Ownership, editor workflow, accept gates |

## License

MIT OR Apache-2.0 — see [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
