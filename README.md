# Grok plugin for Claude Code

**v0.1.3** — early preview. Use [Grok Build](https://grok.x.ai/) from inside Claude Code for code review and task delegation.

Architecture (inspired by [openai/codex-plugin-cc](https://github.com/openai/codex-plugin-cc)): slash commands + a thin rescue subagent call a local **companion** CLI, which runs your installed `grok` binary (`grok -p`). Optional Rust companion for lower latency.

---

## Requirements

| Dependency | Notes |
|------------|--------|
| **Node.js** ≥ 18.18 | Plugin scripts + tests |
| **Grok Build CLI** on `PATH` | Or set `GROK_BIN` to the binary |
| **Grok auth** | Log in once (`grok` interactive) so `~/.grok/auth.json` exists |
| **Rust** (optional) | Only if you build `plugins/grok/bin/grok-companion` |

Check readiness:

```bash
grok --version
# then either:
node plugins/grok/scripts/grok-companion.mjs setup
# or, after install:rust-bin:
./plugins/grok/bin/grok-companion setup
```

---

## Install (pick one)

### Option A — simplest (2 steps, no Rust, no clone)

Requires: Claude Code + `grok` logged in + Node (already on most machines).

```text
/plugin marketplace add babyRess/grok-plugin-cc
/plugin install grok@xai-grok
/reload-plugins
/grok:setup
```

Done. Uses the **Node companion** automatically. No binary download.

### Option B — add native speed (one extra line)

Still no Rust / no `cargo`. Downloads a prebuilt binary to `~/.grok/bin/`:

```bash
curl -fsSL https://raw.githubusercontent.com/babyRess/grok-plugin-cc/master/scripts/install-companion.sh | bash
```

Then reload Claude plugins (or open a new session). The plugin finds `~/.grok/bin/grok-companion` automatically.

### Option C — developers (build from source)

```bash
git clone https://github.com/babyRess/grok-plugin-cc.git
cd grok-plugin-cc
npm run install:rust-bin   # needs Rust
```

### Requirements

| | Option A | Option B | Option C |
|--|----------|----------|----------|
| Claude Code | yes | yes | yes |
| `grok` CLI + auth | yes | yes | yes |
| Node.js | yes | yes | yes |
| Prebuilt download | — | yes | — |
| Rust/cargo | — | — | yes |

---

## What you get

| Command | Purpose |
|---------|---------|
| `/grok:setup` | Check Grok CLI + auth; toggle optional stop review gate |
| `/grok:review` | Read-only review of working tree or branch vs `--base` |
| `/grok:adversarial-review` | Steerable challenge review (optional focus text) |
| `/grok:rescue` | Delegate investigate/fix work to Grok (write by default) |
| `/grok:status` | Running / recent jobs |
| `/grok:result` | Stored output for a finished job |
| `/grok:cancel` | Cancel a background job |
| `/grok:transfer` | Import latest Claude session transcript and continue in Grok |

Also registers the `grok:grok-rescue` subagent for natural-language handoff (“ask Grok to…”).

### Claude context inheritance (default on)

Task/review prompts inject a **compact** list of on-disk Claude skills + MCP server names from:

- `~/.claude/skills`, project `.claude/skills`
- `~/.claude.json` / Claude settings MCP config

```bash
# compact (default, ~2k chars)
./plugins/grok/bin/grok-companion task --read-only "What MCP servers do you see?"

# verbose descriptions (larger / slower)
./plugins/grok/bin/grok-companion task --read-only --inherit-claude-context-full "…"

# disable
./plugins/grok/bin/grok-companion task --read-only --no-inherit-claude-context "…"
```

---

## Usage

```text
/grok:setup
/grok:review
/grok:review --base main --background
/grok:adversarial-review look for race conditions in the cache layer
/grok:rescue investigate why the tests started failing
/grok:rescue --resume apply the top fix
/grok:status
/grok:result
/grok:cancel
```

### Companion CLI (direct)

```bash
# Node
node plugins/grok/scripts/resolve-companion.mjs setup --json
node plugins/grok/scripts/resolve-companion.mjs task --read-only "summarize this repo"

# Rust (after npm run install:rust-bin)
./plugins/grok/bin/grok-companion setup
./plugins/grok/bin/grok-companion task --read-only "Reply with PONG only"
./plugins/grok/bin/grok-companion review
```

### Environment

| Variable | Meaning |
|----------|---------|
| `GROK_BIN` | Path to `grok` binary |
| `GROK_COMPANION_MODEL` | Default model for companion runs |
| `CLAUDE_PLUGIN_DATA` / `GROK_PLUGIN_DATA` | Writable state root (jobs, config) |

---

## Known limitations

This is an **early preview (v0.1.0)**. Please read before filing issues:

1. **Not the live Claude session**  
   Inherited context is **on-disk** Claude config (skills dirs, MCP entries in JSON). It is **not** Claude’s current system prompt, open tools, or in-memory session state.

2. **MCP listed ≠ MCP callable**  
   Claude’s config may list 12 servers; Grok only uses those **connected in Grok’s process**. Failed auth/handshake servers appear in the list but cannot be called until fixed in Grok’s MCP setup.

3. **Compact vs full context**  
   Default injection is **compact** (names only) so prompts stay small and fast. `--inherit-claude-context-full` is slower and larger.

4. **Review can be slow**  
   `/grok:review` embeds git status + a capped diff. Large dirty trees (tens of thousands of lines) take longer. Prefer `--background` or a smaller scope / clean tree.

5. **Headless `grok -p` default**  
   Session IDs are captured via JSON output (`sessionId`) and shown after tasks. Resume with `/grok:rescue --resume` (uses stored id or `-c`). Full ACP (`grok agent stdio`) is reserved (`--runtime acp` exits with a roadmap message).

6. **Stop review gate is optional and aggressive**  
   Can create Claude↔Grok loops and burn usage. Off by default. Re-entry is keyed by Claude message hash (ALLOW skip / max 2 BLOCKs per message).

7. **Rust binary is not shipped in git**  
   `plugins/grok/bin/grok-companion` is gitignored; users build with `npm run install:rust-bin` or use the Node companion.

---

## Architecture

```
Claude Code  (/grok:* commands, grok-rescue subagent, hooks)
      │
      ▼
resolve-companion.mjs  →  plugins/grok/bin/grok-companion (Rust) if present
                       →  else grok-companion.mjs (Node)
      │
      ▼
grok -p …   headless Grok Build (local auth + config + MCP)
```

Job state: `$CLAUDE_PLUGIN_DATA/state/<workspace-slug-hash>/` (or temp fallback).

### Stop review gate (optional)

```text
/grok:setup --enable-review-gate
/grok:setup --disable-review-gate
```

---

## Development

```bash
# Node unit tests (no network / no grok required)
npm test

# Rust unit tests (no live grok API required)
cargo test -p grok-companion --bin grok-companion

# Live integration tests (needs authenticated `grok` on PATH)
cargo test -p grok-companion --test cli_real_grok -- --test-threads=1

npm run install:rust-bin
```

### Layout

```
grok-plugin-cc/
├── Cargo.toml
├── crates/grok-companion/     # Rust companion CLI
├── .claude-plugin/marketplace.json
├── .github/workflows/ci.yml
├── plugins/grok/
│   ├── agents/
│   ├── commands/
│   ├── hooks/
│   ├── prompts/
│   ├── scripts/               # Node companion + resolve-companion
│   ├── bin/                   # built binary (gitignored)
│   └── skills/
└── tests/
```

---

## Roadmap

- [x] Rust companion full CLI
- [x] Live integration tests against real local `grok`
- [x] Compact Claude skills/MCP inheritance
- [x] Capture Grok `sessionId` (JSON) + resume via stored id / `-c`
- [x] `/grok:transfer` Claude transcript → Grok handoff prompt
- [x] Smart review diffs for large dirty trees
- [x] Stop-gate hardened (message-hash re-entry, block cap)
- [ ] Full ACP client over `grok agent stdio` (flag reserved)
- [ ] Structured review JSON via `--json-schema`

---

## License

Apache-2.0. Architecture inspired by OpenAI’s `codex-plugin-cc` (Apache-2.0); this is an independent implementation targeting Grok Build.
