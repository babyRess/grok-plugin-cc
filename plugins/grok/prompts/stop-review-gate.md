Run a stop-gate review of the previous Claude turn.

{{CLAUDE_RESPONSE_BLOCK}}

Decide whether Claude's last turn is safe to accept (no critical bugs introduced, no unsafe operations left half-done, claims match reality).

Reply with exactly one of these as the **first line**:
ALLOW: <one-line reason>
BLOCK: <concrete issues Claude must fix before stopping>

If blocking, list the issues clearly after the BLOCK: line. Do not modify files.
