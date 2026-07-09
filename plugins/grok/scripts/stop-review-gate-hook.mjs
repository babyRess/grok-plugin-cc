#!/usr/bin/env node

/**
 * Optional Stop hook: when stopReviewGate is enabled, run a short Grok review
 * of Claude's last turn. If Grok replies with BLOCK:, deny the stop.
 *
 * Warning: this can create long Claude↔Grok loops and burn usage.
 */

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

import { getGrokLoginStatus } from "./lib/grok.mjs";
import { loadPromptTemplate, interpolateTemplate } from "./lib/prompts.mjs";
import { getConfig, listJobs } from "./lib/state.mjs";
import { sortJobsNewestFirst } from "./lib/job-control.mjs";
import { SESSION_ID_ENV } from "./lib/tracked-jobs.mjs";
import { resolveWorkspaceRoot } from "./lib/workspace.mjs";

const STOP_REVIEW_TIMEOUT_MS = 15 * 60 * 1000;
const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT_DIR = path.resolve(SCRIPT_DIR, "..");
const COMPANION = path.join(SCRIPT_DIR, "grok-companion.mjs");
const STOP_REVIEW_TASK_MARKER = "Run a stop-gate review of the previous Claude turn.";

function readHookInput() {
  const raw = fs.readFileSync(0, "utf8").trim();
  if (!raw) {
    return {};
  }
  try {
    return JSON.parse(raw);
  } catch {
    return {};
  }
}

function emitDecision(payload) {
  process.stdout.write(`${JSON.stringify(payload)}\n`);
}

function logNote(message) {
  if (!message) {
    return;
  }
  process.stderr.write(`${message}\n`);
}

function filterJobsForCurrentSession(jobs, input = {}) {
  const sessionId = input.session_id || process.env[SESSION_ID_ENV] || null;
  if (!sessionId) {
    return jobs;
  }
  return jobs.filter((job) => job.sessionId === sessionId);
}

function buildStopReviewPrompt(input = {}) {
  const lastAssistantMessage = String(input.last_assistant_message ?? "").trim();
  let template = "";
  try {
    template = loadPromptTemplate(ROOT_DIR, "stop-review-gate");
  } catch {
    template = `${STOP_REVIEW_TASK_MARKER}

{{CLAUDE_RESPONSE_BLOCK}}

Decide whether Claude's last turn is safe to accept.
Reply with exactly one of:
ALLOW: <one-line reason>
BLOCK: <concrete issues Claude must fix before stopping>
`;
  }
  const claudeResponseBlock = lastAssistantMessage
    ? ["Previous Claude response:", lastAssistantMessage].join("\n")
    : "Previous Claude response: (empty)";
  return interpolateTemplate(template, {
    CLAUDE_RESPONSE_BLOCK: claudeResponseBlock
  });
}

function parseStopReviewOutput(rawOutput) {
  const text = String(rawOutput ?? "").trim();
  if (!text) {
    return {
      ok: false,
      reason:
        "The stop-time Grok review returned no final output. Run /grok:review --wait manually or bypass the gate."
    };
  }

  const firstLine = text.split(/\r?\n/, 1)[0].trim();
  if (firstLine.startsWith("ALLOW:")) {
    return { ok: true, reason: firstLine.slice("ALLOW:".length).trim() || "allowed" };
  }
  if (firstLine.startsWith("BLOCK:")) {
    return { ok: false, reason: text };
  }

  // Heuristic: if the model forgot the prefix, look for block language
  if (/\bBLOCK\b/i.test(text) && !/\bALLOW\b/i.test(text.split(/\r?\n/, 1)[0])) {
    return { ok: false, reason: text };
  }

  return { ok: true, reason: "No explicit block; allowing stop." };
}

function main() {
  const input = readHookInput();
  const cwd = input.cwd || process.cwd();
  const workspaceRoot = resolveWorkspaceRoot(cwd);
  const config = getConfig(workspaceRoot);

  if (!config.stopReviewGate) {
    return;
  }

  const authStatus = getGrokLoginStatus(cwd);
  if (!authStatus.available) {
    logNote(`Grok is not set up for the review gate. ${authStatus.detail}. Run /grok:setup.`);
    return;
  }

  // Skip if a companion job is already running in this session
  const active = filterJobsForCurrentSession(listJobs(workspaceRoot), input).filter(
    (j) => j.status === "running" || j.status === "queued"
  );
  if (active.length > 0) {
    logNote("Skipping stop review gate because a Grok job is already active.");
    return;
  }

  // Avoid re-entry loops: if the last job was already a stop-gate, allow stop
  const recent = sortJobsNewestFirst(listJobs(workspaceRoot))[0];
  if (recent?.title === "Grok Stop Gate Review" && recent.status === "completed") {
    return;
  }

  const prompt = buildStopReviewPrompt(input);
  const result = spawnSync(
    process.execPath,
    [COMPANION, "task", "--read-only", "--", prompt],
    {
      cwd: workspaceRoot,
      encoding: "utf8",
      timeout: STOP_REVIEW_TIMEOUT_MS,
      env: process.env,
      windowsHide: true
    }
  );

  const output = `${result.stdout ?? ""}\n${result.stderr ?? ""}`;
  const decision = parseStopReviewOutput(result.stdout ?? "");

  if (decision.ok) {
    logNote(`Stop review gate: ALLOW (${decision.reason})`);
    return;
  }

  logNote("Stop review gate: BLOCK");
  // Claude Code Stop hook: return decision to block
  emitDecision({
    decision: "block",
    reason: decision.reason || output || "Grok stop-gate review found issues."
  });
}

main();
