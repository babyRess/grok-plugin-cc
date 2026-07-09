import path from "node:path";
import { runCommand } from "./process.mjs";
import { resolveWorkspaceRoot } from "./workspace.mjs";

const DEFAULT_MAX_DIFF_CHARS = 40_000;
const SMART_DIFF_THRESHOLD = 24_000;
const MAX_FILES_LISTED = 80;
const MAX_SAMPLED_FILES = 12;
const MAX_PER_FILE_PATCH = 4_000;

function git(cwd, args) {
  return runCommand("git", args, { cwd, timeoutMs: 30_000 });
}

export function ensureGitRepository(cwd) {
  const result = git(cwd, ["rev-parse", "--is-inside-work-tree"]);
  if (result.status !== 0 || result.stdout.trim() !== "true") {
    throw new Error("Not inside a git repository. Run this from a repo checkout.");
  }
  return resolveWorkspaceRoot(cwd);
}

/**
 * @param {string} cwd
 * @param {{ base?: string | null, scope?: string | null }} [options]
 */
export function resolveReviewTarget(cwd, options = {}) {
  const repoRoot = ensureGitRepository(cwd);
  const scope = options.scope ?? "auto";
  const base = options.base ?? null;

  if (base || scope === "branch") {
    const baseRef = base || "main";
    const shortstat = git(repoRoot, ["diff", "--shortstat", `${baseRef}...HEAD`]);
    const status = git(repoRoot, ["status", "--short", "--untracked-files=all"]);
    return {
      kind: "branch",
      base: baseRef,
      label: `branch vs ${baseRef}`,
      repoRoot,
      empty:
        !shortstat.stdout.trim() &&
        !status.stdout.trim() &&
        shortstat.status === 0
    };
  }

  const status = git(repoRoot, ["status", "--short", "--untracked-files=all"]);
  const cached = git(repoRoot, ["diff", "--shortstat", "--cached"]);
  const unstaged = git(repoRoot, ["diff", "--shortstat"]);
  const empty = !status.stdout.trim() && !cached.stdout.trim() && !unstaged.stdout.trim();

  return {
    kind: "working-tree",
    base: null,
    label: "working tree",
    repoRoot,
    empty
  };
}

/**
 * Collect a text summary of the review target for prompt injection.
 * @param {string} cwd
 * @param {ReturnType<typeof resolveReviewTarget>} target
 * @param {{ maxDiffChars?: number }} [options]
 */
export function collectReviewContext(cwd, target, options = {}) {
  const repoRoot = target.repoRoot || resolveWorkspaceRoot(cwd);
  const branchResult = git(repoRoot, ["rev-parse", "--abbrev-ref", "HEAD"]);
  const branch = branchResult.stdout.trim() || "HEAD";

  let diff = "";
  let status = "";
  let summary = "";
  let nameStatus = "";

  if (target.kind === "branch" && target.base) {
    const log = git(repoRoot, ["log", "--oneline", `${target.base}...HEAD`, "-20"]);
    const shortstat = git(repoRoot, ["diff", "--shortstat", `${target.base}...HEAD`]);
    const fullDiff = git(repoRoot, ["diff", `${target.base}...HEAD`]);
    const names = git(repoRoot, ["diff", "--name-status", `${target.base}...HEAD`]);
    status = log.stdout.trim();
    summary = shortstat.stdout.trim() || "No branch diff.";
    diff = fullDiff.stdout;
    nameStatus = names.stdout;
  } else {
    const st = git(repoRoot, ["status", "--short", "--untracked-files=all"]);
    const cached = git(repoRoot, ["diff", "--cached"]);
    const unstaged = git(repoRoot, ["diff"]);
    const namesC = git(repoRoot, ["diff", "--name-status", "--cached"]);
    const namesU = git(repoRoot, ["diff", "--name-status"]);
    status = st.stdout.trim();
    summary = [
      git(repoRoot, ["diff", "--shortstat", "--cached"]).stdout.trim(),
      git(repoRoot, ["diff", "--shortstat"]).stdout.trim()
    ]
      .filter(Boolean)
      .join(" | ") || (status ? "Untracked or status-only changes present." : "No changes.");
    diff = [cached.stdout, unstaged.stdout].filter(Boolean).join("\n");
    nameStatus = [namesC.stdout, namesU.stdout].filter(Boolean).join("\n");
  }

  const maxDiffChars = Number(options.maxDiffChars) || DEFAULT_MAX_DIFF_CHARS;
  diff = buildSmartDiff(diff, nameStatus, maxDiffChars);

  return {
    repoRoot,
    branch,
    target,
    status,
    summary,
    diff,
    relativeHint: path.relative(process.cwd(), repoRoot) || "."
  };
}

/**
 * @param {string} rawDiff
 * @param {string} nameStatus
 * @param {number} maxDiffChars
 */
export function buildSmartDiff(rawDiff, nameStatus, maxDiffChars = DEFAULT_MAX_DIFF_CHARS) {
  if (!rawDiff) {
    return "";
  }
  if (rawDiff.length <= Math.min(SMART_DIFF_THRESHOLD, maxDiffChars)) {
    return hardCap(rawDiff, maxDiffChars);
  }

  const files = String(nameStatus || "")
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter(Boolean);

  let out = `[smart-diff] Full diff is large (${rawDiff.length} chars). Showing name-status + sampled patches.\n\n`;
  out += `### Changed files (${files.length})\n`;
  for (const line of files.slice(0, MAX_FILES_LISTED)) {
    out += `${line}\n`;
  }
  if (files.length > MAX_FILES_LISTED) {
    out += `... ${files.length - MAX_FILES_LISTED} more files omitted\n`;
  }
  out += "\n";

  const patches = splitGitPatches(rawDiff);
  out += `### Sampled patches (first ${Math.min(MAX_SAMPLED_FILES, patches.length)} of ${patches.length})\n`;
  for (const patch of patches.slice(0, MAX_SAMPLED_FILES)) {
    let p = patch;
    if (p.length > MAX_PER_FILE_PATCH) {
      p = `${p.slice(0, MAX_PER_FILE_PATCH)}\n...[file patch truncated]...\n`;
    }
    out += `${p}\n`;
    if (out.length >= maxDiffChars) {
      break;
    }
  }
  return hardCap(out, maxDiffChars);
}

function splitGitPatches(diff) {
  if (!diff.includes("diff --git ")) {
    return [diff];
  }
  const parts = diff.split(/(?=^diff --git )/m).filter(Boolean);
  return parts.length ? parts : [diff];
}

function hardCap(s, max) {
  if (s.length <= max) {
    return s;
  }
  return `${s.slice(0, max)}\n\n...[diff truncated at ${max} chars]...`;
}
