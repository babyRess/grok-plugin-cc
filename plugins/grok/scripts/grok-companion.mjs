#!/usr/bin/env node

import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { parseArgs, splitRawArgumentString } from "./lib/args.mjs";
import { maybeInjectClaudeContext } from "./lib/claude-context.mjs";
import { readStdinIfPiped } from "./lib/fs.mjs";
import { collectReviewContext, ensureGitRepository, resolveReviewTarget } from "./lib/git.mjs";
import {
  buildReviewPrompt,
  getGrokAvailability,
  getGrokLoginStatus,
  normalizeEffort,
  runGrokHeadless
} from "./lib/grok.mjs";
import {
  buildSingleJobSnapshot,
  buildStatusSnapshot,
  readStoredJob,
  resolveCancelableJob,
  resolveResultJob,
  sortJobsNewestFirst
} from "./lib/job-control.mjs";
import { binaryAvailable, terminateProcessTree } from "./lib/process.mjs";
import { interpolateTemplate, loadPromptTemplate } from "./lib/prompts.mjs";
import {
  renderCancelReport,
  renderJobStatusReport,
  renderReviewResult,
  renderSetupReport,
  renderStatusReport,
  renderStoredJobResult,
  renderTaskResult
} from "./lib/render.mjs";
import {
  generateJobId,
  getConfig,
  listJobs,
  resolveJobsDir,
  setConfig,
  upsertJob
} from "./lib/state.mjs";
import {
  createJobLogFile,
  createJobProgressUpdater,
  createJobRecord,
  createProgressReporter,
  nowIso,
  runTrackedJob
} from "./lib/tracked-jobs.mjs";
import { buildTransferPrompt, findLatestClaudeSession } from "./lib/transfer.mjs";
import { resolveWorkspaceRoot } from "./lib/workspace.mjs";

const ROOT_DIR = path.resolve(fileURLToPath(new URL("..", import.meta.url)));
const SCRIPT_PATH = fileURLToPath(import.meta.url);
const STOP_REVIEW_TASK_MARKER = "Run a stop-gate review of the previous Claude turn.";

function printUsage() {
  console.log(
    [
      "Usage:",
      "  node scripts/grok-companion.mjs setup [--enable-review-gate|--disable-review-gate] [--json]",
      "  node scripts/grok-companion.mjs review [--wait|--background] [--base <ref>] [--scope <auto|working-tree|branch>] [--model <m>] [--effort <e>] [--no-inherit-claude-context]",
      "  node scripts/grok-companion.mjs adversarial-review [...] [--no-inherit-claude-context] [focus text]",
      "  node scripts/grok-companion.mjs task [--background] [--write|--read-only] [--resume-last|--fresh] [--model <m>] [--effort <e>] [--no-inherit-claude-context] [prompt]",
      "  node scripts/grok-companion.mjs status [job-id] [--all] [--json]",
      "  node scripts/grok-companion.mjs result [job-id] [--json]",
      "  node scripts/grok-companion.mjs cancel [job-id] [--json]",
      "  node scripts/grok-companion.mjs task-resume-candidate [--json]",
      "  node scripts/grok-companion.mjs transfer [--source <session.jsonl>] [--read-only] [--background]"
    ].join("\n")
  );
}

function outputResult(value, asJson) {
  if (asJson) {
    console.log(JSON.stringify(value, null, 2));
  } else {
    process.stdout.write(typeof value === "string" ? value : `${JSON.stringify(value, null, 2)}\n`);
  }
}

function normalizeArgv(argv) {
  if (argv.length === 1) {
    const [raw] = argv;
    if (!raw || !raw.trim()) {
      return [];
    }
    return splitRawArgumentString(raw);
  }
  return argv;
}

function parseCommandInput(argv, config = {}) {
  return parseArgs(normalizeArgv(argv), {
    ...config,
    aliasMap: {
      C: "cwd",
      ...(config.aliasMap ?? {})
    }
  });
}

function resolveCommandCwd(options = {}) {
  return options.cwd ? path.resolve(process.cwd(), options.cwd) : process.cwd();
}

function firstMeaningfulLine(text, fallback) {
  const line = String(text ?? "")
    .split(/\r?\n/)
    .map((value) => value.trim())
    .find(Boolean);
  return line ?? fallback;
}

function shorten(text, limit = 96) {
  const normalized = String(text ?? "")
    .trim()
    .replace(/\s+/g, " ");
  if (!normalized) {
    return "";
  }
  if (normalized.length <= limit) {
    return normalized;
  }
  return `${normalized.slice(0, limit - 3)}...`;
}

function ensureGrokReady(cwd) {
  const availability = getGrokAvailability(cwd);
  if (!availability.available) {
    throw new Error(
      `Grok CLI is not available (${availability.detail}). Install Grok Build, ensure \`grok\` is on PATH, then run /grok:setup.`
    );
  }
  const auth = getGrokLoginStatus(cwd);
  if (!auth.loggedIn) {
    process.stderr.write(`[grok] Warning: ${auth.detail}\n`);
  }
}

function buildSetupReport(cwd, actionsTaken = []) {
  const workspaceRoot = resolveWorkspaceRoot(cwd);
  const nodeStatus = binaryAvailable("node", ["--version"], { cwd });
  const grokStatus = getGrokAvailability(cwd);
  const authStatus = getGrokLoginStatus(cwd);
  const config = getConfig(workspaceRoot);

  const nextSteps = [];
  if (!grokStatus.available) {
    nextSteps.push("Install Grok Build and ensure `grok` is on your PATH (or set GROK_BIN).");
  }
  if (grokStatus.available && !authStatus.loggedIn) {
    nextSteps.push("Authenticate Grok (open `grok` interactively or configure API credentials).");
  }
  if (!config.stopReviewGate) {
    nextSteps.push("Optional: run `/grok:setup --enable-review-gate` to require a Grok review before stop.");
  }

  return {
    ready: nodeStatus.available && grokStatus.available,
    node: nodeStatus,
    grok: grokStatus,
    auth: authStatus,
    config,
    actionsTaken,
    nextSteps,
    workspaceRoot
  };
}

function handleSetup(argv) {
  const { options } = parseCommandInput(argv, {
    booleanOptions: ["json", "enable-review-gate", "disable-review-gate"]
  });
  const cwd = resolveCommandCwd(options);
  const workspaceRoot = resolveWorkspaceRoot(cwd);
  const actionsTaken = [];

  if (options["enable-review-gate"] && options["disable-review-gate"]) {
    throw new Error("Pass only one of --enable-review-gate or --disable-review-gate.");
  }
  if (options["enable-review-gate"]) {
    setConfig(workspaceRoot, { stopReviewGate: true });
    actionsTaken.push("Enabled stop review gate.");
  }
  if (options["disable-review-gate"]) {
    setConfig(workspaceRoot, { stopReviewGate: false });
    actionsTaken.push("Disabled stop review gate.");
  }

  const report = buildSetupReport(cwd, actionsTaken);
  outputResult(options.json ? report : renderSetupReport(report), Boolean(options.json));
  process.exitCode = report.ready ? 0 : 1;
}

function writeWorkerRequest(workspaceRoot, jobId, payload) {
  const requestFile = path.join(resolveJobsDir(workspaceRoot), `${jobId}-request.json`);
  fs.mkdirSync(path.dirname(requestFile), { recursive: true });
  fs.writeFileSync(requestFile, `${JSON.stringify(payload, null, 2)}\n`, "utf8");
  return requestFile;
}

function createCompanionJob({ prefix, kind, title, workspaceRoot, jobClass, summary, write = false }) {
  return createJobRecord({
    id: generateJobId(prefix),
    kind,
    kindLabel:
      kind === "adversarial-review" ? "adversarial-review" : jobClass === "review" ? "review" : "rescue",
    title,
    workspaceRoot,
    jobClass,
    summary,
    write
  });
}

function createTrackedProgress(job, options = {}) {
  const logFile = options.logFile ?? createJobLogFile(job.workspaceRoot, job.id, job.title);
  return {
    logFile,
    progress: createProgressReporter({
      stderr: Boolean(options.stderr),
      logFile,
      onEvent: createJobProgressUpdater(job.workspaceRoot, job.id)
    })
  };
}

async function runForegroundCommand(job, runner, options = {}) {
  const { logFile, progress } = createTrackedProgress(job, {
    logFile: options.logFile,
    stderr: !options.json
  });
  const execution = await runTrackedJob(job, () => runner(progress), { logFile });
  outputResult(options.json ? execution.payload : execution.rendered, Boolean(options.json));
  process.exitCode = execution.exitStatus === 0 ? 0 : 1;
  return execution;
}

function spawnDetachedTaskWorker(cwd, jobId) {
  const child = spawn(process.execPath, [SCRIPT_PATH, "task-worker", "--job-id", jobId, "--cwd", cwd], {
    cwd,
    detached: true,
    stdio: "ignore",
    env: process.env,
    windowsHide: true
  });
  child.unref();
  return child;
}

async function executeReviewRun(request) {
  ensureGrokReady(request.cwd);
  ensureGitRepository(request.cwd);

  const target = resolveReviewTarget(request.cwd, {
    base: request.base,
    scope: request.scope
  });
  const context = collectReviewContext(request.cwd, target);
  const focusText = request.focusText?.trim() ?? "";
  const adversarial = Boolean(request.adversarial);
  const reviewName = adversarial ? "Adversarial Review" : "Review";

  if (target.empty && !focusText) {
    const message = `Nothing to review for ${target.label}.`;
    return {
      exitStatus: 0,
      payload: { review: reviewName, target, empty: true, message },
      rendered: `${message}\n`,
      summary: message,
      jobTitle: `Grok ${reviewName}`,
      jobClass: "review",
      targetLabel: target.label
    };
  }

  let prompt;
  if (adversarial) {
    try {
      const template = loadPromptTemplate(ROOT_DIR, "adversarial-review");
      prompt = interpolateTemplate(template, {
        BRANCH: context.branch,
        TARGET_LABEL: context.target.label,
        SUMMARY: context.summary,
        STATUS: context.status || "(empty)",
        DIFF: context.diff || "(no textual diff)",
        FOCUS_TEXT: focusText || "(none)"
      });
    } catch {
      prompt = buildReviewPrompt(context, { adversarial: true, focusText });
    }
  } else {
    prompt = buildReviewPrompt(context, { adversarial: false, focusText });
  }

  const inheritClaudeContext = request.inheritClaudeContext !== false;
  const detail = request.inheritClaudeContextFull ? "full" : "compact";
  prompt = maybeInjectClaudeContext(prompt, request.cwd, inheritClaudeContext, detail);

  const result = await runGrokHeadless(context.repoRoot, {
    prompt,
    model: request.model,
    effort: request.effort,
    write: false,
    onProgress: request.onProgress
  });

  const payload = {
    review: reviewName,
    target,
    grok: {
      status: result.status,
      stderr: result.stderr,
      stdout: result.finalMessage
    }
  };

  return {
    exitStatus: result.status,
    grokSessionId: result.grokSessionId,
    payload,
    rendered: renderReviewResult(
      { status: result.status, stdout: result.finalMessage, failureMessage: result.stderr },
      { reviewLabel: reviewName, targetLabel: target.label }
    ),
    summary: firstMeaningfulLine(result.finalMessage, `${reviewName} completed.`),
    jobTitle: `Grok ${reviewName}`,
    jobClass: "review",
    targetLabel: target.label
  };
}

async function executeTaskRun(request) {
  const workspaceRoot = resolveWorkspaceRoot(request.cwd);
  ensureGrokReady(request.cwd);

  let prompt = request.prompt?.trim() ?? "";
  let continueLatest = false;

  if (request.resumeLast) {
    continueLatest = true;
    if (!prompt) {
      prompt =
        "Continue from the current session state. Pick the next highest-value step and follow through until the task is resolved.";
    }
  }

  if (!prompt) {
    throw new Error("Provide a prompt, a prompt file, piped stdin, or use --resume-last.");
  }

  const inheritClaudeContext = request.inheritClaudeContext !== false;
  const detail = request.inheritClaudeContextFull ? "full" : "compact";
  prompt = maybeInjectClaudeContext(prompt, request.cwd, inheritClaudeContext, detail);

  const write = Boolean(request.write);

  // Prefer last known Grok session when resuming.
  let resumeSessionId = null;
  if (continueLatest) {
    const jobs = sortJobsNewestFirst(listJobs(workspaceRoot));
    const prior = jobs.find(
      (j) =>
        (j.jobClass === "task" || j.kind === "task") &&
        j.grokSessionId
    );
    resumeSessionId = prior?.grokSessionId ?? null;
  }

  const result = await runGrokHeadless(workspaceRoot, {
    prompt,
    model: request.model,
    effort: request.effort,
    write,
    continueLatest: continueLatest && !resumeSessionId,
    resumeSessionId,
    onProgress: request.onProgress
  });

  const rawOutput = result.finalMessage || "";
  const failureMessage = result.status === 0 ? "" : result.stderr || "Grok task failed.";
  const isStopGate = String(prompt).includes(STOP_REVIEW_TASK_MARKER);
  const title = isStopGate ? "Grok Stop Gate Review" : request.resumeLast ? "Grok Resume" : "Grok Task";

  let rendered = renderTaskResult(
    { rawOutput, failureMessage },
    { title, jobId: request.jobId ?? null, write }
  );
  if (result.grokSessionId) {
    rendered += `\n\nGrok session: \`${result.grokSessionId}\`\nResume: \`grok -r ${result.grokSessionId}\` or \`/grok:rescue --resume\`\n`;
  }

  return {
    exitStatus: result.status,
    grokSessionId: result.grokSessionId,
    payload: {
      status: result.status,
      rawOutput,
      stderr: result.stderr
    },
    rendered,
    summary: firstMeaningfulLine(rawOutput, firstMeaningfulLine(failureMessage, `${title} finished.`)),
    jobTitle: title,
    jobClass: "task",
    write
  };
}

async function handleReviewCommand(argv, { adversarial = false } = {}) {
  const { options, positionals } = parseCommandInput(argv, {
    booleanOptions: [
      "wait",
      "background",
      "json",
      "no-inherit-claude-context",
      "inherit-claude-context-full"
    ],
    valueOptions: ["base", "scope", "model", "effort", "cwd"]
  });
  const cwd = resolveCommandCwd(options);
  const workspaceRoot = resolveWorkspaceRoot(cwd);
  const focusText = positionals.join(" ").trim();
  const effort = options.effort ? normalizeEffort(options.effort) : null;
  const reviewName = adversarial ? "Adversarial Review" : "Review";
  const kind = adversarial ? "adversarial-review" : "review";
  const inheritClaudeContext = !options["no-inherit-claude-context"];
  const inheritClaudeContextFull = Boolean(options["inherit-claude-context-full"]);

  const target = resolveReviewTarget(cwd, {
    base: options.base ?? null,
    scope: options.scope ?? "auto"
  });

  const job = createCompanionJob({
    prefix: adversarial ? "adv" : "rev",
    kind,
    title: `Grok ${reviewName}`,
    workspaceRoot,
    jobClass: "review",
    summary: `${reviewName} ${target.label}${focusText ? `: ${shorten(focusText)}` : ""}`,
    write: false
  });
  upsertJob(workspaceRoot, job);

  const request = {
    cwd,
    base: options.base ?? null,
    scope: options.scope ?? "auto",
    model: options.model ?? null,
    effort,
    focusText,
    adversarial,
    inheritClaudeContext,
    inheritClaudeContextFull
  };

  if (options.background) {
    writeWorkerRequest(workspaceRoot, job.id, { type: "review", request });
    const child = spawnDetachedTaskWorker(cwd, job.id);
    upsertJob(workspaceRoot, { id: job.id, pid: child.pid, status: "queued", phase: "queued" });
    const message = `${job.title} started in the background as ${job.id}. Check /grok:status ${job.id} for progress.\n`;
    outputResult(
      options.json ? { jobId: job.id, status: "queued", title: job.title } : message,
      Boolean(options.json)
    );
    return;
  }

  await runForegroundCommand(
    job,
    async (progress) => executeReviewRun({ ...request, onProgress: progress }),
    { json: options.json }
  );
}

async function handleTask(argv) {
  const { options, positionals } = parseCommandInput(argv, {
    booleanOptions: [
      "background",
      "write",
      "read-only",
      "resume-last",
      "resume",
      "fresh",
      "json",
      "no-inherit-claude-context",
      "inherit-claude-context-full"
    ],
    valueOptions: ["model", "effort", "cwd", "prompt-file"]
  });
  const cwd = resolveCommandCwd(options);
  const workspaceRoot = resolveWorkspaceRoot(cwd);

  let prompt = "";
  if (options["prompt-file"]) {
    prompt = fs.readFileSync(path.resolve(cwd, options["prompt-file"]), "utf8");
  } else {
    prompt = positionals.join(" ") || readStdinIfPiped();
  }

  const resumeLast = Boolean(options["resume-last"] || options.resume) && !options.fresh;
  // Default write-capable for task/rescue; --read-only forces read-only
  const writeMode = options["read-only"] ? false : true;
  const inheritClaudeContext = !options["no-inherit-claude-context"];
  const inheritClaudeContextFull = Boolean(options["inherit-claude-context-full"]);

  if (!prompt.trim() && !resumeLast) {
    throw new Error("Provide a prompt, a prompt file, piped stdin, or use --resume-last.");
  }

  const effort = options.effort ? normalizeEffort(options.effort) : null;
  const job = createCompanionJob({
    prefix: "task",
    kind: "task",
    title: resumeLast ? "Grok Resume" : "Grok Task",
    workspaceRoot,
    jobClass: "task",
    summary: shorten(prompt || "resume"),
    write: writeMode
  });
  upsertJob(workspaceRoot, job);

  const request = {
    cwd,
    model: options.model ?? null,
    effort,
    prompt: prompt.trim(),
    write: writeMode,
    resumeLast,
    jobId: job.id,
    inheritClaudeContext,
    inheritClaudeContextFull
  };

  if (options.background) {
    writeWorkerRequest(workspaceRoot, job.id, { type: "task", request });
    const child = spawnDetachedTaskWorker(cwd, job.id);
    upsertJob(workspaceRoot, { id: job.id, pid: child.pid, status: "queued", phase: "queued" });
    const message = `${job.title} started in the background as ${job.id}. Check /grok:status ${job.id} for progress.\n`;
    outputResult(
      options.json ? { jobId: job.id, status: "queued", title: job.title } : message,
      Boolean(options.json)
    );
    return;
  }

  await runForegroundCommand(
    job,
    async (progress) => executeTaskRun({ ...request, onProgress: progress }),
    { json: options.json }
  );
}

async function handleTaskWorker(argv) {
  const { options } = parseCommandInput(argv, {
    valueOptions: ["job-id", "cwd"]
  });
  if (!options["job-id"]) {
    throw new Error("task-worker requires --job-id");
  }
  const cwd = resolveCommandCwd(options);
  const workspaceRoot = resolveWorkspaceRoot(cwd);
  const jobId = options["job-id"];
  const requestFile = path.join(resolveJobsDir(workspaceRoot), `${jobId}-request.json`);
  if (!fs.existsSync(requestFile)) {
    throw new Error(`Missing worker request file: ${requestFile}`);
  }
  const payload = JSON.parse(fs.readFileSync(requestFile, "utf8"));
  const jobs = listJobs(workspaceRoot);
  const existing = jobs.find((j) => j.id === jobId);
  const job =
    existing ??
    createCompanionJob({
      prefix: "task",
      kind: payload.type === "review" ? "review" : "task",
      title: "Grok Worker",
      workspaceRoot,
      jobClass: payload.type === "review" ? "review" : "task",
      summary: "background worker",
      write: Boolean(payload.request?.write)
    });
  job.id = jobId;

  await runTrackedJob(job, async () => {
    const { logFile, progress } = createTrackedProgress(job, { stderr: false });
    job.logFile = logFile;
    if (payload.type === "review") {
      return executeReviewRun({ ...payload.request, onProgress: progress });
    }
    return executeTaskRun({ ...payload.request, jobId, onProgress: progress });
  });
}

function handleStatus(argv) {
  const { options, positionals } = parseCommandInput(argv, {
    booleanOptions: ["all", "json"],
    valueOptions: ["cwd"]
  });
  const cwd = resolveCommandCwd(options);
  const workspaceRoot = resolveWorkspaceRoot(cwd);
  const reference = positionals[0] ?? null;

  if (reference) {
    const snapshot = buildSingleJobSnapshot(workspaceRoot, reference);
    if (!snapshot) {
      outputResult(options.json ? { error: "not_found" } : `Job not found: ${reference}\n`, Boolean(options.json));
      process.exitCode = 1;
      return;
    }
    outputResult(options.json ? snapshot : renderJobStatusReport(snapshot), Boolean(options.json));
    return;
  }

  const snapshot = buildStatusSnapshot(workspaceRoot, { all: Boolean(options.all) });
  outputResult(options.json ? snapshot : renderStatusReport(snapshot), Boolean(options.json));
}

function handleResult(argv) {
  const { options, positionals } = parseCommandInput(argv, {
    booleanOptions: ["json"],
    valueOptions: ["cwd"]
  });
  const cwd = resolveCommandCwd(options);
  const workspaceRoot = resolveWorkspaceRoot(cwd);
  const job = resolveResultJob(workspaceRoot, positionals[0] ?? null);
  if (!job) {
    outputResult(options.json ? { error: "not_found" } : "No stored Grok result found.\n", Boolean(options.json));
    process.exitCode = 1;
    return;
  }
  const stored = readStoredJob(workspaceRoot, job.id);
  if (options.json) {
    outputResult({ job, stored }, true);
  } else {
    outputResult(renderStoredJobResult(stored, job), false);
  }
}

function handleTaskResumeCandidate(argv) {
  const { options } = parseCommandInput(argv, {
    booleanOptions: ["json"],
    valueOptions: ["cwd"]
  });
  const cwd = resolveCommandCwd(options);
  const workspaceRoot = resolveWorkspaceRoot(cwd);
  const jobs = sortJobsNewestFirst(listJobs(workspaceRoot)).filter(
    (j) => j.jobClass === "task" || j.kind === "task"
  );
  const latest = jobs.find(
    (j) => j.status === "completed" || j.status === "failed" || j.status === "running"
  );
  const available = Boolean(latest);
  const payload = {
    available,
    jobId: latest?.id ?? null,
    title: latest?.title ?? null,
    summary: latest?.summary ?? null,
    status: latest?.status ?? null,
    grokSessionId: latest?.grokSessionId ?? null
  };
  if (options.json) {
    outputResult(payload, true);
  } else {
    outputResult(
      available
        ? `Resumable task available: ${latest.id} (${latest.title})\n`
        : "No resumable Grok task thread for this repository.\n",
      false
    );
  }
}

async function handleCancel(argv) {
  const { options, positionals } = parseCommandInput(argv, {
    booleanOptions: ["json"],
    valueOptions: ["cwd"]
  });
  const cwd = resolveCommandCwd(options);
  const workspaceRoot = resolveWorkspaceRoot(cwd);
  const job = resolveCancelableJob(workspaceRoot, positionals[0] ?? null);
  if (!job) {
    outputResult(
      options.json ? { canceled: false, job: null } : "No cancelable Grok job found.\n",
      Boolean(options.json)
    );
    process.exitCode = 1;
    return;
  }

  let canceled = false;
  let detail = null;
  if (job.pid) {
    canceled = terminateProcessTree(job.pid);
    detail = canceled ? "signal sent" : "failed to signal process";
  } else {
    detail = "no pid recorded";
  }

  upsertJob(workspaceRoot, {
    id: job.id,
    status: "canceled",
    phase: "canceled",
    finishedAt: nowIso(),
    progressMessage: "Canceled by user",
    error: null
  });

  const report = { canceled: true, job, detail };
  outputResult(options.json ? report : renderCancelReport(report), Boolean(options.json));
}

async function main() {
  const [command, ...argv] = process.argv.slice(2);
  if (!command || command === "-h" || command === "--help") {
    printUsage();
    return;
  }

  switch (command) {
    case "setup":
      handleSetup(argv);
      break;
    case "review":
      await handleReviewCommand(argv, { adversarial: false });
      break;
    case "adversarial-review":
      await handleReviewCommand(argv, { adversarial: true });
      break;
    case "task":
      await handleTask(argv);
      break;
    case "task-worker":
      await handleTaskWorker(argv);
      break;
    case "status":
      handleStatus(argv);
      break;
    case "result":
      handleResult(argv);
      break;
    case "task-resume-candidate":
      handleTaskResumeCandidate(argv);
      break;
    case "cancel":
      await handleCancel(argv);
      break;
    case "transfer":
      await handleTransfer(argv);
      break;
    default:
      printUsage();
      throw new Error(`Unknown command: ${command}`);
  }
}

async function handleTransfer(argv) {
  const { options } = parseCommandInput(argv, {
    booleanOptions: ["read-only", "background", "json"],
    valueOptions: ["source", "cwd"]
  });
  const cwd = resolveCommandCwd(options);
  const source = options.source
    ? path.resolve(cwd, options.source)
    : findLatestClaudeSession(cwd);
  const handoff = buildTransferPrompt(source);
  process.stderr.write(
    `[grok] Transfer: ${handoff.turns} turns from ${handoff.source}\n`
  );
  // Re-enter task path
  const taskArgv = [];
  if (options.background) {
    taskArgv.push("--background");
  }
  if (options["read-only"]) {
    taskArgv.push("--read-only");
  }
  if (options.json) {
    taskArgv.push("--json");
  }
  taskArgv.push("--", handoff.prompt);
  await handleTask(taskArgv);
}

main().catch((error) => {
  process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
});
