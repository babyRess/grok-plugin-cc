---
description: Check whether the local Grok CLI is ready and optionally toggle the stop-time review gate
argument-hint: "[--enable-review-gate|--disable-review-gate]"
allowed-tools: Bash(node:*)
---

Run Grok companion setup and report readiness.

Raw arguments:
$ARGUMENTS

```bash
node "${CLAUDE_PLUGIN_ROOT}/scripts/resolve-companion.mjs" setup $ARGUMENTS
```

Return the command stdout verbatim. Do not paraphrase.
