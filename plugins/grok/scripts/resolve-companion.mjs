#!/usr/bin/env node
/**
 * Prefer native grok-companion if found; otherwise Node companion.
 *
 * Search order:
 *   1. GROK_COMPANION_BIN
 *   2. <plugin>/bin/grok-companion
 *   3. ~/.grok/bin/grok-companion   (global install-companion.sh default)
 *   4. `grok-companion` on PATH
 *   5. Node fallback: grok-companion.mjs
 */
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const PLUGIN_ROOT = path.resolve(SCRIPT_DIR, "..");
const NODE_COMPANION = path.join(SCRIPT_DIR, "grok-companion.mjs");

const argv = process.argv.slice(2);
const command = argv[0];

const RUST_COMMANDS = new Set([
  "setup",
  "status",
  "review",
  "adversarial-review",
  "task",
  "task-worker",
  "result",
  "cancel",
  "task-resume-candidate",
  "transfer"
]);

function isExecutableFile(p) {
  try {
    return Boolean(p) && fs.existsSync(p) && fs.statSync(p).isFile();
  } catch {
    return false;
  }
}

function resolveRustBin() {
  const fromEnv = process.env.GROK_COMPANION_BIN;
  if (isExecutableFile(fromEnv)) {
    return fromEnv;
  }

  const pluginBin = path.join(PLUGIN_ROOT, "bin", "grok-companion");
  if (isExecutableFile(pluginBin)) {
    return pluginBin;
  }

  const homeBin = path.join(os.homedir(), ".grok", "bin", "grok-companion");
  if (isExecutableFile(homeBin)) {
    return homeBin;
  }

  // PATH lookup
  const which = spawnSync(process.platform === "win32" ? "where" : "which", ["grok-companion"], {
    encoding: "utf8"
  });
  if (which.status === 0) {
    const candidate = which.stdout.trim().split(/\r?\n/)[0];
    if (isExecutableFile(candidate)) {
      return candidate;
    }
  }

  return null;
}

function run(bin, args, useNode = false) {
  const result = useNode
    ? spawnSync(process.execPath, [bin, ...args], {
        stdio: "inherit",
        env: process.env
      })
    : spawnSync(bin, args, {
        stdio: "inherit",
        env: process.env
      });
  process.exit(result.status ?? 1);
}

const rustBin = resolveRustBin();
if (command && RUST_COMMANDS.has(command) && rustBin) {
  run(rustBin, argv, false);
} else {
  run(NODE_COMPANION, argv, true);
}
