/**
 * Discover on-disk Claude Code skills + MCP config for Grok prompt injection.
 */
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { resolveWorkspaceRoot } from "./workspace.mjs";

const MAX_SKILLS = 80;
const MAX_MCP = 40;
const MAX_DESC = 80;
const MAX_BLOCK_CHARS = 3500;

/**
 * @param {string} cwd
 * @returns {{ skills: object[], mcpServers: object[], notes: string[] }}
 */
export function collectClaudeContext(cwd) {
  const home = os.homedir();
  const workspace = resolveWorkspaceRoot(cwd);
  /** @type {object[]} */
  const skills = [];
  /** @type {object[]} */
  const mcpServers = [];
  /** @type {string[]} */
  const notes = [];

  collectSkillsDir(path.join(home, ".claude", "skills"), "user:~/.claude/skills", skills);
  collectSkillsDir(
    path.join(workspace, ".claude", "skills"),
    "project:.claude/skills",
    skills
  );
  collectCommandsDir(
    path.join(home, ".claude", "commands"),
    "user:~/.claude/commands",
    skills
  );
  collectCommandsDir(
    path.join(workspace, ".claude", "commands"),
    "project:.claude/commands",
    skills
  );

  dedupeByName(skills);
  if (skills.length > MAX_SKILLS) {
    const overflow = skills.length - MAX_SKILLS;
    skills.length = MAX_SKILLS;
    notes.push(`Skill list truncated; ${overflow} more skill(s) omitted.`);
  }

  const mcpFiles = [
    [path.join(home, ".claude.json"), "user:~/.claude.json"],
    [path.join(home, ".claude", "settings.json"), "user:~/.claude/settings.json"],
    [path.join(home, ".claude", "settings.local.json"), "user:~/.claude/settings.local.json"],
    [path.join(workspace, ".claude", "settings.json"), "project:.claude/settings.json"],
    [path.join(workspace, ".claude", "settings.local.json"), "project:.claude/settings.local.json"],
    [path.join(workspace, ".mcp.json"), "project:.mcp.json"]
  ];
  for (const [file, source] of mcpFiles) {
    collectMcpFromJsonFile(file, source, mcpServers);
  }
  dedupeByName(mcpServers);
  if (mcpServers.length > MAX_MCP) {
    const overflow = mcpServers.length - MAX_MCP;
    mcpServers.length = MAX_MCP;
    notes.push(`MCP list truncated; ${overflow} more server(s) omitted.`);
  }

  if (skills.length === 0 && mcpServers.length === 0) {
    notes.push("No Claude skills or MCP servers found under ~/.claude or project .claude/.");
  }

  return { skills, mcpServers, notes };
}

/**
 * @param {{ skills: object[], mcpServers: object[], notes: string[] }} ctx
 * @param {"compact"|"full"} [detail]
 */
export function formatClaudeContextBlock(ctx, detail = "compact") {
  if (!ctx.skills.length && !ctx.mcpServers.length) {
    return "";
  }

  const lines = [
    "## Claude Code context (on-disk, compact)",
    "Not the live Claude session. Names only — do not open every skill file.",
    "MCP listed here is Claude's config; call tools only if Grok has the same MCP.",
    ""
  ];

  if (ctx.mcpServers.length) {
    lines.push(`### MCP servers (${ctx.mcpServers.length})`);
    if (detail === "full") {
      for (const m of ctx.mcpServers) {
        lines.push(`- \`${m.name}\` [${m.kind}] ${m.detail}`);
      }
    } else {
      lines.push(ctx.mcpServers.map((m) => `\`${m.name}\`(${m.kind})`).join(", "));
    }
    lines.push("");
  }

  if (ctx.skills.length) {
    lines.push(`### Skills (${ctx.skills.length})`);
    if (detail === "full") {
      for (const s of ctx.skills) {
        const desc = s.description ? ` — ${s.description}` : "";
        lines.push(`- \`${s.name}\`${desc}`);
      }
    } else {
      lines.push(ctx.skills.map((s) => `\`${s.name}\``).join(", "));
    }
    lines.push("");
  }

  for (const n of ctx.notes) {
    lines.push(`Note: ${n}`);
  }

  let block = lines.join("\n");
  if (block.length > MAX_BLOCK_CHARS) {
    block = `${block.slice(0, MAX_BLOCK_CHARS)}\n…[claude context truncated]\n`;
  }
  return block;
}

/**
 * @param {string} prompt
 * @param {string} cwd
 * @param {boolean} enabled
 * @param {"compact"|"full"} [detail]
 */
export function maybeInjectClaudeContext(prompt, cwd, enabled = true, detail = "compact") {
  if (!enabled) {
    return prompt;
  }
  const ctx = collectClaudeContext(cwd);
  const block = formatClaudeContextBlock(ctx, detail);
  if (!block.trim()) {
    return prompt;
  }
  process.stderr.write(
    `[grok] Claude context: ${ctx.mcpServers.length} MCP, ${ctx.skills.length} skills, ~${block.length} chars (${detail})\n`
  );
  return `${block}\n---\n\n${prompt}`;
}

function collectSkillsDir(dir, source, out) {
  if (!fs.existsSync(dir)) {
    return;
  }
  let entries;
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true });
  } catch {
    return;
  }
  for (const ent of entries) {
    const full = path.join(dir, ent.name);
    if (ent.isDirectory()) {
      const skillMd = path.join(full, "SKILL.md");
      if (fs.existsSync(skillMd)) {
        out.push({
          name: ent.name,
          path: skillMd,
          description: parseSkillDescription(skillMd),
          source
        });
      }
    } else if (ent.isFile() && ent.name.endsWith(".md")) {
      out.push({
        name: path.basename(ent.name, ".md"),
        path: full,
        description: parseSkillDescription(full),
        source
      });
    }
  }
}

function collectCommandsDir(dir, source, out) {
  if (!fs.existsSync(dir)) {
    return;
  }
  let entries;
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true });
  } catch {
    return;
  }
  for (const ent of entries) {
    if (!ent.isFile() || !ent.name.endsWith(".md")) {
      continue;
    }
    const name = path.basename(ent.name, ".md");
    if (out.some((s) => s.name === name)) {
      continue;
    }
    const full = path.join(dir, ent.name);
    out.push({
      name,
      path: full,
      description: parseSkillDescription(full),
      source
    });
  }
}

function parseSkillDescription(filePath) {
  let raw;
  try {
    raw = fs.readFileSync(filePath, "utf8");
  } catch {
    return null;
  }
  if (raw.startsWith("---")) {
    const end = raw.indexOf("\n---", 3);
    if (end !== -1) {
      const fm = raw.slice(3, end);
      for (const line of fm.split(/\r?\n/)) {
        const m = line.match(/^description:\s*(.*)$/);
        if (m) {
          const v = m[1].trim().replace(/^["']|["']$/g, "");
          if (v) {
            return truncate(v, MAX_DESC);
          }
        }
      }
    }
  }
  for (const line of raw.split(/\r?\n/)) {
    const t = line.trim();
    if (!t || t.startsWith("#") || t === "---") {
      continue;
    }
    if (t.startsWith("name:") || t.startsWith("description:")) {
      continue;
    }
    return truncate(t, MAX_DESC);
  }
  return null;
}

function collectMcpFromJsonFile(filePath, source, out) {
  if (!fs.existsSync(filePath)) {
    return;
  }
  let value;
  try {
    value = JSON.parse(fs.readFileSync(filePath, "utf8"));
  } catch {
    return;
  }
  const servers = value?.mcpServers || value?.mcp_servers || {};
  if (!servers || typeof servers !== "object") {
    return;
  }
  for (const [name, cfg] of Object.entries(servers)) {
    out.push(mcpEntryFromConfig(name, cfg, source));
  }
}

function mcpEntryFromConfig(name, cfg, source) {
  if (cfg?.command) {
    const args = Array.isArray(cfg.args) ? cfg.args.join(" ") : "";
    const detail = args ? `${cfg.command} ${args}` : String(cfg.command);
    return { name, kind: "stdio", detail: truncate(detail, 200), source };
  }
  if (cfg?.url || cfg?.serverUrl) {
    return {
      name,
      kind: "http",
      detail: truncate(String(cfg.url || cfg.serverUrl), 200),
      source
    };
  }
  return { name, kind: "unknown", detail: "configured", source };
}

function dedupeByName(items) {
  const seen = new Set();
  // keep last (project preferred if collected later) — reverse
  const rev = [...items].reverse();
  const kept = [];
  for (const item of rev) {
    if (seen.has(item.name)) {
      continue;
    }
    seen.add(item.name);
    kept.push(item);
  }
  items.length = 0;
  items.push(...kept.reverse().sort((a, b) => a.name.localeCompare(b.name)));
}

function truncate(s, max) {
  const t = String(s).trim();
  if (t.length <= max) {
    return t;
  }
  return `${t.slice(0, max - 1)}…`;
}
