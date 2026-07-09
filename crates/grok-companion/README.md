# grok-companion (Rust)

Full native reimplementation of the Grok plugin companion CLI. Uses the **real** local `grok` binary (`GROK_BIN` or PATH) — no fakes.

Commands:

| Command | Purpose |
|---------|---------|
| `setup` | Grok binary + auth + stop-gate config |
| `status` | List jobs |
| `review` | Read-only code review |
| `adversarial-review` | Steerable challenge review |
| `task` | Write-capable rescue (default) / `--read-only` |
| *(all task/review)* | `--no-inherit-claude-context` to skip Claude skill/MCP injection (on by default) |
| `task-worker` | Internal background worker |
| `result` | Stored job output |
| `cancel` | Cancel active job |
| `task-resume-candidate` | Resumable task probe |

State layout matches the Node companion under  
`$CLAUDE_PLUGIN_DATA/state/<slug-hash>/` (or `$GROK_PLUGIN_DATA`, or temp fallback).

## Build

```bash
# from repo root
cargo build -p grok-companion --release
./target/release/grok-companion setup
npm run install:rust-bin   # copy into plugins/grok/bin/
```

## Usage

```bash
grok-companion setup [--enable-review-gate|--disable-review-gate] [--json]
grok-companion status [job-id] [--all] [--json]
grok-companion review [--base <ref>] [--background] [--json]
grok-companion adversarial-review [--base <ref>] [focus text]
grok-companion task [--read-only] [--resume-last] [--background] [prompt]
grok-companion result [job-id]
grok-companion cancel [job-id]
grok-companion task-resume-candidate [--json]
```

## Tests

```bash
cargo test -p grok-companion          # unit + real-grok integration
```

Integration tests in `tests/cli_real_grok.rs` call your installed `grok` CLI (must be authenticated).

## Env

| Variable | Meaning |
|----------|---------|
| `GROK_BIN` | Path to `grok` |
| `GROK_COMPANION_MODEL` | Default model |
| `CLAUDE_PLUGIN_DATA` / `GROK_PLUGIN_DATA` | Plugin data root for job state |

## Plugin wiring

Slash commands can call either:

```bash
node "${CLAUDE_PLUGIN_ROOT}/scripts/grok-companion.mjs" setup
# or
"${CLAUDE_PLUGIN_ROOT}/bin/grok-companion" setup
```

Copy or symlink the release binary into `plugins/grok/bin/` after build.
