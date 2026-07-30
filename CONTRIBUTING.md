# Contributing

Kerabit is designed for **multi-model / multi-session** work: own a crate or a phase, not the whole engine.

## Operating rules

1. **Own a crate or a phase.** Prefer PRs/commits scoped to one plan todo id (e.g. `p1-pipeline`).
2. **Do not expand the public API** without updating [API.md](API.md) in the same change.
3. **Never expose `wgpu::*` or `winit::*`** from `kerabit`’s public surface (except a documented advanced module later — not before P4).
4. **Shaders** live in `crates/kerabit-render/shaders/` as `.wgsl`, included via `include_str!`.
5. **Examples** must compile against the public API only.
6. **Accept gate before next phase:** `cargo test` + relevant `cargo run --example …` must pass.
7. **No drive-by refactors** of crates you do not own in that session.
8. **New dependencies** need a one-line justification in [ARCHITECTURE.md](ARCHITECTURE.md) “Deps” and must respect the size budget.

## Phase ownership (suggested)

| Session | Own |
|---------|-----|
| A | P0 workspace + docs |
| B | P0 math + color |
| C | P1 render window/pipeline (after workspace) |
| D | P2 world (after math) |
| E | P2 mesh/camera/light (after vertex layout freeze) |
| F | P3 facade + input + playground |
| G | P4+ / P5+ as separate tracks |

Merge order: **P0 → P1 → (P2 world ∥ P2 render scene) → P3 → P4/P5 parallel → P6 → P7**.

## Content (levels)

Author and edit playable scenes in **`kerabit-editor`** (`cargo run -p kerabit-editor`), not by hand-editing JSON unless necessary.

- **Reach** — `games/reach/levels/`, registered in `games/reach/src/main.rs` (`LEVEL_FILES`). Tags: `player`, `goal`, `ground`, `wall`, `hazard`. Keep unit-cube players (`half = 0.5`) and leave dodge gaps ≥ **1.0**.
- **Surge** — `games/surge/levels/`, registered in `games/surge/src/main.rs`. Same role tags (no `goal`); hazard motion tags: `orbit`, `slide_x`, `slide_z`.

## Packaging (Reach / E6)

```bash
./scripts/package-reach.sh          # release build + dist/Reach.app + Reach-macos.zip
./scripts/package-reach.sh --skip-build   # reuse existing release binary
```

Accept: unzip `dist/Reach-macos.zip` on a Mac and double-click **Reach.app** (no terminal). Icon from `games/reach/packaging/AppIcon.png`. Do not commit `dist/`.

## Accept gates (current)

| Phase | Gate |
|-------|------|
| P0 | `cargo build -p kerabit` succeeds; no window yet |
| **P1** | Visible shaded cube; resize safe — `cargo run -p kerabit-render --example hardcoded_cube` |
| P2 | Transform unit tests; multiple meshes |
| P3 | Playground demo; API.md matches code; no wgpu in public rustdoc |
| **E6** | `./scripts/package-reach.sh` produces `Reach.app`; levels/assets load from bundle Resources |

## Local checks (P1)

```bash
cargo build -p kerabit -p kerabit-render
cargo test -p kerabit-math -p kerabit-color
cargo run -p kerabit-render --example hardcoded_cube
```

Prefer `cargo fmt` / `clippy -D warnings` once P1 is green.

## What not to do

- Do not edit the plan file as a substitute for shipping code.
- Do not commit `target/`, secrets, or large binary assets.
- Do not pull in Bevy/Unity/Godot or other full engines as dependencies.
