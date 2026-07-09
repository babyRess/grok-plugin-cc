---
description: Run a Grok code review against local git state
argument-hint: "[--wait|--background] [--base <ref>] [--scope auto|working-tree|branch] [--model <m>] [--effort <level>] [--no-inherit-claude-context]"
disable-model-invocation: true
allowed-tools: Read, Glob, Grep, Bash(node:*), Bash(git:*), AskUserQuestion
---

Run a Grok review through the shared companion runtime.

Raw slash-command arguments:
`$ARGUMENTS`

Core constraint:
- This command is review-only.
- Do not fix issues, apply patches, or suggest that you are about to make changes.
- Your only job is to run the review and return Grok's output verbatim to the user.

Execution mode rules:
- If the raw arguments include `--wait`, do not ask. Run the review in the foreground.
- If the raw arguments include `--background`, do not ask. Run the review in a Claude background task.
- Otherwise, estimate the review size before asking:
  - For working-tree review, start with `git status --short --untracked-files=all`.
  - For working-tree review, also inspect both `git diff --shortstat --cached` and `git diff --shortstat`.
  - For base-branch review, use `git diff --shortstat <base>...HEAD`.
  - Recommend waiting only when the review is clearly tiny (roughly 1-2 files).
  - In every other case, including unclear size, recommend background.
- Then use `AskUserQuestion` exactly once with two options, putting the recommended option first and suffixing its label with `(Recommended)`:
  - `Wait for results`
  - `Run in background`

Argument handling:
- Preserve the user's arguments exactly.
- Do not strip `--wait` or `--background` yourself.
- The companion script parses flags; Claude Code's `Bash(..., run_in_background: true)` is what actually detaches the run when using background mode from Claude.

Foreground flow:
- Run:
```bash
node "${CLAUDE_PLUGIN_ROOT}/scripts/resolve-companion.mjs" review $ARGUMENTS
```
- Return the command stdout verbatim, exactly as-is.
- Do not paraphrase, summarize, or add commentary before or after it.
- Do not fix any issues mentioned in the review output.

Background flow:
- Launch the review with `Bash` in the background (Node companion supports `--background`; Rust binary is foreground-only for now — resolve-companion falls back as needed):
```text
Bash({
  command: `node "${CLAUDE_PLUGIN_ROOT}/scripts/resolve-companion.mjs" review $ARGUMENTS --background`,
  description: "Grok review",
  run_in_background: true
})
```
- Do not wait for completion in this turn.
- After launching, tell the user: "Grok review started in the background. Check `/grok:status` for progress."
