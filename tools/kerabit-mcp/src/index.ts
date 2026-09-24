#!/usr/bin/env node
/**
 * Kerabit MCP — stdio server for docs, scenes, Juni checks, and cargo run/stop.
 *
 *   KERABIT_ROOT=/path/to/Kerabit npx tsx tools/kerabit-mcp/src/index.ts
 */

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";

import { hostApiText, searchDocs } from "./docs.js";
import {
  listRuns,
  RUN_TARGETS,
  startRun,
  stopAll,
  stopRun,
  type RunTarget,
} from "./process.js";
import {
  existsInRoot,
  readText,
  resolveRoot,
  workspaceVersion,
  writeText,
} from "./root.js";
import { checkJuni } from "./scripts.js";
import { catalog, listMods, scaffoldMod } from "./mods.js";
import {
  listScenes,
  listScripts,
  scaffoldGame,
  validateScene,
} from "./scenes.js";

function text(s: string) {
  return { content: [{ type: "text" as const, text: s }] };
}

function json(data: unknown) {
  return text(JSON.stringify(data, null, 2));
}

const root = resolveRoot();

const server = new McpServer({
  name: "kerabit",
  version: "2.0.0",
});

server.tool(
  "kerabit_status",
  "Kerabit repo status: root path, workspace version, packages, MCP capabilities",
  {},
  async () =>
    json({
      root,
      version: workspaceVersion(root),
      packages: [
        "spark",
        "reach",
        "surge",
        "showcase",
        "kerabit-editor",
        "kerabit (examples: hello, hello_juni, playground)",
      ],
      run_targets: RUN_TARGETS,
      has_spark: existsInRoot(root, "games/spark/Cargo.toml"),
      has_api: existsInRoot(root, "API.md"),
    })
);

server.tool(
  "kerabit_host_api",
  "Return the Frozen for 3.0 Juni host API table",
  {},
  async () => text(hostApiText())
);

server.tool(
  "kerabit_search_docs",
  "Search Kerabit docs (API.md, ROADMAP, ARCHITECTURE, CHANGELOG, README, scripting.html)",
  { query: z.string().describe("Substring to find (case-insensitive)"), limit: z.number().int().min(1).max(100).optional() },
  async ({ query, limit }) => text(searchDocs(root, query, limit ?? 20))
);

server.tool(
  "kerabit_list_scenes",
  "List .kerabit.json scenes under games/ and examples/",
  {},
  async () => json(listScenes(root))
);

server.tool(
  "kerabit_list_scripts",
  "List .juni scripts under games/, examples/, tools/",
  {},
  async () => json(listScripts(root))
);

server.tool(
  "kerabit_read",
  "Read a text file relative to KERABIT_ROOT",
  { path: z.string().describe("Relative path, e.g. games/spark/scenes/spark.juni") },
  async ({ path: rel }) => {
    try {
      return text(readText(root, rel));
    } catch (err) {
      return text(`error: ${err instanceof Error ? err.message : String(err)}`);
    }
  }
);

server.tool(
  "kerabit_write",
  "Write/create a text file relative to KERABIT_ROOT (creates parent dirs)",
  {
    path: z.string().describe("Relative path"),
    contents: z.string().describe("Full file contents"),
  },
  async ({ path: rel, contents }) => {
    try {
      writeText(root, rel, contents);
      return text(`wrote ${rel} (${contents.length} bytes)`);
    } catch (err) {
      return text(`error: ${err instanceof Error ? err.message : String(err)}`);
    }
  }
);

server.tool(
  "kerabit_validate_scene",
  "Validate a .kerabit.json scene (duplicate names, parents, missing scripts/meshes)",
  { path: z.string().describe("Relative path to .kerabit.json") },
  async ({ path: rel }) => {
    try {
      const errors = validateScene(root, rel);
      if (errors.length === 0) {
        return text(`ok: ${rel}`);
      }
      return text(`issues in ${rel}:\n` + errors.map((e) => `- ${e}`).join("\n"));
    } catch (err) {
      return text(`error: ${err instanceof Error ? err.message : String(err)}`);
    }
  }
);

server.tool(
  "kerabit_check_juni",
  "Type-check a .juni script against the Kerabit prelude via cargo run -p kerabit-juni --bin check_juni",
  { path: z.string().describe("Relative path to .juni") },
  async ({ path: rel }) => {
    try {
      const result = checkJuni(root, rel);
      return text(result.ok ? result.output : `FAIL\n${result.output}`);
    } catch (err) {
      return text(`error: ${err instanceof Error ? err.message : String(err)}`);
    }
  }
);

server.tool(
  "kerabit_list_mods",
  "List discovered Kerabit mods (./mods, ~/.kerabit/mods, KERABIT_MODS) plus the community catalog",
  {},
  async () =>
    json({
      installed: listMods(root),
      catalog: catalog(root),
    })
);

server.tool(
  "kerabit_scaffold_mod",
  "Create a community mod pack under mods/<id> with a spinning-cube scene and Juni script",
  {
    id: z.string().describe("kebab-case pack id, e.g. cool-maps"),
    name: z.string().describe("Display name"),
  },
  async ({ id, name }) => {
    try {
      return json({ ok: true, ...scaffoldMod(root, id, name) });
    } catch (err) {
      return text(`error: ${err instanceof Error ? err.message : String(err)}`);
    }
  }
);

server.tool(
  "kerabit_scaffold_game",
  "Create Spark-shaped scene + .juni under target_dir/scenes/<name>.*",
  {
    name: z.string().describe("Game/scene name (alphanumeric, _ -)"),
    target_dir: z
      .string()
      .describe("Relative directory, e.g. games/mygame or examples/scenes_out"),
  },
  async ({ name, target_dir }) => {
    try {
      const created = scaffoldGame(root, target_dir, name);
      return json({ ok: true, ...created });
    } catch (err) {
      return text(`error: ${err instanceof Error ? err.message : String(err)}`);
    }
  }
);

server.tool(
  "kerabit_run",
  "Start a Kerabit package or example via cargo run (detached; log under .kerabit/mcp-runs/)",
  {
    target: z.enum([
      "spark",
      "reach",
      "surge",
      "showcase",
      "kerabit-editor",
      "hello",
      "hello_juni",
      "playground",
    ] as const),
    release: z.boolean().optional().describe("Pass --release to cargo"),
  },
  async ({ target, release }) => {
    try {
      const run = startRun(root, target as RunTarget, release ?? false);
      return json(run);
    } catch (err) {
      return text(`error: ${err instanceof Error ? err.message : String(err)}`);
    }
  }
);

server.tool(
  "kerabit_stop",
  "Stop a managed run by id, or all if id omitted",
  { id: z.string().optional().describe("Run id from kerabit_run; omit to stop all") },
  async ({ id }) => {
    if (id) {
      return text(stopRun(id));
    }
    const msgs = stopAll();
    return text(msgs.length ? msgs.join("\n") : "no active runs");
  }
);

server.tool(
  "kerabit_list_runs",
  "List active cargo runs started by this MCP server",
  {},
  async () => json(listRuns())
);

async function main() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
