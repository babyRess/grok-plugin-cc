import fs from "node:fs";
import path from "node:path";

import { resolveJobsDir, upsertJob } from "./state.mjs";

export const SESSION_ID_ENV = "GROK_COMPANION_SESSION_ID";

export function nowIso() {
  return new Date().toISOString();
}

export function createJobRecord({
  id,
  kind,
  kindLabel,
  title,
  workspaceRoot,
  jobClass,
  summary,
  write = false
}) {
  return {
    id,
    kind,
    kindLabel: kindLabel ?? kind,
    title,
    workspaceRoot,
    jobClass,
    summary,
    write: Boolean(write),
    status: "queued",
    phase: "queued",
    progressMessage: null,
    pid: null,
    sessionId: process.env[SESSION_ID_ENV] || process.env.CLAUDE_SESSION_ID || null,
    grokSessionId: null,
    logFile: null,
    resultFile: null,
    exitCode: null,
    error: null,
    createdAt: nowIso(),
    updatedAt: nowIso(),
    startedAt: null,
    finishedAt: null
  };
}

export function createJobLogFile(workspaceRoot, jobId, title = "job") {
  const jobsDir = resolveJobsDir(workspaceRoot);
  fs.mkdirSync(jobsDir, { recursive: true });
  const safe = String(title).replace(/[^a-zA-Z0-9._-]+/g, "-").slice(0, 40) || "job";
  const logFile = path.join(jobsDir, `${jobId}-${safe}.log`);
  fs.writeFileSync(logFile, "", "utf8");
  return logFile;
}

export function appendLogLine(logFile, line) {
  if (!logFile) {
    return;
  }
  fs.appendFileSync(logFile, `${line}\n`, "utf8");
}

/**
 * @param {{ stderr?: boolean, logFile?: string | null, onEvent?: (event: object) => void }} [options]
 */
export function createProgressReporter(options = {}) {
  return (update) => {
    const message =
      typeof update === "string" ? update : update?.message ?? update?.phase ?? "";
    const phase = typeof update === "object" ? update?.phase ?? null : null;
    if (options.stderr && message) {
      process.stderr.write(`[grok] ${message}\n`);
    }
    if (options.logFile && message) {
      appendLogLine(options.logFile, `[${nowIso()}] ${message}`);
    }
    if (options.onEvent) {
      options.onEvent({
        message,
        phase,
        ...(typeof update === "object" ? update : {})
      });
    }
  };
}

export function createJobProgressUpdater(workspaceRoot, jobId) {
  return (event) => {
    upsertJob(workspaceRoot, {
      id: jobId,
      phase: event.phase ?? undefined,
      progressMessage: event.message ?? undefined
    });
  };
}

/**
 * @param {object} job
 * @param {() => Promise<object>} runner
 * @param {{ logFile?: string | null }} [options]
 */
export async function runTrackedJob(job, runner, options = {}) {
  const logFile = options.logFile ?? job.logFile;
  upsertJob(job.workspaceRoot, {
    id: job.id,
    status: "running",
    phase: "running",
    startedAt: nowIso(),
    logFile,
    pid: process.pid
  });

  try {
    const result = await runner();
    const exitStatus = result.exitStatus ?? 0;
    const resultFile = path.join(resolveJobsDir(job.workspaceRoot), `${job.id}-result.json`);
    fs.writeFileSync(
      resultFile,
      `${JSON.stringify(
        {
          jobId: job.id,
          exitStatus,
          payload: result.payload ?? null,
          rendered: result.rendered ?? "",
          threadId: result.threadId ?? null,
          grokSessionId: result.grokSessionId ?? null,
          summary: result.summary ?? null,
          finishedAt: nowIso()
        },
        null,
        2
      )}\n`,
      "utf8"
    );

    upsertJob(job.workspaceRoot, {
      id: job.id,
      status: exitStatus === 0 ? "completed" : "failed",
      phase: exitStatus === 0 ? "completed" : "failed",
      exitCode: exitStatus,
      finishedAt: nowIso(),
      resultFile,
      grokSessionId: result.grokSessionId ?? null,
      progressMessage: result.summary ?? null,
      error: exitStatus === 0 ? null : result.summary ?? "Job failed"
    });

    return {
      ...result,
      resultFile
    };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    appendLogLine(logFile, `[${nowIso()}] ERROR: ${message}`);
    upsertJob(job.workspaceRoot, {
      id: job.id,
      status: "failed",
      phase: "failed",
      exitCode: 1,
      finishedAt: nowIso(),
      error: message,
      progressMessage: message
    });
    throw error;
  }
}
