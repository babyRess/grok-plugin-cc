//! Transfer Claude Code session transcripts into a Grok handoff prompt.
//!
//! Claude stores sessions under `~/.claude/projects/<encoded-cwd>/*.jsonl`.
//! We extract recent user/assistant turns and start a Grok task with that context.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{CompanionError, Result};
use crate::workspace::resolve_workspace_root;

const MAX_TURNS: usize = 24;
const MAX_CHARS_PER_MSG: usize = 4_000;
const MAX_HANDOFF_CHARS: usize = 28_000;

#[derive(Debug, Clone)]
pub struct TransferResult {
    pub source: PathBuf,
    pub prompt: String,
    pub turns: usize,
}

/// Encode a path the way Claude Code names project dirs (roughly).
fn encode_project_dir(path: &Path) -> String {
    let abs = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf());
    abs.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn claude_projects_root() -> PathBuf {
    directories::UserDirs::new()
        .map(|u| u.home_dir().join(".claude/projects"))
        .unwrap_or_else(|| PathBuf::from(".claude/projects"))
}

/// Find the most recently modified `.jsonl` under the Claude project for `cwd`.
pub fn find_latest_claude_session(cwd: &Path) -> Result<PathBuf> {
    let workspace = resolve_workspace_root(cwd);
    let projects = claude_projects_root();
    if !projects.is_dir() {
        return Err(CompanionError::msg(format!(
            "No Claude projects directory at {}. Open a Claude Code session first.",
            projects.display()
        )));
    }

    let encoded = encode_project_dir(&workspace);
    let mut candidates: Vec<PathBuf> = Vec::new();

    // Exact-ish match first
    let preferred = projects.join(&encoded);
    if preferred.is_dir() {
        collect_jsonl(&preferred, &mut candidates);
    }

    // Fallback: any project dir containing workspace basename
    if candidates.is_empty() {
        let base = workspace
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if let Ok(entries) = fs::read_dir(&projects) {
            for ent in entries.flatten() {
                let name = ent.file_name().to_string_lossy().into_owned();
                if !base.is_empty() && name.contains(base) {
                    collect_jsonl(&ent.path(), &mut candidates);
                }
            }
        }
    }

    // Last resort: newest jsonl under all projects (can be wrong repo)
    if candidates.is_empty() {
        if let Ok(entries) = fs::read_dir(&projects) {
            for ent in entries.flatten() {
                collect_jsonl(&ent.path(), &mut candidates);
            }
        }
    }

    candidates.sort_by_key(|p| {
        fs::metadata(p)
            .and_then(|m| m.modified())
            .ok()
            .map(std::cmp::Reverse)
    });

    candidates.into_iter().next().ok_or_else(|| {
        CompanionError::msg(format!(
            "No Claude session jsonl found for workspace {} under {}.",
            workspace.display(),
            projects.display()
        ))
    })
}

fn collect_jsonl(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for ent in entries.flatten() {
        let p = ent.path();
        if p.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            out.push(p);
        }
    }
}

/// Build a Grok handoff prompt from a Claude transcript jsonl.
pub fn build_transfer_prompt(source: &Path) -> Result<TransferResult> {
    let raw = fs::read_to_string(source).map_err(|e| {
        CompanionError::msg(format!("Failed to read {}: {e}", source.display()))
    })?;

    let mut turns: Vec<(String, String)> = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        // Common Claude Code shapes
        // Prefer nested message envelope (Claude Code jsonl)
        let msg = v.get("message").unwrap_or(&v);
        let role_raw = msg
            .get("role")
            .or_else(|| v.get("role"))
            .or_else(|| v.get("type"))
            .and_then(|r| r.as_str())
            .unwrap_or("");
        let role = match role_raw {
            "user" | "human" => "user",
            "assistant" | "ai" => "assistant",
            _ => continue,
        };
        let content = extract_content(msg);
        if content.trim().is_empty() {
            continue;
        }
        turns.push((role.into(), truncate_msg(&content)));
    }

    if turns.is_empty() {
        return Err(CompanionError::msg(format!(
            "No user/assistant turns found in {}.",
            source.display()
        )));
    }

    // Keep last N turns
    let start = turns.len().saturating_sub(MAX_TURNS);
    let selected = &turns[start..];

    let mut body = String::new();
    body.push_str("# Claude → Grok session transfer\n\n");
    body.push_str("You are continuing work started in Claude Code. Below is a recent transcript excerpt.\n");
    body.push_str("Pick up the highest-value next step and continue until the original goal is met or blocked.\n");
    body.push_str(&format!("Source: `{}`\n\n", source.display()));
    body.push_str("## Transcript\n\n");
    for (role, text) in selected {
        body.push_str(&format!("### {role}\n{text}\n\n"));
    }
    body.push_str("## Instruction\nContinue from the latest assistant state. Prefer the smallest safe next steps.\n");

    if body.len() > MAX_HANDOFF_CHARS {
        let mut end = MAX_HANDOFF_CHARS;
        while end > 0 && !body.is_char_boundary(end) {
            end -= 1;
        }
        body.truncate(end);
        body.push_str("\n…[transfer truncated]\n");
    }

    Ok(TransferResult {
        source: source.to_path_buf(),
        prompt: body,
        turns: selected.len(),
    })
}

fn extract_content(v: &serde_json::Value) -> String {
    if let Some(s) = v.get("content").and_then(|c| c.as_str()) {
        return s.to_string();
    }
    if let Some(arr) = v.get("content").and_then(|c| c.as_array()) {
        let mut parts = Vec::new();
        for item in arr {
            if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
                parts.push(t.to_string());
            } else if let Some(t) = item.as_str() {
                parts.push(t.to_string());
            }
        }
        return parts.join("\n");
    }
    if let Some(s) = v.get("text").and_then(|t| t.as_str()) {
        return s.to_string();
    }
    String::new()
}

fn truncate_msg(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() <= MAX_CHARS_PER_MSG {
        return s.to_string();
    }
    let truncated: String = s.chars().take(MAX_CHARS_PER_MSG).collect();
    format!("{truncated}\n…[message truncated]…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn build_transfer_from_jsonl() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("sess.jsonl");
        fs::write(
            &path,
            r#"{"type":"user","message":{"role":"user","content":"fix the bug"}}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"I will investigate."}]}}
"#,
        )
        .unwrap();
        let t = build_transfer_prompt(&path).unwrap();
        assert!(t.prompt.contains("fix the bug"));
        assert!(t.prompt.contains("investigate"));
        assert_eq!(t.turns, 2);
    }
}
