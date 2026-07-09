use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;

use crate::error::{CompanionError, Result};
use crate::git::ReviewContext;
use crate::process::binary_available;

#[derive(Debug, Clone)]
pub struct Availability {
    pub available: bool,
    pub detail: String,
    pub binary: PathBuf,
}

#[derive(Debug, Clone)]
pub struct AuthStatus {
    pub available: bool,
    pub logged_in: bool,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct GrokRunResult {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
    /// Grok session UUID when captured via `--output-format json`.
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct HeadlessOptions {
    pub model: Option<String>,
    pub effort: Option<String>,
    pub write: bool,
    pub continue_latest: bool,
    pub resume_session_id: Option<String>,
    pub max_turns: Option<u32>,
    /// Prefer JSON wire format to capture `sessionId` (default true).
    pub capture_session: bool,
}

pub fn resolve_grok_binary() -> PathBuf {
    if let Ok(from_env) = std::env::var("GROK_BIN") {
        let p = PathBuf::from(&from_env);
        if p.exists() {
            return p;
        }
    }

    if let Ok(path) = which::which("grok") {
        return path;
    }

    let home = directories::UserDirs::new()
        .map(|u| u.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    let fallbacks = [
        home.join(".grok/bin/grok"),
        home.join(".local/bin/grok"),
        PathBuf::from("/opt/homebrew/bin/grok"),
        PathBuf::from("/usr/local/bin/grok"),
    ];
    for candidate in fallbacks {
        if candidate.exists() {
            return candidate;
        }
    }
    PathBuf::from("grok")
}

pub fn get_grok_availability(cwd: Option<&Path>) -> Availability {
    let bin = resolve_grok_binary();
    let bin_str = bin.to_string_lossy();
    let (available, detail) = binary_available(bin_str.as_ref(), &["--version"], cwd);
    Availability {
        available,
        detail,
        binary: bin,
    }
}

pub fn get_grok_login_status(cwd: Option<&Path>) -> AuthStatus {
    let availability = get_grok_availability(cwd);
    if !availability.available {
        return AuthStatus {
            available: false,
            logged_in: false,
            detail: availability.detail,
        };
    }

    let home = directories::UserDirs::new()
        .map(|u| u.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let auth_path = home.join(".grok/auth.json");

    if auth_path.exists() {
        if let Ok(raw) = std::fs::read_to_string(&auth_path) {
            if let Ok(parsed) = serde_json::from_str::<Value>(&raw) {
                let has_creds = parsed.get("access_token").is_some()
                    || parsed.get("token").is_some()
                    || parsed.get("api_key").is_some()
                    || parsed.get("apiKey").is_some()
                    || parsed.get("oauth").is_some()
                    || parsed.as_object().map(|o| !o.is_empty()).unwrap_or(false);
                if has_creds {
                    return AuthStatus {
                        available: true,
                        logged_in: true,
                        detail: format!("credentials present at {}", auth_path.display()),
                    };
                }
            }
        }
    }

    AuthStatus {
        available: true,
        logged_in: false,
        detail: format!(
            "No usable credentials found at {}. Run `grok` and complete login, or set API credentials.",
            auth_path.display()
        ),
    }
}

pub fn normalize_effort(effort: &str) -> Result<String> {
    let normalized = effort.trim().to_ascii_lowercase();
    let allowed = [
        "none", "minimal", "low", "medium", "high", "xhigh", "max",
    ];
    if !allowed.contains(&normalized.as_str()) {
        return Err(CompanionError::msg(format!(
            "Unsupported reasoning effort \"{effort}\". Use one of: {}.",
            allowed.join(", ")
        )));
    }
    Ok(if normalized == "max" {
        "xhigh".into()
    } else {
        normalized
    })
}

pub fn build_review_prompt(context: &ReviewContext) -> String {
    format!(
        r#"You are performing a careful code review. Focus on correctness bugs, security issues, regressions, missing tests, and maintainability problems.

Repository: {}
Branch: {}
Review target: {}
Change summary: {}

Git status / recent commits:
```
{}
```

Diff:
```diff
{}
```

Instructions:
- Do NOT modify files.
- Do NOT apply patches.
- Report findings ordered by severity (critical / high / medium / low).
- For each finding include: title, severity, file path if known, why it matters, and a concrete fix suggestion.
- If there are no material issues, say so clearly and mention residual risks.
- End with a short summary.
"#,
        context.repo_root.display(),
        context.branch,
        context.target.label,
        context.summary,
        empty_or(&context.status, "(empty)"),
        empty_or(
            &context.diff,
            "(no textual diff; check untracked files if status is non-empty)"
        )
    )
}

pub fn build_adversarial_prompt(context: &ReviewContext, focus: &str) -> String {
    format!(
        r#"You are performing an **adversarial** code review. Challenge the chosen implementation and design — not only line-level bugs.

Pressure-test:
- Hidden assumptions and failure modes
- Race conditions, data loss, rollback, auth/authz gaps
- Whether a simpler or safer approach exists
- Missing tests for critical paths

Repository branch: {}
Review target: {}
Change summary: {}

Git status / recent commits:
```
{}
```

Diff:
```diff
{}
```

User focus (if any): {}

Instructions:
- Do NOT modify files or apply patches.
- Order findings by severity (critical / high / medium / low).
- For each finding: title, severity, path if known, why it matters, safer alternative.
- Question design choices even when the code is locally correct.
- End with a short summary of residual risk.
"#,
        context.branch,
        context.target.label,
        context.summary,
        empty_or(&context.status, "(empty)"),
        empty_or(&context.diff, "(no textual diff)"),
        if focus.trim().is_empty() {
            "(none)"
        } else {
            focus
        }
    )
}

fn empty_or<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.trim().is_empty() {
        fallback
    } else {
        value
    }
}

/// Run headless `grok -p` with optional write mode / resume.
///
/// Important: non-interactive runs must auto-approve tool use. Without
/// `--always-approve` / `--yolo`, Grok can block forever on a permission
/// prompt while stdin is closed (looks like a hang after "Spawning…").
pub fn run_grok_headless(
    cwd: &Path,
    prompt: &str,
    options: &HeadlessOptions,
) -> Result<GrokRunResult> {
    if prompt.trim().is_empty() {
        return Err(CompanionError::msg(
            "A prompt is required for this Grok run.",
        ));
    }

    let bin = resolve_grok_binary();
    // Always use JSON wire format so we can capture sessionId and extract `.text`.
    let use_json = true;
    let output_format = "json";
    let _ = options.capture_session;
    let mut args: Vec<String> = vec![
        "-p".into(),
        prompt.into(),
        "--output-format".into(),
        output_format.into(),
        "--cwd".into(),
        cwd.display().to_string(),
        // Required for headless: never wait on interactive tool approval.
        "--always-approve".into(),
    ];

    let default_model = std::env::var("GROK_COMPANION_MODEL").ok();
    if let Some(m) = options
        .model
        .as_deref()
        .or(default_model.as_deref())
        .filter(|s| !s.is_empty())
    {
        args.push("-m".into());
        args.push(m.into());
    }
    if let Some(e) = &options.effort {
        args.push("--effort".into());
        args.push(e.clone());
    }
    if let Some(n) = options.max_turns {
        args.push("--max-turns".into());
        args.push(n.to_string());
    }
    if let Some(sid) = &options.resume_session_id {
        args.push("-r".into());
        args.push(sid.clone());
        eprintln!("[grok] Resuming session {sid}");
    } else if options.continue_latest {
        args.push("-c".into());
        eprintln!("[grok] Continuing latest session in cwd (-c)");
    }

    if options.write {
        // Full auto for write-capable rescue work
        args.push("--yolo".into());
    } else {
        // Read-only: block file mutation tools, but still auto-approve reads
        args.push("--disallowed-tools".into());
        args.push("search_replace,Write,Edit".into());
        args.push("--no-subagents".into());
    }

    let mode = if options.write {
        "write"
    } else {
        "read-only"
    };
    eprintln!(
        "[grok] Spawning {} -p … ({mode}, always-approve, prompt ~{} chars)",
        bin.display(),
        prompt.len()
    );
    eprintln!("[grok] Working directory: {}", cwd.display());
    if prompt.len() > 12_000 {
        eprintln!(
            "[grok] Warning: large prompt ({} chars) — Grok may take longer. Prefer compact Claude context.",
            prompt.len()
        );
    }
    eprintln!("[grok] Waiting for Grok (heartbeat every 5s; Ctrl+C to abort)…");

    let mut child = Command::new(&bin)
        .args(&args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| CompanionError::Grok(format!("failed to spawn grok: {e}")))?;

    let pid = child.id();
    let stop_heartbeat = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_flag = stop_heartbeat.clone();
    let heartbeat = std::thread::spawn(move || {
        let mut secs = 0u64;
        while !stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_secs(5));
            if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            secs += 5;
            eprintln!("[grok] still running… {secs}s (pid {pid})");
        }
    });

    // Stream stderr live so the UI does not look frozen.
    let stderr_handle = child.stderr.take().map(|mut stderr| {
        std::thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            let reader = BufReader::new(&mut stderr);
            let mut collected = String::new();
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        eprintln!("[grok:err] {l}");
                        collected.push_str(&l);
                        collected.push('\n');
                    }
                    Err(_) => break,
                }
            }
            collected
        })
    });

    // Collect stdout (JSON is parsed after exit; avoid dumping raw JSON mid-run).
    let stdout_handle = child.stdout.take().map(|mut stdout| {
        std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = String::new();
            let _ = stdout.read_to_string(&mut buf);
            buf
        })
    });

    let status = child
        .wait()
        .map_err(|e| CompanionError::Grok(format!("wait grok: {e}")))?;

    stop_heartbeat.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = heartbeat.join();

    let stdout = match stdout_handle {
        Some(h) => h.join().unwrap_or_default(),
        None => String::new(),
    };
    let stderr = match stderr_handle {
        Some(h) => h.join().unwrap_or_default(),
        None => String::new(),
    };

    let code = status.code().unwrap_or(1);
    let (display_stdout, session_id) = parse_grok_stdout(&stdout, use_json);
    if let Some(ref sid) = session_id {
        eprintln!("[grok] sessionId={sid}");
    }
    if code == 0 {
        eprintln!("\n[grok] Finished successfully.");
    } else {
        eprintln!("\n[grok] Exited with status {code}.");
    }

    Ok(GrokRunResult {
        status: code,
        stdout: display_stdout,
        stderr,
        session_id,
    })
}

/// Parse plain or JSON grok -p output into (text, session_id).
pub fn parse_grok_stdout(raw: &str, expect_json: bool) -> (String, Option<String>) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return (String::new(), None);
    }
    // Always try JSON first if it looks like an object
    if expect_json || trimmed.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
            let text = v
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or(trimmed)
                .to_string();
            let sid = v
                .get("sessionId")
                .or_else(|| v.get("session_id"))
                .and_then(|s| s.as_str())
                .map(str::to_string);
            return (text, sid);
        }
        // streaming-json: take last JSON object line
        if let Some(line) = trimmed.lines().rev().find(|l| l.trim().starts_with('{')) {
            if let Ok(v) = serde_json::from_str::<Value>(line) {
                let text = v
                    .get("text")
                    .and_then(|t| t.as_str())
                    .unwrap_or(line)
                    .to_string();
                let sid = v
                    .get("sessionId")
                    .or_else(|| v.get("session_id"))
                    .and_then(|s| s.as_str())
                    .map(str::to_string);
                return (text, sid);
            }
        }
    }
    (raw.to_string(), None)
}

pub fn run_grok_review(
    cwd: &Path,
    prompt: &str,
    model: Option<&str>,
    effort: Option<&str>,
) -> Result<GrokRunResult> {
    run_grok_headless(
        cwd,
        prompt,
        &HeadlessOptions {
            model: model.map(str::to_string),
            effort: effort.map(str::to_string),
            write: false,
            ..Default::default()
        },
    )
}

pub fn ensure_grok_ready(cwd: &Path) -> Result<()> {
    let availability = get_grok_availability(Some(cwd));
    if !availability.available {
        return Err(CompanionError::msg(format!(
            "Grok CLI is not available ({}). Install Grok Build, ensure `grok` is on PATH, then run setup.",
            availability.detail
        )));
    }
    let auth = get_grok_login_status(Some(cwd));
    if !auth.logged_in {
        eprintln!("[grok] Warning: {}", auth.detail);
    }
    Ok(())
}

pub fn node_available(cwd: Option<&Path>) -> (bool, String) {
    binary_available("node", &["--version"], cwd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_effort_accepts_max_as_xhigh() {
        assert_eq!(normalize_effort("max").unwrap(), "xhigh");
        assert_eq!(normalize_effort("HIGH").unwrap(), "high");
    }

    #[test]
    fn normalize_effort_rejects_unknown() {
        assert!(normalize_effort("turbo").is_err());
    }

    #[test]
    fn build_review_prompt_includes_branch() {
        let ctx = ReviewContext {
            repo_root: PathBuf::from("/tmp/repo"),
            branch: "feat/x".into(),
            target: crate::git::ReviewTarget {
                kind: "working-tree".into(),
                base: None,
                label: "working tree".into(),
                repo_root: PathBuf::from("/tmp/repo"),
                empty: false,
            },
            status: " M a.rs".into(),
            summary: "1 file changed".into(),
            diff: "+fn main() {}".into(),
        };
        let p = build_review_prompt(&ctx);
        assert!(p.contains("feat/x"));
        assert!(p.contains("Do NOT modify files"));
    }
}
