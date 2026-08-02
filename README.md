# Kerabit

Lean native Rust 3D engine: **simple for the game author, deep in the engine**.

**Site:** [kerabitengine.vercel.app](https://kerabitengine.vercel.app) · **Repo:** [github.com/AJpro774/kerabitengine](https://github.com/AJpro774/kerabitengine)

> **Status:** **Alpha** (`1.0.0-alpha.2`). Flagship: **Reach**. Second title: **Surge**. Trailer: **Showcase**. Summit roadmap: [ROADMAP.md](ROADMAP.md).

## Alpha

Clone-and-cargo release for authors. Install is unchanged:

1. Install [Rust stable](https://rustup.rs/) via `rustup` (toolchain pinned in `rust-toolchain.toml`)
2. Working GPU drivers (Metal on macOS; Vulkan/Metal/DX12 via wgpu elsewhere)
3. Clone and run:

```bash
git clone https://github.com/AJpro774/kerabitengine.git
cd kerabitengine
cargo run -p reach
cargo run -p surge
cargo run -p showcase
cargo run -p kerabit-editor
```

Frozen vs experimental public APIs: [API.md](API.md). Release notes: [CHANGELOG.md](CHANGELOG.md).

## Goals

- Tiny game-facing API (builder + `run` closure; no wgpu in user code)
- Real wgpu renderer, scene graph, assets, physics/audio, `.kerabit.json` scenes
- Install/build footprint far under a 20GB budget (target: &lt; 1GB toolchain + debug build)

## Play Reach (release)

**Players:** unzip a published player zip — no Rust required.

| Platform | Artifact | How to run |
|----------|----------|------------|
| macOS | `Reach-macos.zip` | Double-click **Reach.app** |
| Windows | `Reach-windows.zip` | Run `reach.exe` (keep `levels/` + `assets/` beside it) |

Zips appear on [GitHub Releases](https://github.com/AJpro774/kerabitengine/releases) when cut, or as CI artifacts from the `package-reach` workflow (`workflow_dispatch`). Site download notes: [kerabitengine.vercel.app/#download](https://kerabitengine.vercel.app/#download).

**Build the zip yourself** (needs Rust; macOS Xcode CLT only if you pass `--rebuild-icon`):

```bash
# macOS
./scripts/package-reach.sh
# → dist/Reach.app and dist/Reach-macos.zip
open dist/Reach.app

# Windows (PowerShell)
pwsh ./scripts/package-reach-windows.ps1
# → dist/Reach-windows/ and dist/Reach-windows.zip
```

**Dev run** (source tree):

```bash
cargo run -p reach
# or release binary:
cargo build -p reach --release && ./target/release/reach
```

Controls: **Space** start / next · **WASD** move · **R** retry · **Escape** quit.

## Docs (site)

| Guide | URL |
|-------|-----|
| Getting Started (≤30 min stranger path) | [docs/getting-started](https://kerabitengine.vercel.app/docs/getting-started) |
| API tour | [docs/api-tour](https://kerabitengine.vercel.app/docs/api-tour) |
| Editor guide | [docs/editor](https://kerabitengine.vercel.app/docs/editor) |

## Quick start (engine / authors)

```bash
# Requires a recent stable Rust toolchain (pinned in rust-toolchain.toml)
cargo run -p kerabit --example hello
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

Open a Reach or Surge level under `games/*/levels/`. Central 3D viewport (orbit RMB, pan MMB, zoom scroll), click to select (**Shift+click** multi-select), **W/E/R** for move/rotate/scale gizmos, configurable snap (persisted in `~/.kerabit/editor.json`), **Place cube** then click the ground plane. **Ctrl+Z / Ctrl+Shift+Z** undo/redo; Edit → Align X/Y/Z; File → Save Prefab / Instance Prefab (`.kerabit.prefab.json`, samples in `games/reach/prefabs/`). File → Save writes `.kerabit.json`. **Play** runs the scene in a child window (dirty scenes use a temp snapshot; Esc returns with selection intact). Editor is a **dev tool** — not bundled inside the shipped Reach.app.

### Reach (flagship)

```bash
cargo run -p reach
```

Campaign of 12 levels across 3 chapters: title → chapter select → WASD to the cyan pad → avoid red hazards → CLEAR / RETRY. Levels live in `games/reach/levels/` (edit with `kerabit-editor`). Release packaging: `./scripts/package-reach.sh` / `package-reach-windows.ps1` (see **Play Reach** above).

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
| **Reach** (flagship) | `cargo run -p reach` |
| **Surge** (score-attack) | `cargo run -p surge` |
| **Showcase** (trailer) | `cargo run -p showcase` |
| Playground | `cargo run -p kerabit --example playground` |
| Many cubes | `cargo run -p kerabit --example many_cubes --release` |
| Load mesh | `cargo run -p kerabit --example load_mesh` |
| Physics + audio | `cargo run -p kerabit --example physics_audio` |
| Physics sandbox (M2) | `cargo run -p kerabit --example physics_sandbox` |
| Mini game (legacy) | `cargo run -p kerabit --example mini_game` |

## Docs

| Doc | Purpose |
|-----|---------|
| [Site docs](https://kerabitengine.vercel.app/docs/) | Getting Started, API tour, Editor guide |
| [ROADMAP.md](ROADMAP.md) | Summit moonshot phases M0–M9 |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Crates, frame loop, GPU model, phase status |
| [API.md](API.md) | Public surface contract + alpha freeze |
| [CHANGELOG.md](CHANGELOG.md) | Release notes |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Ownership, editor workflow, accept gates |

## License

MIT OR Apache-2.0 — see [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
