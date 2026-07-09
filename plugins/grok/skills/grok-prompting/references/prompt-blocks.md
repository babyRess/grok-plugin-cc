# Prompt blocks

## Fix request

```text
Investigate and fix: <problem>
Constraints: smallest safe patch; do not refactor unrelated code.
Definition of done: <tests or observable check>.
```

## Diagnosis only

```text
Diagnose only (do not modify files): <problem>
Report root cause, evidence, and recommended fix options ordered by risk.
```

## Continue prior work

```text
Continue the previous Grok task in this repository.
Apply the highest-value next step and keep going until the original goal is met or blocked.
```
