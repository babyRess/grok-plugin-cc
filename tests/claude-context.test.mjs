import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, it, before, after } from "node:test";

import {
  collectClaudeContext,
  formatClaudeContextBlock,
  maybeInjectClaudeContext
} from "../plugins/grok/scripts/lib/claude-context.mjs";

describe("claude-context", () => {
  /** @type {string} */
  let dir;
  /** @type {string} */
  let home;

  before(() => {
    dir = fs.mkdtempSync(path.join(os.tmpdir(), "claude-ctx-"));
    home = path.join(dir, "home");
    fs.mkdirSync(path.join(dir, ".git"), { recursive: true });
    fs.mkdirSync(path.join(dir, ".claude", "skills", "demo-skill"), { recursive: true });
    fs.writeFileSync(
      path.join(dir, ".claude", "skills", "demo-skill", "SKILL.md"),
      "---\nname: demo-skill\ndescription: Demo skill for node tests\n---\n\n# Demo\n"
    );
    fs.mkdirSync(path.join(home, ".claude"), { recursive: true });
    // Monkey-patch os.homedir by writing to real home is bad — collectClaudeContext uses os.homedir().
    // For this test we only verify project-local skills via workspace discovery.
  });

  after(() => {
    fs.rmSync(dir, { recursive: true, force: true });
  });

  it("finds project .claude/skills", () => {
    const ctx = collectClaudeContext(dir);
    assert.ok(ctx.skills.some((s) => s.name === "demo-skill"));
    const block = formatClaudeContextBlock(ctx, "compact");
    assert.match(block, /Claude Code context/);
    assert.match(block, /demo-skill/);
    assert.ok(block.length < 2000, `compact block too large: ${block.length}`);
  });

  it("injects block before prompt", () => {
    const out = maybeInjectClaudeContext("DO THE THING", dir, true, "compact");
    assert.match(out, /Claude Code context/);
    assert.match(out, /DO THE THING/);
    assert.ok(out.indexOf("Claude Code") < out.indexOf("DO THE THING"));
  });

  it("skips inject when disabled", () => {
    const out = maybeInjectClaudeContext("DO THE THING", dir, false);
    assert.equal(out, "DO THE THING");
  });
});
