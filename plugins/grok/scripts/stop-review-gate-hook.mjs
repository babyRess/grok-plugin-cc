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
import { SESSION_ID_ENV } from "./lib/tracked-jobs.mjs";
import { resolveWorkspaceRoot } from "./lib/workspace.mjs";

const STOP_REVIEW_TIMEOUT_MS = 15 * 60 * 1000;
const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT_DIR = path.resolve(SCRIPT_DIR, "..");
const COMPANION = path.join(SCRIPT_DIR, "resolve-companion.mjs");
const STOP_REVIEW_TASK_MARKER = "Run a stop-gate review of the previous Claude turn.";
const GATE_MARKER_FILE = ".stop-gate-last.json";

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

function gateStatePath(workspaceRoot) {
  // Prefer Grok-owned plugin data; ignore foreign CLAUDE_PLUGIN_DATA (e.g. codex).
  const data =
    process.env.GROK_PLUGIN_DATA ||
    (process.env.CLAUDE_PLUGIN_DATA &&
    String(process.env.CLAUDE_PLUGIN_DATA).toLowerCase().includes("grok")
      ? process.env.CLAUDE_PLUGIN_DATA
      : null);
  if (data) {
    return path.join(data, GATE_MARKER_FILE);
  }
  return path.join(workspaceRoot, ".grok-companion-stop-gate.json");
}

function readGateState(workspaceRoot) {
  const p = gateStatePath(workspaceRoot);
  try {
    if (!fs.existsSync(p)) {
      return null;
    }
    return JSON.parse(fs.readFileSync(p, "utf8"));
  } catch {
    return null;
  }
}

function writeGateState(workspaceRoot, state) {
  const p = gateStatePath(workspaceRoot);
  try {
    fs.mkdirSync(path.dirname(p), { recursive: true });
    fs.writeFileSync(p, `${JSON.stringify(state, null, 2)}\n`, "utf8");
  } catch (e) {
    logNote(`stop-gate: failed to persist gate state: ${e.message}`);
  }
}

function hashMessage(text) {
  // Lightweight stable hash — enough to detect same Claude turn vs new work.
  let h = 0;
  const s = String(text || "");
  for (let i = 0; i < s.length; i += 1) {
    h = (Math.imul(31, h) + s.charCodeAt(i)) | 0;
  }
  return String(h);
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

export function parseStopReviewOutput(rawOutput) {
  const text = String(rawOutput ?? "").trim();
  if (!text) {
    return {
      ok: false,
      reason:
        "The stop-time Grok review returned no final output. Run /grok:review --wait manually or bypass the gate."
    };
  }

  // Prefer first ALLOW:/BLOCK: line anywhere in first 20 lines
  const lines = text.split(/\r?\n/).slice(0, 20);
  for (const line of lines) {
    const t = line.trim();
    if (t.startsWith("ALLOW:")) {
      return { ok: true, reason: t.slice("ALLOW:".length).trim() || "allowed" };
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

  const active = filterJobsForCurrentSession(listJobs(workspaceRoot), input).filter(
    (j) => j.status === "running" || j.status === "queued"
  );
  if (active.length > 0) {
    logNote("Skipping stop review gate because a Grok job is already active.");
    return;
  }

  const msgHash = hashMessage(input.last_assistant_message);
  const prior = readGateState(workspaceRoot);

  // Only skip re-entry when we already ALLOWed the *same* Claude message.
  // A prior BLOCK for this message must re-run after Claude tries to fix.
  // A completed stop-gate job for a *different* message must still run.
  if (prior?.decision === "ALLOW" && prior?.messageHash === msgHash) {
    logNote("Stop review gate: already ALLOW for this Claude turn; skipping.");
    return;
  }

  // Prevent infinite BLOCK loops: if we blocked this exact message twice, allow with note.
  if (prior?.decision === "BLOCK" && prior?.messageHash === msgHash && (prior.blockCount || 0) >= 2) {
    logNote(
      "Stop review gate: already BLOCKed this Claude turn twice — allowing stop to avoid infinite loop. Re-enable after addressing issues."
    );
    writeGateState(workspaceRoot, {
      decision: "ALLOW",
      messageHash: msgHash,
      reason: "max block count for same message",
      at: new Date().toISOString()
    });
    return;
  }

  const prompt = buildStopReviewPrompt(input);
  const result = spawnSync(
    process.execPath,
    [
      COMPANION,
      "task",
      "--read-only",
      "--no-inherit-claude-context",
      "--",
      prompt
    ],
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
    writeGateState(workspaceRoot, {
      decision: "ALLOW",
      messageHash: msgHash,
      reason: decision.reason,
      at: new Date().toISOString()
    });
    return;
  }

  const blockCount =
    prior?.decision === "BLOCK" && prior?.messageHash === msgHash
      ? (prior.blockCount || 0) + 1
      : 1;

  logNote(`Stop review gate: BLOCK (count=${blockCount})`);
  writeGateState(workspaceRoot, {
    decision: "BLOCK",
    messageHash: msgHash,
    reason: decision.reason,
    blockCount,
    at: new Date().toISOString()
  });

  emitDecision({
    decision: "block",
    reason: decision.reason || output || "Grok stop-gate review found issues."
  });
}

main();
