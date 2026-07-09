---
description: Show running and recent Grok companion jobs for this repository
argument-hint: "[job-id] [--all]"
allowed-tools: Bash(node:*)
---

```bash
node "${CLAUDE_PLUGIN_ROOT}/scripts/resolve-companion.mjs" status $ARGUMENTS
```

Return stdout verbatim.
