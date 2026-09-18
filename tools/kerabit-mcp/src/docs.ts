import fs from "node:fs";
import path from "node:path";

const DOC_FILES = [
  "API.md",
  "ROADMAP.md",
  "ARCHITECTURE.md",
  "CHANGELOG.md",
  "README.md",
  "site/docs/scripting.html",
] as const;

/** Embedded Frozen for 3.0 host snapshot (kept in sync with API.md and crates/kerabit-juni/juni/kerabit.juni). */
export const HOST_API_TABLE = `| Function | Notes |
|----------|-------|
| \`dt() -> f32\` / \`quit()\` / \`reload_scripts()\` / \`log(text)\` | Frame delta; quit; force reload; log line |
| \`key_down(name) -> bool\` / \`key_pressed(name) -> bool\` | Case-insensitive key names ("W", "Space", "Escape") |
| \`mouse_x()\` / \`mouse_y()\` / \`mouse_down(btn)\` / \`mouse_pressed(btn)\` | \`"left"\` / \`"right"\` / \`"middle"\` |
| \`entity(name) -> i32\` / \`self_entity() -> i32\` / \`exists(e)\` / \`enabled(e)\` | Handles; \`0\` = none |
| \`pos_x/y/z(e)\` / \`scale_x/y/z(e)\` / \`has_tag(e, tag)\` / \`tag_count(tag)\` / \`tag_at(tag, i)\` | World reads |
| \`set_pos\` / \`translate\` / \`rotate_x/y/z\` / \`set_scale\` / \`set_enabled\` / \`add_tag\` / \`remove_tag\` | World writes (take \`e: i32\` first) |
| \`despawn(e)\` / \`spawn_cube(...) -> i32\` / \`spawn_cube_ex(...) -> i32\` / \`spawn_plane(...) -> i32\` / \`spawn_prefab(path, x, y, z)\` | Authorship |
| \`move_planar(e, dx, dz)\` / \`register_box(e)\` | Physics helpers |
| \`play(path)\` / \`play_at(path, x, y, z)\` / \`spawn_particles(x, y, z, count, r, g, b)\` / \`set_camera(ex, ey, ez, tx, ty, tz)\` | Juice / view |
| \`ui_text(x, y, size, r, g, b, text)\` / \`ui_rect(x, y, w, h, r, g, b, a)\` | Overlay (ASCII text; normalized 0–1) |
| \`dist_xz\` / \`dist_between(a, b)\` / \`near(a, b, radius)\` | Prelude helpers (Juni) |

Scripts are Juni (statically typed, Python-like indentation). \`fn main() -> i32\` runs once at load;
\`fn frame(dt: f32) -> i32\` runs every frame. Keep values in a \`state:\` block. Builtins such as
\`sqrt\`, \`clamp\`, \`lerp\`, \`print\` are available; canvas / WebGPU builtins are not.`;

export function hostApiText(): string {
  return `# Kerabit Juni host API (Frozen for 3.0)\n\n${HOST_API_TABLE}\n`;
}

export function searchDocs(root: string, query: string, limit = 20): string {
  const q = query.trim().toLowerCase();
  if (!q) {
    return "empty query";
  }
  const hits: { file: string; line: number; text: string }[] = [];
  for (const rel of DOC_FILES) {
    const abs = path.join(root, rel);
    if (!fs.existsSync(abs)) continue;
    const lines = fs.readFileSync(abs, "utf8").split(/\r?\n/);
    for (let i = 0; i < lines.length; i++) {
      if (lines[i].toLowerCase().includes(q)) {
        hits.push({ file: rel, line: i + 1, text: lines[i].trim() });
        if (hits.length >= limit) break;
      }
    }
    if (hits.length >= limit) break;
  }
  if (hits.length === 0) {
    return `No hits for "${query}" in ${DOC_FILES.join(", ")}`;
  }
  return hits.map((h) => `${h.file}:${h.line}: ${h.text}`).join("\n");
}
