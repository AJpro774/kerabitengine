import fs from "node:fs";
import path from "node:path";

import { resolveInRoot, writeText } from "./root.js";

type SceneMesh =
  | { type: "cube" }
  | { type: "plane"; size?: number }
  | { type: "obj"; path: string }
  | { type: "gltf"; path: string };

type SceneEntity = {
  name: string;
  parent?: string;
  mesh?: SceneMesh;
  material?: { texture?: string; normal_map?: string };
  extras?: Record<string, unknown>;
  components?: Record<string, unknown>;
};

type SceneFile = {
  version?: number;
  extras?: Record<string, unknown>;
  components?: Record<string, unknown>;
  entities?: SceneEntity[];
};

function walk(dir: string, pred: (name: string) => boolean, out: string[]): void {
  if (!fs.existsSync(dir)) return;
  for (const ent of fs.readdirSync(dir, { withFileTypes: true })) {
    if (ent.name === "target" || ent.name === "node_modules" || ent.name === ".git") {
      continue;
    }
    const abs = path.join(dir, ent.name);
    if (ent.isDirectory()) {
      walk(abs, pred, out);
    } else if (ent.isFile() && pred(ent.name)) {
      out.push(abs);
    }
  }
}

export function listScenes(root: string): string[] {
  const found: string[] = [];
  for (const sub of ["games", "examples"]) {
    walk(path.join(root, sub), (n) => n.endsWith(".kerabit.json"), found);
  }
  return found
    .map((abs) => path.relative(root, abs).split(path.sep).join("/"))
    .sort();
}

export function listScripts(root: string): string[] {
  const found: string[] = [];
  for (const sub of ["games", "examples", "tools"]) {
    walk(path.join(root, sub), (n) => n.endsWith(".juni"), found);
  }
  return found
    .map((abs) => path.relative(root, abs).split(path.sep).join("/"))
    .sort();
}

function mapScriptPath(bag?: Record<string, unknown>): string | undefined {
  if (!bag) return undefined;
  const v = bag.script;
  return typeof v === "string" && v.length > 0 ? v : undefined;
}

function assetExists(root: string, sceneDir: string, rel: string): boolean {
  if (!rel) return false;
  if (path.isAbsolute(rel) && fs.existsSync(rel)) return true;
  if (fs.existsSync(path.join(sceneDir, rel))) return true;
  if (fs.existsSync(path.join(root, rel))) return true;
  return false;
}

/** Port of tools/kerabit-editor validation (names, parents, assets, scripts). */
export function validateScene(root: string, rel: string): string[] {
  const abs = resolveInRoot(root, rel);
  const sceneDir = path.dirname(abs);
  let data: SceneFile;
  try {
    data = JSON.parse(fs.readFileSync(abs, "utf8")) as SceneFile;
  } catch (err) {
    return [`invalid JSON: ${err instanceof Error ? err.message : String(err)}`];
  }

  const errors: string[] = [];
  const entities = data.entities ?? [];
  const counts = new Map<string, number>();
  for (const e of entities) {
    counts.set(e.name, (counts.get(e.name) ?? 0) + 1);
  }
  for (const [name, count] of counts) {
    if (count > 1) {
      errors.push(`duplicate entity name "${name}" (${count}×)`);
    }
  }

  const names = entities.map((e) => e.name);
  for (const e of entities) {
    if (e.parent) {
      if (!names.includes(e.parent)) {
        errors.push(`entity "${e.name}" parent "${e.parent}" not found`);
      }
      if (e.parent === e.name) {
        errors.push(`entity "${e.name}" parents itself`);
      }
    }
    const mesh = e.mesh;
    if (mesh && (mesh.type === "obj" || mesh.type === "gltf")) {
      if (!assetExists(root, sceneDir, mesh.path)) {
        errors.push(`entity "${e.name}": missing mesh asset ${mesh.path}`);
      }
    }
    const tex = e.material?.texture;
    if (tex && !assetExists(root, sceneDir, tex)) {
      errors.push(`entity "${e.name}": missing texture ${tex}`);
    }
    const nrm = e.material?.normal_map;
    if (nrm && !assetExists(root, sceneDir, nrm)) {
      errors.push(`entity "${e.name}": missing normal map ${nrm}`);
    }
    const script =
      mapScriptPath(e.extras) ?? mapScriptPath(e.components);
    if (script && !assetExists(root, sceneDir, script)) {
      errors.push(`entity "${e.name}": missing script ${script}`);
    }
  }

  const sceneScript =
    mapScriptPath(data.extras) ?? mapScriptPath(data.components);
  if (sceneScript && !assetExists(root, sceneDir, sceneScript)) {
    errors.push(`scene script missing: ${sceneScript}`);
  }

  return errors;
}

export function scaffoldGame(
  root: string,
  targetDir: string,
  name: string
): { scene: string; script: string } {
  const safe = name.replace(/[^a-zA-Z0-9_-]/g, "_") || "game";
  const base = targetDir.replace(/\/+$/, "");
  const sceneRel = `${base}/scenes/${safe}.kerabit.json`;
  const scriptRel = `${base}/scenes/${safe}.juni`;

  const scene = {
    version: 1,
    clear_color: [0.07, 0.08, 0.11],
    ambient: [0.14, 0.15, 0.18],
    camera: {
      fov_y: 55.0,
      eye: [0.0, 11.0, 14.0],
      target: [0.0, 0.4, 0.0],
      near: 0.1,
      far: 100.0,
    },
    light: {
      direction: [-0.4, -1.0, -0.25],
      intensity: 1.25,
      color: [0.95, 0.98, 1.0],
    },
    extras: { script: `${safe}.juni` },
    entities: [
      {
        name: "ground",
        tags: ["ground"],
        mesh: { type: "plane", size: 16.0 },
        material: { color: [0.32, 0.36, 0.42], roughness: 0.92 },
        at: [0.0, 0.0, 0.0],
      },
      {
        name: "player",
        tags: ["player"],
        mesh: { type: "cube" },
        material: { color: [0.32, 0.86, 0.48], roughness: 0.4 },
        at: [-5.0, 0.5, 0.0],
        scale: [0.8, 0.8, 0.8],
      },
      {
        name: "goal",
        tags: ["goal"],
        mesh: { type: "cube" },
        material: { color: [0.2, 0.85, 0.95], roughness: 0.35 },
        at: [5.0, 0.35, 0.0],
        scale: [1.2, 0.25, 1.2],
      },
    ],
  };

  const script = `# ${safe} — scaffolded by kerabit-mcp (Spark-shaped, Juni)
# \`main\` runs once at load; \`frame\` runs every frame. Values live in \`state:\`.

state:
    player: i32 = 0
    goal: i32 = 0
    won: bool = false

fn main() -> i32:
    player = entity("player")
    goal = entity("goal")
    return 0

fn frame(dt: f32) -> i32:
    if key_pressed("Escape"):
        quit()
        return 0
    let px = pos_x(player)
    let pz = pos_z(player)
    set_camera(px * 0.15, 11.0, 14.0, px * 0.2, 0.4, pz * 0.2)

    let speed = 6.5
    let dx = 0.0
    let dz = 0.0
    if key_down("W") or key_down("Up"):
        dz = dz - 1.0
    if key_down("S") or key_down("Down"):
        dz = dz + 1.0
    if key_down("A") or key_down("Left"):
        dx = dx - 1.0
    if key_down("D") or key_down("Right"):
        dx = dx + 1.0
    let len = sqrt(dx * dx + dz * dz)
    if len > 0.001:
        move_planar(player, dx / len * speed * dt, dz / len * speed * dt)

    if not won and near(player, goal, 1.15):
        won = true
        spawn_particles(pos_x(goal), 1.0, pos_z(goal), 40, 0.3, 0.9, 1.0)

    ui_text(0.03, 0.03, 0.028, 0.91, 1.0, 0.29, "${safe.toUpperCase()}")
    ui_text(0.03, 0.07, 0.022, 0.85, 0.82, 0.75, "WASD move  Esc quit")
    if won:
        ui_text(0.4, 0.4, 0.06, 0.91, 1.0, 0.29, "CLEAR")
    return 0
`;

  writeText(root, sceneRel, JSON.stringify(scene, null, 2) + "\n");
  writeText(root, scriptRel, script);
  return { scene: sceneRel, script: scriptRel };
}
