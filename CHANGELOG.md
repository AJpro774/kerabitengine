# Changelog

All notable changes to Kerabit are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/) with alpha
prerelease tags (`1.0.0-alpha.N`).

## [1.0.0-alpha.1] — 2026-07-31

First Alpha v1.0 cut for authors cloning the engine.

### Added

- Dual-license texts: `LICENSE-MIT` and `LICENSE-APACHE` (MIT OR Apache-2.0).
- GitHub Actions CI: `cargo check` + `cargo test` on macOS; `cargo check` on Ubuntu.
- Alpha API freeze table in [API.md](API.md) (frozen vs experimental surfaces).
- Marketing site clone/install section matching README cargo commands.

### Changed

- Workspace version set to `1.0.0-alpha.1`.
- `repository` metadata corrected to `https://github.com/AJpro774/kerabitengine`.
- CONTRIBUTING trimmed for alpha newcomers (stale P0–P7 session table removed).

### Notes

- **Install unchanged:** Rust stable via `rustup`, then `git clone` + `cargo run -p …`.
- Breaking changes to **Frozen for alpha** APIs require a new alpha minor bump and a CHANGELOG entry.
- Not in this alpha: crates.io publish, Windows player packaging, large renderer features.
