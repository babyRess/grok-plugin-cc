---
description: Cancel an active background Grok companion job
argument-hint: "[job-id]"
allowed-tools: Bash(node:*)
---

```bash
node "${CLAUDE_PLUGIN_ROOT}/scripts/resolve-companion.mjs" cancel $ARGUMENTS
```

Return stdout verbatim.
