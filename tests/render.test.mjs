import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  renderSetupReport,
  renderStatusReport,
  renderTaskResult
} from "../plugins/grok/scripts/lib/render.mjs";

describe("render", () => {
  it("renders setup report", () => {
    const text = renderSetupReport({
      ready: true,
      node: { detail: "v22" },
      grok: { detail: "grok 0.2" },
      auth: { loggedIn: true, detail: "ok" },
      config: { stopReviewGate: false },
      nextSteps: ["Optional step"]
    });
    assert.match(text, /Ready: yes/);
    assert.match(text, /Optional step/);
  });

  it("renders empty status", () => {
    assert.match(renderStatusReport({ jobs: [] }), /No Grok companion jobs/);
  });

  it("renders task result", () => {
    const text = renderTaskResult(
      { rawOutput: "done" },
      { title: "Grok Task", jobId: "task-1", write: true }
    );
    assert.match(text, /Grok Task/);
    assert.match(text, /write-capable/);
    assert.match(text, /done/);
  });
});
