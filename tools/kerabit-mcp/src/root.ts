import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));

/** Resolve Kerabit repo root (KERABIT_ROOT or two levels up from tools/kerabit-mcp). */
export function resolveRoot(): string {
  const fromEnv = process.env.KERABIT_ROOT?.trim();
  if (fromEnv) {
    return path.resolve(fromEnv);
  }
  // src/ -> kerabit-mcp/ -> tools/ -> repo
  return path.resolve(HERE, "..", "..", "..");
}

/** Resolve a user path relative to root; reject escapes outside the tree. */
export function resolveInRoot(root: string, rel: string): string {
  const cleaned = rel.replace(/\\/g, "/");
  if (path.isAbsolute(cleaned)) {
    throw new Error(`path must be relative to KERABIT_ROOT: ${rel}`);
  }
  if (cleaned.split("/").includes("..")) {
    throw new Error(`path must not contain '..': ${rel}`);
  }
  const abs = path.resolve(root, cleaned);
  const rootResolved = path.resolve(root);
  if (abs !== rootResolved && !abs.startsWith(rootResolved + path.sep)) {
    throw new Error(`path escapes KERABIT_ROOT: ${rel}`);
  }
  return abs;
}

export function readText(root: string, rel: string): string {
  const abs = resolveInRoot(root, rel);
  return fs.readFileSync(abs, "utf8");
}

export function writeText(root: string, rel: string, contents: string): void {
  const abs = resolveInRoot(root, rel);
  fs.mkdirSync(path.dirname(abs), { recursive: true });
  fs.writeFileSync(abs, contents, "utf8");
}

export function workspaceVersion(root: string): string {
  const toml = fs.readFileSync(path.join(root, "Cargo.toml"), "utf8");
  const m = toml.match(/\[workspace\.package\][\s\S]*?^version\s*=\s*"([^"]+)"/m);
  return m?.[1] ?? "unknown";
}

export function existsInRoot(root: string, rel: string): boolean {
  try {
    return fs.existsSync(resolveInRoot(root, rel));
  } catch {
    return false;
  }
}
