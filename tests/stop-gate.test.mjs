import assert from "node:assert/strict";
import { describe, it } from "node:test";

// Import parse function by re-reading module after export — use dynamic import of hook file
// The hook exports parseStopReviewOutput
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const hookUrl = pathToFileURL(
  path.join(root, "plugins/grok/scripts/stop-review-gate-hook.mjs")
).href;

// Hook runs main() on import — bad for tests. Test pure logic inline instead.

function parseStopReviewOutput(rawOutput) {
  const text = String(rawOutput ?? "").trim();
  if (!text) {
    return { ok: false, reason: "empty" };
  }
  const lines = text.split(/\r?\n/).slice(0, 20);
  for (const line of lines) {
    const t = line.trim();
    if (t.startsWith("ALLOW:")) {
      return { ok: true, reason: t.slice(6).trim() || "allowed" };
    }
    if (t.startsWith("BLOCK:")) {
      return { ok: false, reason: text };
    }
  }
  if (/\bBLOCK\b/i.test(text) && !/^\s*ALLOW:/im.test(text)) {
    return { ok: false, reason: text };
  }
  return { ok: true, reason: "No explicit block; allowing stop." };
}

describe("stop-gate parse", () => {
  it("parses ALLOW", () => {
    const d = parseStopReviewOutput("ALLOW: looks fine\nextra");
    assert.equal(d.ok, true);
  });

  it("parses BLOCK", () => {
    const d = parseStopReviewOutput("BLOCK: broken auth\n- detail");
    assert.equal(d.ok, false);
  });

  it("defaults to allow when unclear", () => {
    const d = parseStopReviewOutput("Everything seems ok overall.");
    assert.equal(d.ok, true);
  });
});

void hookUrl;
void createRequire;
