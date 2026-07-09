/**
 * Build a Grok handoff prompt from a Claude Code session jsonl.
 */
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { resolveWorkspaceRoot } from "./workspace.mjs";

const MAX_TURNS = 24;
const MAX_CHARS_PER_MSG = 4_000;
const MAX_HANDOFF_CHARS = 28_000;

function encodeProjectDir(p) {
  let abs = p;
  try {
    abs = fs.realpathSync(p);
  } catch {
    // keep
  }
  return String(abs).replace(/[^a-zA-Z0-9]+/g, "-");
}

function collectJsonl(dir, out) {
  if (!fs.existsSync(dir)) {
    return;
  }
  for (const name of fs.readdirSync(dir)) {
    if (name.endsWith(".jsonl")) {
      out.push(path.join(dir, name));
    }
  }
}

export function findLatestClaudeSession(cwd) {
  const workspace = resolveWorkspaceRoot(cwd);
  const projects = path.join(os.homedir(), ".claude", "projects");
  if (!fs.existsSync(projects)) {
    throw new Error(`No Claude projects directory at ${projects}`);
  }
  const candidates = [];
  const preferred = path.join(projects, encodeProjectDir(workspace));
  collectJsonl(preferred, candidates);
  if (candidates.length === 0) {
    const base = path.basename(workspace);
    for (const name of fs.readdirSync(projects)) {
      if (base && name.includes(base)) {
        collectJsonl(path.join(projects, name), candidates);
      }
    }
  }
  if (candidates.length === 0) {
    for (const name of fs.readdirSync(projects)) {
      collectJsonl(path.join(projects, name), candidates);
    }
  }
  candidates.sort((a, b) => {
    try {
      return fs.statSync(b).mtimeMs - fs.statSync(a).mtimeMs;
    } catch {
      return 0;
    }
  });
  if (!candidates.length) {
    throw new Error(`No Claude session jsonl found for ${workspace}`);
  }
  return candidates[0];
}

function extractContent(v) {
  if (typeof v?.content === "string") {
    return v.content;
  }
  if (Array.isArray(v?.content)) {
    return v.content
      .map((item) => item?.text || (typeof item === "string" ? item : ""))
      .filter(Boolean)
      .join("\n");
  }
  if (typeof v?.text === "string") {
    return v.text;
  }
  return "";
}

function truncateMsg(s) {
  const t = String(s).trim();
  if (t.length <= MAX_CHARS_PER_MSG) {
    return t;
  }
  return `${t.slice(0, MAX_CHARS_PER_MSG)}\n…[message truncated]…`;
}

export function buildTransferPrompt(source) {
  const raw = fs.readFileSync(source, "utf8");
  const turns = [];
  for (const line of raw.split(/\r?\n/)) {
    if (!line.trim()) {
      continue;
    }
    let v;
    try {
      v = JSON.parse(line);
    } catch {
      continue;
    }
    let role = v.role || v.type || "";
    let content = extractContent(v);
    if ((!role || !content) && v.message) {
      role = v.message.role || role;
      content = extractContent(v.message);
    }
    if (role === "human") {
      role = "user";
    }
    if (role === "ai") {
      role = "assistant";
    }
    if (role !== "user" && role !== "assistant") {
      continue;
    }
    if (!String(content).trim()) {
      continue;
    }
    turns.push([role, truncateMsg(content)]);
  }
  if (!turns.length) {
    throw new Error(`No user/assistant turns found in ${source}`);
  }
  const selected = turns.slice(-MAX_TURNS);
  let body = `# Claude → Grok session transfer

You are continuing work started in Claude Code. Below is a recent transcript excerpt.
Pick up the highest-value next step and continue until the original goal is met or blocked.
Source: \`${source}\`

## Transcript

`;
  for (const [role, text] of selected) {
    body += `### ${role}\n${text}\n\n`;
  }
  body += `## Instruction\nContinue from the latest assistant state. Prefer the smallest safe next steps.\n`;
  if (body.length > MAX_HANDOFF_CHARS) {
    body = `${body.slice(0, MAX_HANDOFF_CHARS)}\n…[transfer truncated]\n`;
  }
  return { source, prompt: body, turns: selected.length };
}
