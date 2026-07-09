import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { parseArgs, splitRawArgumentString } from "../plugins/grok/scripts/lib/args.mjs";

describe("splitRawArgumentString", () => {
  it("splits on whitespace and respects quotes", () => {
    assert.deepEqual(splitRawArgumentString(`review --base main "focus text"`), [
      "review",
      "--base",
      "main",
      "focus text"
    ]);
  });
});

describe("parseArgs", () => {
  it("parses boolean and value options", () => {
    const { options, positionals } = parseArgs(
      ["--background", "--base", "main", "hello", "world"],
      {
        booleanOptions: ["background"],
        valueOptions: ["base"]
      }
    );
    assert.equal(options.background, true);
    assert.equal(options.base, "main");
    assert.deepEqual(positionals, ["hello", "world"]);
  });

  it("supports --key=value", () => {
    const { options } = parseArgs(["--model=grok-build"], {
      valueOptions: ["model"]
    });
    assert.equal(options.model, "grok-build");
  });
});
