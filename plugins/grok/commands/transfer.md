---
description: Transfer the latest Claude Code session transcript into a Grok task and continue
argument-hint: "[--source <session.jsonl>] [--read-only] [--background]"
allowed-tools: Bash(node:*), Bash(ls:*), Glob
---

Transfer Claude conversation context into Grok via the companion CLI.

Raw arguments:
$ARGUMENTS

Rules:
- Prefer the resolve-companion entrypoint (Rust binary when installed).
- Do not paraphrase companion stdout.

```bash
node "${CLAUDE_PLUGIN_ROOT}/scripts/resolve-companion.mjs" transfer $ARGUMENTS
```

Return stdout verbatim.
