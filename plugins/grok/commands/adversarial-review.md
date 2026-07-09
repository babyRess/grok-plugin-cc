---
description: Run a steerable adversarial Grok review that challenges design choices
argument-hint: "[--wait|--background] [--base <ref>] [--scope auto|working-tree|branch] [--model <m>] [--effort <level>] [--no-inherit-claude-context] [focus text]"
disable-model-invocation: true
allowed-tools: Read, Glob, Grep, Bash(node:*), Bash(git:*), AskUserQuestion
---

Run an adversarial Grok review through the shared companion runtime.

Raw slash-command arguments:
`$ARGUMENTS`

Core constraint:
- Review-only. Do not fix code.
- Return Grok's output verbatim.

Execution mode:
- Honor `--wait` / `--background` if present.
- Otherwise estimate size (same as `/grok:review`) and use `AskUserQuestion` once with `Wait for results` vs `Run in background`, recommended first.

Foreground:
```bash
node "${CLAUDE_PLUGIN_ROOT}/scripts/resolve-companion.mjs" adversarial-review $ARGUMENTS
```

Return stdout verbatim. Do not paraphrase.

Background: launch with `run_in_background: true` and tell the user to check `/grok:status`.
