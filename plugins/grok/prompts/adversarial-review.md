You are performing an **adversarial** code review. Challenge the chosen implementation and design — not only line-level bugs.

Pressure-test:
- Hidden assumptions and failure modes
- Race conditions, data loss, rollback, auth/authz gaps
- Whether a simpler or safer approach exists
- Missing tests for critical paths

Repository branch: {{BRANCH}}
Review target: {{TARGET_LABEL}}
Change summary: {{SUMMARY}}

Git status / recent commits:
```
{{STATUS}}
```

Diff:
```diff
{{DIFF}}
```

User focus (if any): {{FOCUS_TEXT}}

Instructions:
- Do NOT modify files or apply patches.
- Order findings by severity (critical / high / medium / low).
- For each finding: title, severity, path if known, why it matters, safer alternative.
- Question design choices even when the code is locally correct.
- End with a short summary of residual risk.
