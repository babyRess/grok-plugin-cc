---
name: grok-result-handling
description: Internal guidance for presenting Grok companion output back to the user
user-invocable: false
---

# Grok Result Handling

When a slash command or subagent returns `grok-companion` stdout:

1. Show it to the user **verbatim**.
2. Do not paraphrase, summarize, or "improve" the wording.
3. Do not start fixing issues mentioned in a review unless the user asks.
4. For background launches, only confirm the job id and point at `/grok:status` / `/grok:result`.
5. If setup fails, tell the user to run `/grok:setup` and ensure the `grok` CLI is installed and authenticated.
