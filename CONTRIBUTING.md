# Contributing

Kerabit is designed for **multi-model / multi-session** work: own a crate or a feature, not the whole engine.

## Operating rules

1. **Own a crate or a feature.** Prefer PRs/commits scoped to one clear change.
2. **Do not expand or break the public API** without updating [API.md](API.md) in the same change. Breaking a **Frozen for alpha** item also needs a new alpha bump + [CHANGELOG.md](CHANGELOG.md) entry — see the freeze table in API.md.
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
- **Reach** — registered in `games/reach/src/main.rs` (`LEVEL_FILES` + `CHAPTERS`). Tags: `player`, `goal`, `ground`, `wall`, `hazard`. Keep unit-cube players (`half = 0.5`) and leave dodge gaps ≥ **1.0**. Best times live in `~/.kerabit/reach_progress.txt` (Windows: `%LOCALAPPDATA%/Kerabit/`).
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

Accept: unzip `dist/Reach-macos.zip` on a Mac and double-click **Reach.app**; unzip `Reach-windows.zip` and run `reach.exe` beside `levels/` + `assets/`. Icon (macOS) from `games/reach/packaging/AppIcon.png`. Do not commit `dist/`.

**CI artifacts:** `.github/workflows/package-reach.yml` is manual (`workflow_dispatch`). It builds release Reach on `macos-latest` and `windows-latest` and uploads `Reach-macos` / `Reach-windows` artifacts. Attach those zips to a GitHub Release when cutting a player build; the site download section points at Releases + the packaging scripts.

## Site docs

Static pages under `site/docs/` (Getting Started, API tour, Editor). Deploy from `site/` to Vercel (`kerabitengine.vercel.app`). Keep the stranger path: rustup → clone → `cargo run -p kerabit --example hello` → `cargo run -p kerabit-editor`.

## Local checks

```bash
cargo check --workspace
cargo test --workspace
cargo run -p reach
cargo run -p surge
cargo run -p kerabit-editor
```

Prefer `cargo fmt` / `clippy -D warnings` before opening a PR. CI runs check+test on macOS, Windows, and Ubuntu (see `.github/workflows/ci.yml`).

## What not to do

- Do not edit the plan file as a substitute for shipping code.
- Do not commit `target/`, secrets, or large binary assets.
- Do not pull in Bevy/Unity/Godot or other full engines as dependencies.
- Do not invent a new install path — authors use `rustup` + `git clone` + `cargo run -p …`.
