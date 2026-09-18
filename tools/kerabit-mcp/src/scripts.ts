import { spawnSync } from "node:child_process";
import path from "node:path";

import { resolveInRoot } from "./root.js";

/** Type-check a `.juni` script against the Kerabit prelude via `check_juni`. */
export function checkJuni(root: string, rel: string): { ok: boolean; output: string } {
  const abs = resolveInRoot(root, rel);
  const result = spawnSync(
    "cargo",
    ["run", "-q", "-p", "kerabit-juni", "--bin", "check_juni", "--", abs],
    {
      cwd: root,
      encoding: "utf8",
      env: process.env,
    }
  );
  const out = [result.stdout, result.stderr].filter(Boolean).join("").trim();
  return {
    ok: result.status === 0,
    output: out || (result.status === 0 ? `ok: ${path.relative(root, abs)}` : "check failed"),
  };
}
