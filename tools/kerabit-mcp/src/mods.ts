import fs from "node:fs";
import path from "node:path";

import { resolveInRoot, writeText } from "./root.js";

export type ModManifest = {
  id: string;
  name: string;
  version?: string;
  author?: string;
  description?: string;
  game?: string;
  scenes?: string[];
  homepage?: string;
};

export type ListedMod = ModManifest & { dir: string };

function walkMods(dir: string, out: ListedMod[]): void {
  if (!fs.existsSync(dir)) return;
  for (const ent of fs.readdirSync(dir, { withFileTypes: true })) {
    if (!ent.isDirectory()) continue;
    const pack = path.join(dir, ent.name);
    const manifestPath = path.join(pack, "mod.kerabit.json");
    if (!fs.existsSync(manifestPath)) continue;
    try {
      const raw = JSON.parse(fs.readFileSync(manifestPath, "utf8")) as ModManifest;
      if (!raw.id || !raw.name) continue;
      out.push({
        ...raw,
        dir: pack,
      });
    } catch {
      // skip broken manifests
    }
  }
}

export function listMods(root: string): ListedMod[] {
  const found: ListedMod[] = [];
  walkMods(path.join(root, "mods"), found);
  const dataDir = process.env.KERABIT_HOME
    ? process.env.KERABIT_HOME
    : process.env.HOME || process.env.USERPROFILE
      ? path.join((process.env.HOME || process.env.USERPROFILE) as string, ".kerabit")
      : "";
  if (dataDir) {
    walkMods(path.join(dataDir, "mods"), found);
  }
  if (process.env.KERABIT_MODS) {
    const sep = process.platform === "win32" ? ";" : ":";
    for (const extra of process.env.KERABIT_MODS.split(sep)) {
      if (extra) walkMods(extra, found);
    }
  }
  const byId = new Map<string, ListedMod>();
  for (const m of found) {
    byId.set(m.id, m);
  }
  return [...byId.values()].sort((a, b) => a.name.localeCompare(b.name));
}

export function catalog(root: string): unknown {
  const p = resolveInRoot(root, "community/catalog.json");
  if (!fs.existsSync(p)) return { version: 1, mods: [] };
  return JSON.parse(fs.readFileSync(p, "utf8"));
}

const SAMPLE_SCENE = `{
  "version": 1,
  "clear_color": [0.08, 0.09, 0.12],
  "ambient": [0.15, 0.16, 0.18],
  "camera": {
    "fov_y": 60.0,
    "eye": [5.0, 3.0, 7.0],
    "target": [0.0, 0.0, 0.0],
    "near": 0.1,
    "far": 100.0
  },
  "light": {
    "direction": [-0.35, -1.0, -0.25],
    "intensity": 1.2,
    "color": [1.0, 1.0, 1.0]
  },
  "extras": { "script": "entry.juni" },
  "entities": [
    {
      "name": "cube",
      "mesh": { "type": "cube" },
      "material": { "color": [0.91, 1.0, 0.29], "roughness": 0.35 },
      "at": [0.0, 0.5, 0.0]
    },
    {
      "name": "ground",
      "mesh": { "type": "plane", "size": 40.0 },
      "material": { "color": [0.5, 0.5, 0.5], "roughness": 0.9 },
      "at": [0.0, 0.0, 0.0]
    }
  ]
}
`;

const SAMPLE_SCRIPT = `# Community mod — main once, frame every Play frame.
state:
    cube: i32 = 0
    t: f32 = 0.0

fn main() -> i32:
    cube = entity("cube")
    return 0

fn frame(dt: f32) -> i32:
    t = t + dt
    rotate_y(cube, 1.1 * dt)
    if key_pressed("Escape"):
        quit()
    return 0
`;

export function scaffoldMod(
  root: string,
  id: string,
  name: string
): { dir: string; files: string[] } {
  if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(id)) {
    throw new Error("id must be kebab-case [a-z0-9-]");
  }
  const dirRel = path.posix.join("mods", id);
  const abs = resolveInRoot(root, dirRel);
  if (fs.existsSync(abs)) {
    throw new Error(`${dirRel} already exists`);
  }
  const manifest = {
    id,
    name,
    version: "0.1.0",
    author: "",
    description: "A Kerabit community mod.",
    game: "*",
    scenes: ["scenes/entry.kerabit.json"],
    homepage: "",
  };
  writeText(root, path.posix.join(dirRel, "mod.kerabit.json"), JSON.stringify(manifest, null, 2) + "\n");
  writeText(root, path.posix.join(dirRel, "scenes/entry.kerabit.json"), SAMPLE_SCENE);
  writeText(root, path.posix.join(dirRel, "scenes/entry.juni"), SAMPLE_SCRIPT);
  return {
    dir: dirRel,
    files: [
      path.posix.join(dirRel, "mod.kerabit.json"),
      path.posix.join(dirRel, "scenes/entry.kerabit.json"),
      path.posix.join(dirRel, "scenes/entry.juni"),
    ],
  };
}
