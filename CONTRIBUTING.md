# Contributing

Kerabit is designed for **multi-model / multi-session** work: own a crate or a feature, not the whole engine.

## Operating rules

1. **Own a crate or a feature.** Prefer PRs/commits scoped to one clear change.
2. **Do not expand or break the public API** without updating [API.md](API.md) in the same change. Breaking a **Frozen for 1.0** or **Frozen for 2.0** item also needs a semver bump + [CHANGELOG.md](CHANGELOG.md) entry — see the freeze table in API.md.
3. **Never expose `wgpu::*` or `winit::*`** from `kerabit`’s public surface (except a documented advanced module later).
4. **Shaders** live in `crates/kerabit-render/shaders/` as `.wgsl`, included via `include_str!`.
5. **Examples** must compile against the public API only.
6. **Accept gate:** `cargo test --workspace` (and relevant `cargo run -p …` smoke) must pass.
7. **No drive-by refactors** of crates you do not own in that session.
8. **New dependencies** need a one-line justification in [ARCHITECTURE.md](ARCHITECTURE.md) “Deps” and must respect the size budget.

## Editor workflow

Author and edit playable scenes in **`kerabit-editor`** (`cargo run -p kerabit-editor`), not by hand-editing JSON unless necessary.

- Open levels under `games/reach/levels/` or `games/surge/levels/`.
- File → Save writes `.kerabit.json`. Prefer **Play** in the editor to smoke a scene when available.
- **Juni (3.0)** — Host API + Script panel (Check / Reload); `extras.script` on the scene or an entity. `cargo run -p spark` · `cargo run -p kerabit --example hello_juni`. Type-check a script: `cargo run -p kerabit-juni --bin check_juni -- path.juni`. The host table lives in `crates/kerabit-juni/juni/kerabit.juni`; adding a function means: extern line there → `host.rs` `func_wrap` → API.md row → MCP `docs.ts` table.
- **MCP** — Agent tooling in `tools/kerabit-mcp` (stdio). See [tools/kerabit-mcp/README.md](tools/kerabit-mcp/README.md).
- **Reach** — registered in `games/reach/src/main.rs` (`LEVEL_FILES` + `CHAPTERS`). Tags: `player`, `goal`, `ground`, `wall`, `hazard`. Keep unit-cube players (`half = 0.5`) and leave dodge gaps ≥ **1.0**. Best times live in `~/.kerabit/reach_progress.txt` (Windows: `%USERPROFILE%\.kerabit\`). Full Windows move: [WINDOWS.md](WINDOWS.md).
- **Surge** — registered in `games/surge/src/main.rs`. Same role tags (no `goal`); hazard motion tags: `orbit`, `slide_x`, `slide_z` (experimental — see API.md).

## Packaging (Reach)

```bash
# macOS → dist/Reach.app + Reach-macos.zip
./scripts/package-reach.sh
./scripts/package-reach.sh --skip-build   # reuse existing release binary

# Windows (PowerShell) → dist/Reach-windows/ + Reach-windows.zip
pwsh ./scripts/package-reach-windows.ps1
pwsh ./scripts/package-reach-windows.ps1 -SkipBuild
```

Accept: unzip `dist/Reach-macos.zip` on a Mac and double-click **Reach.app**; unzip `Reach-windows.zip` and run `reach.exe` beside `levels/` + `assets/`. All Windows products: `pwsh ./scripts/package-windows.ps1`. Icon (macOS) from `games/reach/packaging/AppIcon.png`. Do not commit `dist/`.

**CI artifacts:** `.github/workflows/package-reach.yml` is manual (`workflow_dispatch`). It builds release Reach on `macos-latest` and `windows-latest` and uploads `Reach-macos` / `Reach-windows` artifacts. Attach those zips to a GitHub Release when cutting a player build; the site download section points at Releases + the packaging scripts.

## Site docs

Static pages under `site/docs/` (Getting Started, API tour, Juni scripting, Editor, Community mods). Deploy from repo root so `vercel.json` `cleanUrls` apply (`kerabitengine.vercel.app`). Keep the stranger path: rustup → clone → `cargo run -p kerabit --example hello` → `hello_juni` / `spark` → `cargo run -p kerabit-editor`. Engine and editor from source: macOS / Windows / Linux. Reach player zips: macOS + Windows.

## Working on the Juni compiler

`kerabit-juni` pins the compiler crates to a **git tag** of [Juno](https://github.com/AJpro774/Juno) (see `crates/kerabit-juni/Cargo.toml`). To develop against a local checkout without editing manifests, add a gitignored `.cargo/config.toml` at the repo root:

```toml
[patch."https://github.com/AJpro774/Juno.git"]
juni-driver = { path = "/path/to/Juno/crates/juni-driver" }
juni-check = { path = "/path/to/Juno/crates/juni-check" }
```

Land language / codegen changes in Juno first, tag them, then bump the tag here in the same change as any prelude or host update.

## Local checks

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p reach
cargo run -p surge
cargo run -p kerabit-editor
# M8 stress (~10k cubes; prefer --release)
cargo run -p kerabit --example many_cubes --release
cargo run -p kerabit --example hello_juni
cargo run -p kerabit-juni --bin check_juni -- games/spark/scenes/spark.juni
```

Prefer `cargo fmt` / `clippy -D warnings` before opening a PR. CI runs check+test+clippy on macOS, Windows, and Ubuntu (see `.github/workflows/ci.yml`).

## What not to do

- Do not edit the plan file as a substitute for shipping code.
- Do not commit `target/`, secrets, or large binary assets.
- Do not pull in Bevy/Unity/Godot or other full engines as dependencies.
- Do not invent a new install path — authors use `rustup` + `git clone` + `cargo run -p …`.
