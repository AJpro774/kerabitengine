import { spawn, type ChildProcess } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

export type RunTarget =
  | "spark"
  | "reach"
  | "surge"
  | "showcase"
  | "kerabit-editor"
  | "hello"
  | "hello_juni"
  | "playground";

export const RUN_TARGETS: RunTarget[] = [
  "spark",
  "reach",
  "surge",
  "showcase",
  "kerabit-editor",
  "hello",
  "hello_juni",
  "playground",
];

type ManagedRun = {
  id: string;
  target: RunTarget;
  pid: number;
  startedAt: string;
  logPath: string;
  child: ChildProcess;
};

const runs = new Map<string, ManagedRun>();
let seq = 0;

function cargoArgs(target: RunTarget, release: boolean): string[] {
  const args = ["run"];
  if (release) args.push("--release");
  switch (target) {
    case "hello":
    case "hello_juni":
    case "playground":
      return [...args, "-p", "kerabit", "--example", target];
    default:
      return [...args, "-p", target];
  }
}

function runsDir(root: string): string {
  const dir = path.join(root, ".kerabit", "mcp-runs");
  fs.mkdirSync(dir, { recursive: true });
  return dir;
}

export function listRuns(): Omit<ManagedRun, "child">[] {
  // Drop dead processes
  for (const [id, run] of runs) {
    try {
      process.kill(run.pid, 0);
    } catch {
      runs.delete(id);
    }
  }
  return [...runs.values()].map(({ child: _c, ...rest }) => rest);
}

export function startRun(
  root: string,
  target: RunTarget,
  release = false
): Omit<ManagedRun, "child"> {
  if (!RUN_TARGETS.includes(target)) {
    throw new Error(`unknown target: ${target}`);
  }
  const id = `run-${++seq}`;
  const logPath = path.join(runsDir(root), `${id}.log`);
  const log = fs.openSync(logPath, "w");
  const child = spawn("cargo", cargoArgs(target, release), {
    cwd: root,
    env: process.env,
    detached: true,
    stdio: ["ignore", log, log],
  });
  child.unref();
  if (child.pid == null) {
    throw new Error("failed to spawn cargo");
  }
  const managed: ManagedRun = {
    id,
    target,
    pid: child.pid,
    startedAt: new Date().toISOString(),
    logPath: path.relative(root, logPath).split(path.sep).join("/"),
    child,
  };
  runs.set(id, managed);
  child.on("exit", () => {
    runs.delete(id);
  });
  const { child: _c, ...rest } = managed;
  return rest;
}

function killTree(pid: number): void {
  if (process.platform === "win32") {
    spawn("taskkill", ["/PID", String(pid), "/T", "/F"], {
      stdio: "ignore",
      windowsHide: true,
    });
    return;
  }
  try {
    process.kill(-pid, "SIGTERM");
  } catch {
    process.kill(pid, "SIGTERM");
  }
}

export function stopRun(id: string): string {
  const run = runs.get(id);
  if (!run) {
    return `unknown run id: ${id}`;
  }
  try {
    killTree(run.pid);
  } catch (err) {
    return `stop failed: ${err instanceof Error ? err.message : String(err)}`;
  }
  runs.delete(id);
  return `stopped ${id} (pid ${run.pid})`;
}

export function stopAll(): string[] {
  const ids = [...runs.keys()];
  return ids.map((id) => stopRun(id));
}
