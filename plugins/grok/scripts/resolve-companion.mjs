#!/usr/bin/env node
/**
 * Prefer the native Rust binary when present; otherwise fall back to Node.
 *
 * Usage:
 *   node resolve-companion.mjs <command> [args...]
 */
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const PLUGIN_ROOT = path.resolve(SCRIPT_DIR, "..");
const RUST_BIN = path.join(PLUGIN_ROOT, "bin", "grok-companion");
const NODE_COMPANION = path.join(SCRIPT_DIR, "grok-companion.mjs");

const argv = process.argv.slice(2);
const command = argv[0];

// Full Rust parity: all companion subcommands
const RUST_COMMANDS = new Set([
  "setup",
  "status",
  "review",
  "adversarial-review",
  "task",
  "task-worker",
  "result",
  "cancel",
  "task-resume-candidate"
]);

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

if (
  command &&
  RUST_COMMANDS.has(command) &&
  fs.existsSync(RUST_BIN) &&
  fs.statSync(RUST_BIN).isFile()
) {
  run(RUST_BIN, argv, false);
} else {
  run(NODE_COMPANION, argv, true);
}
