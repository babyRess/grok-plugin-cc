---
name: grok-cli-runtime
description: Internal helper contract for calling the grok-companion runtime from Claude Code
user-invocable: false
---

# Grok Runtime

Use this skill only inside the `grok:grok-rescue` subagent.

Primary helper:
- `node "${CLAUDE_PLUGIN_ROOT}/scripts/resolve-companion.mjs" task "<raw arguments>"`
  (falls back to Node companion if the Rust binary is not installed under `bin/`)

Execution rules:
- The rescue subagent is a forwarder, not an orchestrator. Its only job is to invoke `task` once and return that stdout unchanged.
- Prefer the helper over hand-rolled `git`, direct Grok CLI strings, or any other Bash activity.
- Do not call `setup`, `review`, `adversarial-review`, `status`, `result`, or `cancel` from `grok:grok-rescue`.
- Use `task` for every rescue request, including diagnosis, planning, research, and explicit fix requests.
- You may use the `grok-prompting` skill to rewrite the user's request into a tighter Grok prompt before the single `task` call.
- That prompt drafting is the only Claude-side work allowed. Do not inspect the repo, solve the task yourself, or add independent analysis outside the forwarded prompt text.
- Leave `--effort` unset unless the user explicitly requests a specific effort.
- Leave model unset by default. Add `--model` only when the user explicitly asks for one.
- Default to write-capable Grok work unless the user explicitly asks for read-only behavior (then pass `--read-only`).

Command selection:
- Use exactly one `task` invocation per rescue handoff.
- **Background mapping (critical):**
  - Claude-side `--background` / background subagent execution is **not** enough by itself.
  - When this rescue runs in the background (user passed `--background`, or you chose background for a long task), you **must** pass companion `task --background ...`.
  - That detaches a real `task-worker` so Grok survives when Claude reaps the Bash/subagent process.
  - If you strip companion `--background` and only run a foreground `task` inside a Claude-background job, the process dies mid-run with: "Worker process PID … died without writing a result" and no log.
  - Claude-side `--wait` / foreground rescue: call companion `task` **without** `--background` and return full stdout.
  - Never treat `--background` / `--wait` as natural-language task text.
- If the forwarded request includes `--model` or `--effort`, pass them through to `task`.
- If the forwarded request includes `--resume`, strip that token from the task text and add `--resume-last`.
- If the forwarded request includes `--fresh`, strip that token from the task text and do not add `--resume-last`.
- `--effort`: accepted values are `none`, `minimal`, `low`, `medium`, `high`, `xhigh`.
- `task --resume-last`: internal helper for "keep going", "resume", "apply the top fix", or "dig deeper" after a previous rescue run.

Safety rules:
- Default to write-capable Grok work in `grok:grok-rescue` unless the user explicitly asks for read-only behavior.
- Preserve the user's task text as-is apart from stripping routing flags.
- Do not inspect the repository, read files, grep, monitor progress, poll status, fetch results, cancel jobs, summarize output, or do any follow-up work of your own.
- Return the stdout of the `task` command exactly as-is.
- If the Bash call fails or Grok cannot be invoked, return nothing.
