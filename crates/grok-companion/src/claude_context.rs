//! Discover on-disk Claude Code skills + MCP config and format a prompt block
//! for Grok headless runs (`--inherit-claude-context`).

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::workspace::resolve_workspace_root;

const MAX_SKILLS: usize = 80;
const MAX_MCP: usize = 40;
const MAX_DESC_CHARS: usize = 80;
/// Hard cap on injected block size so Grok is not flooded (was ~22k chars).
const MAX_BLOCK_CHARS: usize = 3500;

#[derive(Debug, Clone)]
pub struct SkillEntry {
    pub name: String,
    #[allow(dead_code)]
    pub path: PathBuf,
    pub description: Option<String>,
    #[allow(dead_code)]
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct McpEntry {
    pub name: String,
    pub kind: String,
    pub detail: String,
    #[allow(dead_code)]
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContextDetail {
    /// Compact names-only block (default). Keeps prompts small/fast.
    #[default]
    Compact,
    /// Include short descriptions / command lines.
    Full,
}

#[derive(Debug, Clone, Default)]
pub struct ClaudeContext {
    pub skills: Vec<SkillEntry>,
    pub mcp_servers: Vec<McpEntry>,
    pub notes: Vec<String>,
}

impl ClaudeContext {
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty() && self.mcp_servers.is_empty()
    }

    pub fn to_prompt_block_with_detail(&self, detail: ContextDetail) -> String {
        if self.is_empty() {
            return String::new();
        }

        let mut lines = vec![
            "## Claude Code context (on-disk, compact)".to_string(),
            "Not the live Claude session. Names only — do not open every skill file.".to_string(),
            "MCP listed here is Claude's config; call tools only if Grok has the same MCP.".to_string(),
            String::new(),
        ];

        // MCP first — usually what users care about for "what MCP do you see?"
        if !self.mcp_servers.is_empty() {
            lines.push(format!("### MCP servers ({})", self.mcp_servers.len()));
            match detail {
                ContextDetail::Compact => {
                    let names: Vec<String> = self
                        .mcp_servers
                        .iter()
                        .map(|m| format!("`{}`({})", m.name, m.kind))
                        .collect();
                    lines.push(names.join(", "));
                }
                ContextDetail::Full => {
                    for m in &self.mcp_servers {
                        lines.push(format!("- `{}` [{}] {}", m.name, m.kind, m.detail));
                    }
                }
            }
            lines.push(String::new());
        }

        if !self.skills.is_empty() {
            lines.push(format!("### Skills ({})", self.skills.len()));
            match detail {
                ContextDetail::Compact => {
                    let names: Vec<String> =
                        self.skills.iter().map(|s| format!("`{}`", s.name)).collect();
                    // wrap roughly
                    let mut row = String::new();
                    for (i, n) in names.iter().enumerate() {
                        if !row.is_empty() {
                            row.push_str(", ");
                        }
                        row.push_str(n);
                        if row.len() > 100 || i + 1 == names.len() {
                            lines.push(row.clone());
                            row.clear();
                        }
                    }
                }
                ContextDetail::Full => {
                    for s in &self.skills {
                        let desc = s
                            .description
                            .as_deref()
                            .map(|d| format!(" — {d}"))
                            .unwrap_or_default();
                        lines.push(format!("- `{}`{}", s.name, desc));
                    }
                }
            }
            lines.push(String::new());
        }

        if !self.notes.is_empty() {
            for n in &self.notes {
                lines.push(format!("Note: {n}"));
            }
            lines.push(String::new());
        }

        let mut block = lines.join("\n");
        if block.len() > MAX_BLOCK_CHARS {
            // truncate at a UTF-8 char boundary (byte index must not split a multi-byte char)
            let mut end = MAX_BLOCK_CHARS.min(block.len());
            while end > 0 && !block.is_char_boundary(end) {
                end -= 1;
            }
            block.truncate(end);
            block.push_str("\n…[claude context truncated]\n");
        }
        block
    }
}

/// Collect Claude skills + MCP for a workspace cwd.
pub fn collect_claude_context(cwd: &Path) -> ClaudeContext {
    let mut ctx = ClaudeContext::default();
    let home = directories::UserDirs::new()
        .map(|u| u.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let workspace = resolve_workspace_root(cwd);

    // Skills
    collect_skills_dir(
        &home.join(".claude/skills"),
        "user:~/.claude/skills",
        &mut ctx.skills,
    );
    collect_skills_dir(
        &workspace.join(".claude/skills"),
        "project:.claude/skills",
        &mut ctx.skills,
    );
    // Also walk parents between cwd and workspace for nested .claude/skills
    collect_skills_along_path(cwd, &workspace, &mut ctx.skills);

    // Flat commands → skill-like entries
    collect_commands_dir(
        &home.join(".claude/commands"),
        "user:~/.claude/commands",
        &mut ctx.skills,
    );
    collect_commands_dir(
        &workspace.join(".claude/commands"),
        "project:.claude/commands",
        &mut ctx.skills,
    );

    // Dedupe by name (prefer project over user)
    dedupe_skills(&mut ctx.skills);
    if ctx.skills.len() > MAX_SKILLS {
        let overflow = ctx.skills.len() - MAX_SKILLS;
        ctx.skills.truncate(MAX_SKILLS);
        ctx.notes.push(format!(
            "Skill list truncated; {overflow} more skill(s) omitted."
        ));
    }

    // MCP
    collect_mcp_from_json_file(
        &home.join(".claude.json"),
        "user:~/.claude.json",
        &mut ctx.mcp_servers,
    );
    collect_mcp_from_json_file(
        &home.join(".claude/settings.json"),
        "user:~/.claude/settings.json",
        &mut ctx.mcp_servers,
    );
    collect_mcp_from_json_file(
        &home.join(".claude/settings.local.json"),
        "user:~/.claude/settings.local.json",
        &mut ctx.mcp_servers,
    );
    collect_mcp_from_json_file(
        &workspace.join(".claude/settings.json"),
        "project:.claude/settings.json",
        &mut ctx.mcp_servers,
    );
    collect_mcp_from_json_file(
        &workspace.join(".claude/settings.local.json"),
        "project:.claude/settings.local.json",
        &mut ctx.mcp_servers,
    );
    collect_mcp_from_json_file(
        &workspace.join(".mcp.json"),
        "project:.mcp.json",
        &mut ctx.mcp_servers,
    );

    dedupe_mcp(&mut ctx.mcp_servers);
    if ctx.mcp_servers.len() > MAX_MCP {
        let overflow = ctx.mcp_servers.len() - MAX_MCP;
        ctx.mcp_servers.truncate(MAX_MCP);
        ctx.notes.push(format!(
            "MCP list truncated; {overflow} more server(s) omitted."
        ));
    }

    if ctx.is_empty() {
        ctx.notes.push(
            "No Claude skills or MCP servers found under ~/.claude or project .claude/.".into(),
        );
    }

    ctx
}

pub fn maybe_inject_claude_context_with_detail(
    prompt: &str,
    cwd: &Path,
    enabled: bool,
    detail: ContextDetail,
) -> String {
    if !enabled {
        return prompt.to_string();
    }
    let ctx = collect_claude_context(cwd);
    let block = ctx.to_prompt_block_with_detail(detail);
    if block.trim().is_empty() {
        return prompt.to_string();
    }
    eprintln!(
        "[grok] Claude context: {} MCP, {} skills, ~{} chars ({:?})",
        ctx.mcp_servers.len(),
        ctx.skills.len(),
        block.len(),
        detail
    );
    format!("{block}\n---\n\n{prompt}")
}

fn collect_skills_along_path(cwd: &Path, workspace: &Path, out: &mut Vec<SkillEntry>) {
    let mut current = cwd.to_path_buf();
    loop {
        let skills = current.join(".claude/skills");
        if skills.is_dir() && current != workspace {
            let label = format!("path:{}/.claude/skills", current.display());
            collect_skills_dir(&skills, &label, out);
        }
        if current == workspace || !current.pop() {
            break;
        }
    }
}

fn collect_skills_dir(dir: &Path, source: &str, out: &mut Vec<SkillEntry>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let skill_md = path.join("SKILL.md");
            if skill_md.is_file() {
                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                let description = parse_skill_description(&skill_md);
                out.push(SkillEntry {
                    name,
                    path: skill_md,
                    description,
                    source: source.into(),
                });
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            // rare: flat skill files
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            out.push(SkillEntry {
                name,
                path: path.clone(),
                description: parse_skill_description(&path),
                source: source.into(),
            });
        }
    }
}

fn collect_commands_dir(dir: &Path, source: &str, out: &mut Vec<SkillEntry>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        // avoid duplicating if already present as skill
        if out.iter().any(|s| s.name == name) {
            continue;
        }
        out.push(SkillEntry {
            name,
            path: path.clone(),
            description: parse_skill_description(&path),
            source: source.into(),
        });
    }
}

fn parse_skill_description(path: &Path) -> Option<String> {
    let raw = fs::read_to_string(path).ok()?;
    // YAML frontmatter description:
    if let Some(rest) = raw.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            let fm = &rest[..end];
            for line in fm.lines() {
                let line = line.trim();
                if let Some(val) = line.strip_prefix("description:") {
                    let v = val.trim().trim_matches('"').trim_matches('\'');
                    if !v.is_empty() {
                        return Some(truncate(v, MAX_DESC_CHARS));
                    }
                }
            }
        }
    }
    // fallback: first non-empty non-heading line
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') || t == "---" {
            continue;
        }
        if t.starts_with("name:") || t.starts_with("description:") {
            continue;
        }
        return Some(truncate(t, MAX_DESC_CHARS));
    }
    None
}

fn collect_mcp_from_json_file(path: &Path, source: &str, out: &mut Vec<McpEntry>) {
    let Ok(raw) = fs::read_to_string(path) else {
        return;
    };
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return;
    };
    let servers = value
        .get("mcpServers")
        .or_else(|| value.get("mcp_servers"))
        .cloned()
        .unwrap_or(Value::Null);

    // Some Claude configs nest under projects; also scan top-level only for now.
    if let Some(map) = servers.as_object() {
        for (name, cfg) in map {
            out.push(mcp_entry_from_value(name, cfg, source));
        }
    }
}

fn mcp_entry_from_value(name: &str, cfg: &Value, source: &str) -> McpEntry {
    let (kind, detail) = if let Some(cmd) = cfg.get("command").and_then(|v| v.as_str()) {
        let args = cfg
            .get("args")
            .and_then(|a| a.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        let detail = if args.is_empty() {
            cmd.to_string()
        } else {
            format!("{cmd} {args}")
        };
        ("stdio", truncate(&detail, 200))
    } else if let Some(url) = cfg.get("url").and_then(|v| v.as_str()) {
        ("http", truncate(url, 200))
    } else if let Some(url) = cfg.get("serverUrl").and_then(|v| v.as_str()) {
        ("http", truncate(url, 200))
    } else {
        ("unknown", "configured".into())
    };

    McpEntry {
        name: name.to_string(),
        kind: kind.into(),
        detail,
        source: source.into(),
    }
}

fn dedupe_skills(skills: &mut Vec<SkillEntry>) {
    // Prefer later entries (project often collected after user — reverse scan keep first of reverse)
    // We collected user first then project; prefer project: walk reverse and keep first name.
    let mut seen = std::collections::HashSet::new();
    let mut kept = Vec::new();
    for s in skills.drain(..).rev() {
        if seen.insert(s.name.clone()) {
            kept.push(s);
        }
    }
    kept.reverse();
    *skills = kept;
    skills.sort_by(|a, b| a.name.cmp(&b.name));
}

fn dedupe_mcp(servers: &mut Vec<McpEntry>) {
    let mut seen = std::collections::HashSet::new();
    let mut kept = Vec::new();
    for s in servers.drain(..).rev() {
        if seen.insert(s.name.clone()) {
            kept.push(s);
        }
    }
    kept.reverse();
    *servers = kept;
    servers.sort_by(|a, b| a.name.cmp(&b.name));
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{truncated}…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env::lock_env;
    use tempfile::tempdir;

    #[test]
    fn collect_skills_and_mcp_from_fake_claude_home() {
        let _guard = lock_env();
        let dir = tempdir().unwrap();
        let home = dir.path().join("home");
        let project = dir.path().join("project");
        fs::create_dir_all(project.join(".git")).unwrap();
        fs::create_dir_all(project.join(".claude/skills/demo-skill")).unwrap();
        fs::write(
            project.join(".claude/skills/demo-skill/SKILL.md"),
            "---\nname: demo-skill\ndescription: Demo skill for tests\n---\n\n# Demo\n",
        )
        .unwrap();
        fs::create_dir_all(home.join(".claude")).unwrap();
        fs::write(
            home.join(".claude.json"),
            r#"{"mcpServers":{"fake-mcp":{"command":"npx","args":["-y","fake"]}}}"#,
        )
        .unwrap();

        // Point HOME so directories::UserDirs might still use real home —
        // call collectors directly instead.
        let mut ctx = ClaudeContext::default();
        collect_skills_dir(
            &project.join(".claude/skills"),
            "project",
            &mut ctx.skills,
        );
        collect_mcp_from_json_file(&home.join(".claude.json"), "user", &mut ctx.mcp_servers);
        assert_eq!(ctx.skills.len(), 1);
        assert_eq!(ctx.skills[0].name, "demo-skill");
        assert!(ctx.skills[0]
            .description
            .as_deref()
            .unwrap_or("")
            .contains("Demo skill"));
        assert_eq!(ctx.mcp_servers.len(), 1);
        assert_eq!(ctx.mcp_servers[0].name, "fake-mcp");

        let block = ctx.to_prompt_block_with_detail(ContextDetail::Compact);
        assert!(block.contains("demo-skill"));
        assert!(block.contains("fake-mcp"));
        assert!(block.contains("Claude Code context"));
        // compact should stay small
        assert!(block.len() < 800, "block too large: {}", block.len());
    }

    #[test]
    fn inject_disabled_returns_prompt() {
        let block = maybe_inject_claude_context_with_detail(
            "USER TASK",
            Path::new("/nonexistent/path"),
            false,
            ContextDetail::Compact,
        );
        assert_eq!(block, "USER TASK");
    }

    #[test]
    fn compact_block_is_much_smaller_than_full() {
        let mut ctx = ClaudeContext::default();
        for i in 0..50 {
            ctx.skills.push(SkillEntry {
                name: format!("skill-{i}"),
                path: PathBuf::from(format!("/tmp/skill-{i}/SKILL.md")),
                description: Some("x".repeat(100)),
                source: "user".into(),
            });
        }
        for i in 0..15 {
            ctx.mcp_servers.push(McpEntry {
                name: format!("mcp-{i}"),
                kind: "stdio".into(),
                detail: "npx -y long-package-name-here".into(),
                source: "user".into(),
            });
        }
        let compact = ctx.to_prompt_block_with_detail(ContextDetail::Compact);
        let full = ctx.to_prompt_block_with_detail(ContextDetail::Full);
        assert!(compact.len() < full.len());
        assert!(compact.len() <= MAX_BLOCK_CHARS + 40); // allow truncation suffix
    }

    #[test]
    fn truncate_respects_utf8_boundaries() {
        let mut ctx = ClaudeContext::default();
        // Multi-byte chars so a naive byte truncate would panic
        let desc = "日本語スキル説明🔥".repeat(200);
        for i in 0..30 {
            ctx.skills.push(SkillEntry {
                name: format!("skill-{i}"),
                path: PathBuf::from(format!("/tmp/{i}")),
                description: Some(desc.clone()),
                source: "user".into(),
            });
        }
        // Must not panic
        let block = ctx.to_prompt_block_with_detail(ContextDetail::Full);
        assert!(block.contains("truncated") || block.len() <= MAX_BLOCK_CHARS + 50);
        // Valid UTF-8
        assert!(std::str::from_utf8(block.as_bytes()).is_ok());
    }
}
