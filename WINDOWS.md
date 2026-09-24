# Move Kerabit from Mac to Windows

Same repo, same `cargo` commands, same `%USERPROFILE%\.kerabit` folder name as `~/.kerabit`.

## 1. Toolchain

1. GPU drivers (recent NVIDIA / AMD / Intel — wgpu uses DX12 or Vulkan).
2. [Rust](https://rustup.rs/) — run `rustup-init.exe`. Default host `x86_64-pc-windows-msvc`.
3. **MSVC**: install [Build Tools for Visual Studio](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the **Desktop development with C++** workload (or full VS). rustc will not link without `link.exe`.
4. Optional: [Node.js](https://nodejs.org/) LTS if you use `tools/kerabit-mcp`.

Git for Windows is enough for clone. No Xcode, no WSL required.

## 2. Get the tree

```powershell
git clone https://github.com/AJpro774/kerabitengine.git
cd kerabitengine
```

Copy your Mac user data so editor snap, mods, and Reach/Surge scores survive:

```powershell
# On the Mac: zip ~/.kerabit and copy it over, then:
Expand-Archive kerabit.zip $env:USERPROFILE
# result: %USERPROFILE%\.kerabit\editor.json, mods\, reach_progress.txt, …
```

Override the folder with `KERABIT_HOME` if you want it somewhere else.

## 3. Run (same as Mac)

```powershell
cargo run -p kerabit --example hello
cargo run -p kerabit --example hello_juni
cargo run -p spark
cargo run -p strike
cargo run -p surge
cargo run -p reach
cargo run -p showcase
cargo run -p kerabit-editor
```

First build downloads crates and compiles wgpu — several minutes. After that, incremental is fast.

## 4. Windows zips (no Rust on the play machine)

On a Windows box with Rust (or GitHub Actions → **Package Windows**):

```powershell
pwsh ./scripts/package-windows.ps1
# dist\Reach-windows.zip
# dist\Surge-windows.zip
# dist\Spark-windows.zip
# dist\Strike-windows.zip
# dist\Showcase-windows.zip
# dist\Kerabit-editor-windows.zip
```

Unzip a game folder and keep the exe next to `levels/` or `scenes/` (and `assets/` when present). The editor zip already contains `games/` + `mods/` — run `kerabit-editor.exe` from that folder.

Reach-only (same as before): `pwsh ./scripts/package-reach-windows.ps1`

## 5. Paths

| What | Mac | Windows |
|------|-----|---------|
| User data | `~/.kerabit` | `%USERPROFILE%\.kerabit` |
| Extra mod roots | `KERABIT_MODS` (`:`) | `KERABIT_MODS` (`;`) |
| Game data | next to `.app` or cargo crate | next to `.exe` |

`./mods` in the repo (or editor zip) is still scanned first.

## 6. MCP

```powershell
cd tools\kerabit-mcp
npm install
npm run build
```

`kerabit_stop` uses `taskkill /T` on Windows.
