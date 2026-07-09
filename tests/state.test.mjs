import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { describe, it, beforeEach, afterEach } from "node:test";

import {
  generateJobId,
  getConfig,
  listJobs,
  loadState,
  setConfig,
  upsertJob
} from "../plugins/grok/scripts/lib/state.mjs";
import { makeTempDir } from "./helpers.mjs";

describe("state", () => {
  /** @type {string} */
  let dir;
  /** @type {string | undefined} */
  let previousData;

  beforeEach(() => {
    dir = makeTempDir();
    fs.mkdirSync(path.join(dir, ".git"), { recursive: true });
    previousData = process.env.GROK_PLUGIN_DATA;
    process.env.GROK_PLUGIN_DATA = path.join(dir, "plugin-data");
  });

  afterEach(() => {
    if (previousData === undefined) {
      delete process.env.GROK_PLUGIN_DATA;
    } else {
      process.env.GROK_PLUGIN_DATA = previousData;
    }
    fs.rmSync(dir, { recursive: true, force: true });
  });

  it("stores config and jobs per workspace", () => {
    setConfig(dir, { stopReviewGate: true });
    assert.equal(getConfig(dir).stopReviewGate, true);

    const id = generateJobId("task");
    upsertJob(dir, {
      id,
      title: "Test job",
      status: "completed",
      kind: "task"
    });

    const jobs = listJobs(dir);
    assert.equal(jobs.length, 1);
    assert.equal(jobs[0].id, id);
    assert.equal(loadState(dir).config.stopReviewGate, true);
  });
});
