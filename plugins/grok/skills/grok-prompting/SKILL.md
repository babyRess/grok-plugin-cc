---
name: grok-prompting
description: Internal guidance for composing Grok prompts for coding, review, diagnosis, and research tasks inside the Grok Claude Code plugin
user-invocable: false
---

# Grok Prompting

Use only to tighten a user request before a single `task` forward. Do not solve the problem yourself.

## Principles

- State the goal, constraints, and definition of done.
- Prefer concrete file/path hints when the user already provided them.
- Ask for the smallest safe change when the user wants a fix.
- For diagnosis-only asks, say "do not modify files".
- Avoid vague verbs ("improve", "clean up") without success criteria.
- Keep the prompt short enough to stay on-task.

## Anti-patterns

- Dumping the entire conversation history.
- Asking Grok to "think step by step" without a deliverable.
- Mixing multiple unrelated tasks in one rescue call.
- Rewriting the user's intent into a different task.

## Recipes

See `references/prompt-blocks.md` for reusable blocks.
