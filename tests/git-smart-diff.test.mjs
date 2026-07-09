import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { buildSmartDiff } from "../plugins/grok/scripts/lib/git.mjs";

describe("buildSmartDiff", () => {
  it("keeps small diffs", () => {
    const small = "diff --git a/a.rs b/a.rs\n+hi\n";
    assert.equal(buildSmartDiff(small, "M\ta.rs\n", 40_000), small);
  });

  it("summarizes large diffs", () => {
    let huge = "diff --git a/a.rs b/a.rs\n+line\n";
    while (huge.length < 30_000) {
      huge += "diff --git a/x.rs b/x.rs\n+more\n";
    }
    const out = buildSmartDiff(huge, "M\ta.rs\nM\tx.rs\n", 40_000);
    assert.match(out, /smart-diff/);
    assert.match(out, /Changed files/);
    assert.ok(out.length < huge.length);
  });
});
