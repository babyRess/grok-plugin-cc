use std::path::{Path, PathBuf};

use crate::error::{CompanionError, Result};
use crate::process::run_command;
use crate::workspace::resolve_workspace_root;

/// Default full-diff budget (bytes). Large dirty trees get a smarter summary instead.
pub const DEFAULT_MAX_DIFF_CHARS: usize = 40_000;
/// Above this raw diff size, prefer name-status + sampled patches.
const SMART_DIFF_THRESHOLD: usize = 24_000;
const MAX_FILES_LISTED: usize = 80;
const MAX_SAMPLED_FILES: usize = 12;
const MAX_PER_FILE_PATCH: usize = 4_000;

#[derive(Debug, Clone)]
pub struct ReviewTarget {
    pub kind: String, // "working-tree" | "branch"
    pub base: Option<String>,
    pub label: String,
    pub repo_root: PathBuf,
    pub empty: bool,
}

#[derive(Debug, Clone)]
pub struct ReviewContext {
    pub repo_root: PathBuf,
    pub branch: String,
    pub target: ReviewTarget,
    pub status: String,
    pub summary: String,
    pub diff: String,
}

fn git(cwd: &Path, args: &[&str]) -> Result<(i32, String, String)> {
    let result = run_command("git", args, Some(cwd), None)?;
    Ok((result.status, result.stdout, result.stderr))
}

pub fn ensure_git_repository(cwd: &Path) -> Result<PathBuf> {
    let (status, stdout, stderr) = git(cwd, &["rev-parse", "--is-inside-work-tree"])?;
    if status != 0 || stdout.trim() != "true" {
        return Err(CompanionError::msg(format!(
            "Not inside a git repository. {stderr}"
        )));
    }
    Ok(resolve_workspace_root(cwd))
}

pub fn resolve_review_target(
    cwd: &Path,
    base: Option<&str>,
    scope: Option<&str>,
) -> Result<ReviewTarget> {
    let repo_root = ensure_git_repository(cwd)?;
    let scope = scope.unwrap_or("auto");

    if base.is_some() || scope == "branch" {
        let base_ref = base.unwrap_or("main");
        let (st_stat, shortstat, _) = git(
            &repo_root,
            &["diff", "--shortstat", &format!("{base_ref}...HEAD")],
        )?;
        let (_, status, _) = git(
            &repo_root,
            &["status", "--short", "--untracked-files=all"],
        )?;
        let empty = shortstat.trim().is_empty() && status.trim().is_empty() && st_stat == 0;
        return Ok(ReviewTarget {
            kind: "branch".into(),
            base: Some(base_ref.into()),
            label: format!("branch vs {base_ref}"),
            repo_root,
            empty,
        });
    }

    let (_, status, _) = git(
        &repo_root,
        &["status", "--short", "--untracked-files=all"],
    )?;
    let (_, cached, _) = git(&repo_root, &["diff", "--shortstat", "--cached"])?;
    let (_, unstaged, _) = git(&repo_root, &["diff", "--shortstat"])?;
    let empty =
        status.trim().is_empty() && cached.trim().is_empty() && unstaged.trim().is_empty();

    Ok(ReviewTarget {
        kind: "working-tree".into(),
        base: None,
        label: "working tree".into(),
        repo_root,
        empty,
    })
}

/// Collect review context with smart diff truncation for large trees.
pub fn collect_review_context(cwd: &Path, target: &ReviewTarget) -> Result<ReviewContext> {
    collect_review_context_with_limit(cwd, target, DEFAULT_MAX_DIFF_CHARS)
}

pub fn collect_review_context_with_limit(
    cwd: &Path,
    target: &ReviewTarget,
    max_diff_chars: usize,
) -> Result<ReviewContext> {
    let repo_root = if target.repo_root.as_os_str().is_empty() {
        resolve_workspace_root(cwd)
    } else {
        target.repo_root.clone()
    };

    let (_, branch_out, _) = git(&repo_root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let branch = {
        let b = branch_out.trim();
        if b.is_empty() {
            "HEAD".into()
        } else {
            b.to_string()
        }
    };

    let (status, summary, raw_diff, name_status) = if target.kind == "branch" {
        let base = target.base.as_deref().unwrap_or("main");
        let (_, log, _) = git(
            &repo_root,
            &["log", "--oneline", &format!("{base}...HEAD"), "-20"],
        )?;
        let (_, shortstat, _) =
            git(&repo_root, &["diff", "--shortstat", &format!("{base}...HEAD")])?;
        let (_, full_diff, _) = git(&repo_root, &["diff", &format!("{base}...HEAD")])?;
        let (_, names, _) = git(
            &repo_root,
            &["diff", "--name-status", &format!("{base}...HEAD")],
        )?;
        let summary = if shortstat.trim().is_empty() {
            "No branch diff.".into()
        } else {
            shortstat.trim().to_string()
        };
        (log.trim().to_string(), summary, full_diff, names)
    } else {
        let (_, st, _) = git(
            &repo_root,
            &["status", "--short", "--untracked-files=all"],
        )?;
        let (_, cached, _) = git(&repo_root, &["diff", "--cached"])?;
        let (_, unstaged, _) = git(&repo_root, &["diff"])?;
        let (_, cached_ss, _) = git(&repo_root, &["diff", "--shortstat", "--cached"])?;
        let (_, unstaged_ss, _) = git(&repo_root, &["diff", "--shortstat"])?;
        let (_, names_c, _) = git(&repo_root, &["diff", "--name-status", "--cached"])?;
        let (_, names_u, _) = git(&repo_root, &["diff", "--name-status"])?;
        let parts: Vec<&str> = [cached_ss.trim(), unstaged_ss.trim()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();
        let summary = if parts.is_empty() {
            if st.trim().is_empty() {
                "No changes.".into()
            } else {
                "Untracked or status-only changes present.".into()
            }
        } else {
            parts.join(" | ")
        };
        let mut d = String::new();
        if !cached.is_empty() {
            d.push_str(&cached);
        }
        if !unstaged.is_empty() {
            if !d.is_empty() {
                d.push('\n');
            }
            d.push_str(&unstaged);
        }
        let mut names = names_c;
        if !names.is_empty() && !names_u.is_empty() {
            names.push('\n');
        }
        names.push_str(&names_u);
        (st.trim().to_string(), summary, d, names)
    };

    let diff = build_smart_diff(&raw_diff, &name_status, max_diff_chars);

    Ok(ReviewContext {
        repo_root,
        branch,
        target: target.clone(),
        status,
        summary,
        diff,
    })
}

fn build_smart_diff(raw_diff: &str, name_status: &str, max_diff_chars: usize) -> String {
    if raw_diff.is_empty() {
        return String::new();
    }

    // Small enough: use full diff (still hard-capped).
    if raw_diff.len() <= SMART_DIFF_THRESHOLD.min(max_diff_chars) {
        return hard_cap(raw_diff, max_diff_chars);
    }

    // Large: name-status list + sample of first N file patches.
    let mut out = String::new();
    out.push_str(&format!(
        "[smart-diff] Full diff is large ({} chars). Showing name-status + sampled patches.\n\n",
        raw_diff.len()
    ));

    let files: Vec<&str> = name_status
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    out.push_str(&format!("### Changed files ({})\n", files.len()));
    for line in files.iter().take(MAX_FILES_LISTED) {
        out.push_str(line);
        out.push('\n');
    }
    if files.len() > MAX_FILES_LISTED {
        out.push_str(&format!(
            "... {} more files omitted\n",
            files.len() - MAX_FILES_LISTED
        ));
    }
    out.push('\n');

    // Sample patches by splitting raw diff on "diff --git"
    let patches = split_git_patches(raw_diff);
    out.push_str(&format!(
        "### Sampled patches (first {} of {})\n",
        MAX_SAMPLED_FILES.min(patches.len()),
        patches.len()
    ));
    for patch in patches.iter().take(MAX_SAMPLED_FILES) {
        let mut p = (*patch).to_string();
        if p.len() > MAX_PER_FILE_PATCH {
            let mut end = MAX_PER_FILE_PATCH.min(p.len());
            while end > 0 && !p.is_char_boundary(end) {
                end -= 1;
            }
            p.truncate(end);
            p.push_str("\n...[file patch truncated]...\n");
        }
        out.push_str(&p);
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
        if out.len() >= max_diff_chars {
            break;
        }
    }

    hard_cap(&out, max_diff_chars)
}

fn split_git_patches(diff: &str) -> Vec<&str> {
    if diff.is_empty() {
        return Vec::new();
    }
    let mut parts = Vec::new();
    let mut start = 0;
    let bytes = diff.as_bytes();
    let marker = b"diff --git ";
    let mut i = 0;
    while i + marker.len() <= bytes.len() {
        if &bytes[i..i + marker.len()] == marker && i > start {
            parts.push(&diff[start..i]);
            start = i;
        }
        i += 1;
    }
    if start < diff.len() {
        parts.push(&diff[start..]);
    }
    if parts.is_empty() {
        parts.push(diff);
    }
    parts
}

fn hard_cap(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max.min(s.len());
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\n\n...[diff truncated at {max} chars]...",
        &s[..end]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smart_diff_uses_summary_for_large_input() {
        let mut huge = String::from("diff --git a/a.rs b/a.rs\n+line\n");
        while huge.len() < SMART_DIFF_THRESHOLD + 1000 {
            huge.push_str("diff --git a/x.rs b/x.rs\n+more\n");
        }
        let names = "M\ta.rs\nM\tx.rs\n";
        let out = build_smart_diff(&huge, names, DEFAULT_MAX_DIFF_CHARS);
        assert!(out.contains("smart-diff"));
        assert!(out.contains("Changed files"));
        assert!(out.len() <= DEFAULT_MAX_DIFF_CHARS + 80);
    }

    #[test]
    fn small_diff_kept_verbatim() {
        let small = "diff --git a/a.rs b/a.rs\n+hi\n";
        let out = build_smart_diff(small, "M\ta.rs\n", DEFAULT_MAX_DIFF_CHARS);
        assert_eq!(out, small);
    }
}
