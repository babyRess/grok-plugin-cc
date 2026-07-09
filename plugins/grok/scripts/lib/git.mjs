import path from "node:path";
import { runCommand } from "./process.mjs";
import { resolveWorkspaceRoot } from "./workspace.mjs";

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
 */
export function collectReviewContext(cwd, target) {
  const repoRoot = target.repoRoot || resolveWorkspaceRoot(cwd);
  const branchResult = git(repoRoot, ["rev-parse", "--abbrev-ref", "HEAD"]);
  const branch = branchResult.stdout.trim() || "HEAD";

  let diff = "";
  let status = "";
  let summary = "";

  if (target.kind === "branch" && target.base) {
    const log = git(repoRoot, ["log", "--oneline", `${target.base}...HEAD`, "-20"]);
    const shortstat = git(repoRoot, ["diff", "--shortstat", `${target.base}...HEAD`]);
    const fullDiff = git(repoRoot, ["diff", `${target.base}...HEAD`]);
    status = log.stdout.trim();
    summary = shortstat.stdout.trim() || "No branch diff.";
    diff = fullDiff.stdout;
  } else {
    const st = git(repoRoot, ["status", "--short", "--untracked-files=all"]);
    const cached = git(repoRoot, ["diff", "--cached"]);
    const unstaged = git(repoRoot, ["diff"]);
    status = st.stdout.trim();
    summary = [
      git(repoRoot, ["diff", "--shortstat", "--cached"]).stdout.trim(),
      git(repoRoot, ["diff", "--shortstat"]).stdout.trim()
    ]
      .filter(Boolean)
      .join(" | ") || (status ? "Untracked or status-only changes present." : "No changes.");
    diff = [cached.stdout, unstaged.stdout].filter(Boolean).join("\n");
  }

  // Cap huge diffs so prompts stay manageable
  const maxDiffChars = 120_000;
  if (diff.length > maxDiffChars) {
    diff = `${diff.slice(0, maxDiffChars)}\n\n...[diff truncated at ${maxDiffChars} chars]...`;
  }

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
