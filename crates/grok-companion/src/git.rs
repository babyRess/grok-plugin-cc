use std::path::{Path, PathBuf};

use crate::error::{CompanionError, Result};
use crate::process::run_command;
use crate::workspace::resolve_workspace_root;

const MAX_DIFF_CHARS: usize = 120_000;

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

pub fn collect_review_context(cwd: &Path, target: &ReviewTarget) -> Result<ReviewContext> {
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

    let (status, summary, mut diff) = if target.kind == "branch" {
        let base = target.base.as_deref().unwrap_or("main");
        let (_, log, _) = git(
            &repo_root,
            &["log", "--oneline", &format!("{base}...HEAD"), "-20"],
        )?;
        let (_, shortstat, _) =
            git(&repo_root, &["diff", "--shortstat", &format!("{base}...HEAD")])?;
        let (_, full_diff, _) = git(&repo_root, &["diff", &format!("{base}...HEAD")])?;
        let summary = if shortstat.trim().is_empty() {
            "No branch diff.".into()
        } else {
            shortstat.trim().to_string()
        };
        (log.trim().to_string(), summary, full_diff)
    } else {
        let (_, st, _) = git(
            &repo_root,
            &["status", "--short", "--untracked-files=all"],
        )?;
        let (_, cached, _) = git(&repo_root, &["diff", "--cached"])?;
        let (_, unstaged, _) = git(&repo_root, &["diff"])?;
        let (_, cached_ss, _) = git(&repo_root, &["diff", "--shortstat", "--cached"])?;
        let (_, unstaged_ss, _) = git(&repo_root, &["diff", "--shortstat"])?;
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
        (st.trim().to_string(), summary, d)
    };

    if diff.len() > MAX_DIFF_CHARS {
        diff = format!(
            "{}\n\n...[diff truncated at {MAX_DIFF_CHARS} chars]...",
            &diff[..MAX_DIFF_CHARS]
        );
    }

    Ok(ReviewContext {
        repo_root,
        branch,
        target: target.clone(),
        status,
        summary,
        diff,
    })
}
