# Changelog

## 0.1.0

Initial public preview of the Grok plugin for Claude Code.

### Features

- Slash commands: setup, review, adversarial-review, rescue, status, result, cancel
- `grok:grok-rescue` thin forwarder subagent
- Node companion CLI (`grok-companion.mjs`) driving headless `grok -p`
- Rust companion CLI (`crates/grok-companion`) with full command parity
- `resolve-companion.mjs` prefers Rust binary when installed via `npm run install:rust-bin`
- Background jobs (task-worker), status / result / cancel
- Optional stop-time review gate
- Claude on-disk skills + MCP inheritance (compact default; `--inherit-claude-context-full`)
- `--always-approve` for non-interactive Grok (avoids permission hangs)
- Heartbeat + stderr streaming while Grok runs

### Known limitations

See root README — not live Claude session state; MCP must connect in Grok; review can be slow on large diffs.
