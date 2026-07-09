import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { describe, it, beforeEach, afterEach } from "node:test";

import { COMPANION, makeTempDir } from "./helpers.mjs";

function runCompanion(args, options = {}) {
  return spawnSync(process.execPath, [COMPANION, ...args], {
    encoding: "utf8",
    cwd: options.cwd ?? process.cwd(),
    env: {
      ...process.env,
      ...(options.env ?? {})
    },
    timeout: 30_000
  });
}

describe("companion CLI", () => {
  /** @type {string} */
  let dir;
  /** @type {string | undefined} */
  let previousData;

  beforeEach(() => {
    dir = makeTempDir();
    fs.mkdirSync(path.join(dir, ".git"), { recursive: true });
    previousData = process.env.CLAUDE_PLUGIN_DATA;
    process.env.CLAUDE_PLUGIN_DATA = path.join(dir, "plugin-data");
  });

  afterEach(() => {
    if (previousData === undefined) {
      delete process.env.CLAUDE_PLUGIN_DATA;
    } else {
      process.env.CLAUDE_PLUGIN_DATA = previousData;
    }
    fs.rmSync(dir, { recursive: true, force: true });
  });

  it("prints usage for unknown command", () => {
    const result = runCompanion(["nope"], { cwd: dir });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr + result.stdout, /Unknown command|Usage:/);
  });

  it("runs setup --json", () => {
    const result = runCompanion(["setup", "--json"], {
      cwd: dir,
      env: { CLAUDE_PLUGIN_DATA: path.join(dir, "plugin-data") }
    });
    assert.equal(result.status === 0 || result.status === 1, true);
    const parsed = JSON.parse(result.stdout);
    assert.equal(typeof parsed.ready, "boolean");
    assert.ok(parsed.grok);
    assert.ok(parsed.node);
  });

  it("toggles stop review gate", () => {
    const enable = runCompanion(["setup", "--enable-review-gate", "--json"], {
      cwd: dir,
      env: { CLAUDE_PLUGIN_DATA: path.join(dir, "plugin-data") }
    });
    const enabled = JSON.parse(enable.stdout);
    assert.equal(enabled.config.stopReviewGate, true);

    const disable = runCompanion(["setup", "--disable-review-gate", "--json"], {
      cwd: dir,
      env: { CLAUDE_PLUGIN_DATA: path.join(dir, "plugin-data") }
    });
    const disabled = JSON.parse(disable.stdout);
    assert.equal(disabled.config.stopReviewGate, false);
  });

  it("status with no jobs", () => {
    const result = runCompanion(["status"], {
      cwd: dir,
      env: { CLAUDE_PLUGIN_DATA: path.join(dir, "plugin-data") }
    });
    assert.equal(result.status, 0);
    assert.match(result.stdout, /No Grok companion jobs/);
  });

  it("task-resume-candidate when empty", () => {
    const result = runCompanion(["task-resume-candidate", "--json"], {
      cwd: dir,
      env: { CLAUDE_PLUGIN_DATA: path.join(dir, "plugin-data") }
    });
    assert.equal(result.status, 0);
    const parsed = JSON.parse(result.stdout);
    assert.equal(parsed.available, false);
  });
});
