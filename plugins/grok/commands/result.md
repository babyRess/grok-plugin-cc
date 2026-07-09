---
description: Show the final stored Grok output for a finished job
argument-hint: "[job-id]"
allowed-tools: Bash(node:*)
---

```bash
node "${CLAUDE_PLUGIN_ROOT}/scripts/resolve-companion.mjs" result $ARGUMENTS
```

Return stdout verbatim. Do not paraphrase Grok's result.
