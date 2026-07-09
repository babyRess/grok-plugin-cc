## 0.1.6

### Fixed
- Rescue background mapping: Claude-side `--background` must also pass companion `task --background`. Foreground `task` inside a reaped Claude Bash/subagent died with "PID … died without writing a result" and no log — that was not a broken Grok binary
- Job state no longer trusts foreign `CLAUDE_PLUGIN_DATA` (e.g. `codex-openai-codex`); prefers `GROK_PLUGIN_DATA`, grok-looking Claude data dirs, then `~/.grok/companion-state`
- Dead-worker status text no longer blames "older companion builds" for missing logs on foreground runs; points at companion `--background`

## 0.1.5

### Fixed
- Background workers no longer leave permanent `status=running` markers when the process dies mid-task: `status` reaps dead PIDs and marks jobs failed
- Detached `task-worker` now uses a real session (`setsid`) so it can survive parent shell exit, and redirects stdio to `{jobId}.log` instead of `/dev/null`
- Task/review error paths call `fail_job` so launch/wait failures never stick as `running`

## 0.1.4

### Fixed
- Headless write/rescue no longer passes both `--always-approve` and `--yolo` (Grok CLI treats `--yolo` as an alias of `--always-approve`, which failed with "cannot be used multiple times")

## 0.1.3

### Changed
- Simpler install: marketplace-only (Node) or one-line curl to `~/.grok/bin`
- `resolve-companion` finds global `~/.grok/bin/grok-companion`

# Changelog

## 0.1.2

### Added
- Prebuilt binary install (no Rust required): `scripts/install-companion.sh`
- GitHub Actions release workflow for macOS/Linux multi-arch assets
- `npm run install:companion` downloads latest release binary

## 0.1.1

### Added
- `/grok:transfer` — Claude session jsonl → Grok handoff task
- Grok `sessionId` capture (JSON wire format) + resume via stored id
- Smart review diffs (name-status + sampled patches for large trees)
- Stop-gate re-entry by Claude message hash (ALLOW skip; max 2 BLOCKs)

### Fixed / polish
- All slash commands use `resolve-companion.mjs`
- UTF-8 safe Claude context truncation
- `--runtime acp` reserved (not implemented yet)

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
