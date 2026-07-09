/**
 * Grok CLI runtime helpers (headless `grok -p` backend).
 */
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import process from "node:process";

import { binaryAvailable, runCommand } from "./process.mjs";

const DEFAULT_MODEL = process.env.GROK_COMPANION_MODEL || null;
const VALID_EFFORTS = new Set(["none", "minimal", "low", "medium", "high", "xhigh", "max"]);

/**
 * Resolve the grok binary: PATH first, then common install locations.
 */
export function resolveGrokBinary() {
  const fromEnv = process.env.GROK_BIN;
  if (fromEnv && fs.existsSync(fromEnv)) {
    return fromEnv;
  }

  const which = runCommand(process.platform === "win32" ? "where" : "which", ["grok"]);
  if (which.status === 0) {
    const candidate = which.stdout.trim().split(/\r?\n/)[0];
    if (candidate) {
      return candidate;
    }
  }

  const home = os.homedir();
  const fallbacks = [
    path.join(home, ".grok", "bin", "grok"),
    path.join(home, ".local", "bin", "grok"),
    "/opt/homebrew/bin/grok",
    "/usr/local/bin/grok"
  ];
  for (const candidate of fallbacks) {
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }
  return "grok";
}

export function getGrokAvailability(cwd) {
  const bin = resolveGrokBinary();
  return binaryAvailable(bin, ["--version"], { cwd });
}

/**
 * Best-effort auth check: prefer auth.json presence; fall back to doctor-like probe.
 */
export function getGrokLoginStatus(cwd) {
  const availability = getGrokAvailability(cwd);
  if (!availability.available) {
    return {
      available: false,
      loggedIn: false,
      detail: availability.detail
    };
  }

  const authPath = path.join(os.homedir(), ".grok", "auth.json");
  if (fs.existsSync(authPath)) {
    try {
      const raw = fs.readFileSync(authPath, "utf8");
      const parsed = JSON.parse(raw);
      // Heuristic: any non-empty object with tokens/keys counts as logged in
      const hasCreds =
        Boolean(parsed?.access_token) ||
        Boolean(parsed?.token) ||
        Boolean(parsed?.api_key) ||
        Boolean(parsed?.apiKey) ||
        Boolean(parsed?.oauth) ||
        Object.keys(parsed || {}).length > 0;
      if (hasCreds) {
        return {
          available: true,
          loggedIn: true,
          detail: `credentials present at ${authPath}`
        };
      }
    } catch {
      // fall through
    }
  }

  // Soft fail open: binary exists but we couldn't confirm auth
  return {
    available: true,
    loggedIn: false,
    detail: `No usable credentials found at ${authPath}. Run \`grok\` and complete login, or set API credentials.`
  };
}

export function normalizeEffort(effort) {
  if (effort == null || effort === "") {
    return null;
  }
  const normalized = String(effort).trim().toLowerCase();
  if (!VALID_EFFORTS.has(normalized)) {
    throw new Error(
      `Unsupported reasoning effort "${effort}". Use one of: ${[...VALID_EFFORTS].join(", ")}.`
    );
  }
  return normalized === "max" ? "xhigh" : normalized;
}

/**
 * @param {string} cwd
 * @param {{
 *   prompt: string,
 *   model?: string | null,
 *   effort?: string | null,
 *   write?: boolean,
 *   resumeSessionId?: string | null,
 *   continueLatest?: boolean,
 *   maxTurns?: number | null,
 *   onProgress?: (msg: string | object) => void,
 *   env?: NodeJS.ProcessEnv
 * }} options
 * @returns {Promise<{
 *   status: number,
 *   stdout: string,
 *   stderr: string,
 *   grokSessionId: string | null,
 *   finalMessage: string
 * }>}
 */
export function runGrokHeadless(cwd, options) {
  const bin = resolveGrokBinary();
  const prompt = String(options.prompt ?? "").trim();
  if (!prompt) {
    return Promise.reject(new Error("A prompt is required for this Grok run."));
  }

  /** @type {string[]} */
  // --always-approve is required headless: without it Grok can hang forever
  // waiting for interactive tool permission while stdin is closed.
  const args = [
    "-p",
    prompt,
    "--output-format",
    "plain",
    "--cwd",
    cwd,
    "--always-approve"
  ];

  if (options.model || DEFAULT_MODEL) {
    args.push("-m", options.model || DEFAULT_MODEL);
  }
  if (options.effort) {
    args.push("--effort", options.effort);
  }
  if (options.maxTurns) {
    args.push("--max-turns", String(options.maxTurns));
  }
  if (options.resumeSessionId) {
    args.push("-r", options.resumeSessionId);
  } else if (options.continueLatest) {
    args.push("-c");
  }

  if (options.write) {
    // Auto-approve tools for unattended rescue work
    args.push("--yolo");
  } else {
    // Read-oriented: still allow shell for git inspection, but deny file writes
    args.push(
      "--disallowed-tools",
      "search_replace,Write,Edit"
    );
    // Prefer not to spawn nested agents during review
    args.push("--no-subagents");
  }

  const onProgress = options.onProgress;
  if (onProgress) {
    onProgress({
      message: `Spawning ${bin} -p … (always-approve, prompt ~${prompt.length} chars)`,
      phase: "starting"
    });
  }

  return new Promise((resolve) => {
    const child = spawn(bin, args, {
      cwd,
      env: options.env ?? process.env,
      stdio: ["ignore", "pipe", "pipe"],
      shell: process.platform === "win32",
      windowsHide: true
    });

    let stdout = "";
    let stderr = "";

    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
      const line = String(chunk).trim();
      if (line && onProgress) {
        onProgress({ message: line.split(/\r?\n/).pop(), phase: "running" });
      }
    });

    child.on("error", (error) => {
      resolve({
        status: 1,
        stdout,
        stderr: error.message,
        grokSessionId: null,
        finalMessage: ""
      });
    });

    child.on("close", (code) => {
      const finalMessage = stdout.trim();
      if (onProgress) {
        onProgress({
          message: code === 0 ? "Grok finished." : `Grok exited with code ${code}.`,
          phase: code === 0 ? "completed" : "failed"
        });
      }
      resolve({
        status: code ?? 1,
        stdout,
        stderr,
        grokSessionId: null,
        finalMessage
      });
    });
  });
}

/**
 * Build a review prompt from collected git context.
 */
export function buildReviewPrompt(context, { adversarial = false, focusText = "" } = {}) {
  const mode = adversarial
    ? "You are performing an adversarial code review. Challenge design choices, hidden assumptions, failure modes, race conditions, and safer alternatives. Be specific and steerable."
    : "You are performing a careful code review. Focus on correctness bugs, security issues, regressions, missing tests, and maintainability problems.";

  const focus = focusText.trim()
    ? `\n\nExtra review focus from the user:\n${focusText.trim()}\n`
    : "";

  return `${mode}

Repository: ${context.repoRoot}
Branch: ${context.branch}
Review target: ${context.target.label}
Change summary: ${context.summary}

Git status / recent commits:
\`\`\`
${context.status || "(empty)"}
\`\`\`

Diff:
\`\`\`diff
${context.diff || "(no textual diff; check untracked files if status is non-empty)"}
\`\`\`
${focus}
Instructions:
- Do NOT modify files.
- Do NOT apply patches.
- Report findings ordered by severity (critical / high / medium / low).
- For each finding include: title, severity, file path if known, why it matters, and a concrete fix suggestion.
- If there are no material issues, say so clearly and mention residual risks.
- End with a short summary.
`;
}

/**
 * Build a stop-gate review prompt.
 */
export function buildStopGatePrompt(lastAssistantMessage, templateBody) {
  const block = lastAssistantMessage
    ? `Previous Claude response:\n${lastAssistantMessage}`
    : "Previous Claude response: (empty)";
  if (templateBody) {
    return String(templateBody).replace(/\{\{CLAUDE_RESPONSE_BLOCK\}\}/g, block);
  }
  return `Run a stop-gate review of the previous Claude turn.

${block}

Decide whether Claude's last turn is safe to accept.
Reply with exactly one of:
ALLOW: <one-line reason>
BLOCK: <concrete issues Claude must fix before stopping>
`;
}
