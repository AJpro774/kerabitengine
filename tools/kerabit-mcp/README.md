# Kerabit MCP

Stdio [Model Context Protocol](https://modelcontextprotocol.io/) server for the Kerabit repo. Gives Cursor (or any MCP host) **workflow control**: docs lookup, scene/script read-write/validate, scaffold, and `cargo run` / stop for Spark, Reach, Surge, Showcase, the editor, and examples.

Does **not** inject into a live wgpu frame loop (no mid-run entity IPC).

## Setup

```bash
cd tools/kerabit-mcp
npm install
npm run build
```

## Cursor (`mcp.json`)

Use an absolute path to your clone:

```json
{
  "mcpServers": {
    "kerabit": {
      "command": "npx",
      "args": ["-y", "tsx", "tools/kerabit-mcp/src/index.ts"],
      "cwd": "/absolute/path/to/Kerabit",
      "env": {
        "KERABIT_ROOT": "/absolute/path/to/Kerabit"
      }
    }
  }
}
```

Stable built entry (after `npm run build`):

```json
{
  "mcpServers": {
    "kerabit": {
      "command": "node",
      "args": ["tools/kerabit-mcp/dist/index.js"],
      "cwd": "/absolute/path/to/Kerabit",
      "env": {
        "KERABIT_ROOT": "/absolute/path/to/Kerabit"
      }
    }
  }
}
```

## Tools

| Tool | Purpose |
|------|---------|
| `kerabit_status` | Root, version, packages |
| `kerabit_host_api` | Frozen for 3.0 Juni host table |
| `kerabit_search_docs` | Search API / roadmap / architecture / site scripting |
| `kerabit_list_scenes` / `kerabit_list_scripts` | Discover assets (`.kerabit.json` / `.juni`) |
| `kerabit_read` / `kerabit_write` | Sandboxed file IO under `KERABIT_ROOT` |
| `kerabit_validate_scene` | Scene JSON checks (names, parents, scripts, meshes) |
| `kerabit_check_juni` | `cargo run -p kerabit-juni --bin check_juni` (types + host API, `file:line:col` diagnostics) |
| `kerabit_list_mods` | Installed packs + `community/catalog.json` |
| `kerabit_scaffold_mod` | Create `mods/<id>` with `mod.kerabit.json` + cube scene |
| `kerabit_scaffold_game` | Spark-shaped `.kerabit.json` + `.juni` |
| `kerabit_run` / `kerabit_stop` / `kerabit_list_runs` | Detached `cargo run` lifecycle (logs in `.kerabit/mcp-runs/`) |

## Env

| Variable | Meaning |
|----------|---------|
| `KERABIT_ROOT` | Repo root (defaults to three levels above `src/` → workspace root) |

## Dev

```bash
npm run dev    # tsx src/index.ts (stdio — for MCP hosts)
npm run build
npm start      # node dist/index.js
```
