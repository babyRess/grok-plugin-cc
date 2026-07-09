use crate::state::{Config, Job};
use serde_json::Value;

pub fn render_setup_report(
    ready: bool,
    node_detail: &str,
    grok_detail: &str,
    logged_in: bool,
    auth_detail: &str,
    config: &Config,
    actions: &[String],
    next_steps: &[String],
) -> String {
    let mut lines = vec![
        "# Grok Companion Setup".into(),
        String::new(),
        format!("Ready: {}", if ready { "yes" } else { "no" }),
        "Runtime: rust".into(),
        format!("Node: {node_detail}"),
        format!("Grok: {grok_detail}"),
        format!(
            "Auth: {} ({auth_detail})",
            if logged_in {
                "logged in"
            } else {
                "not authenticated"
            }
        ),
        format!(
            "Stop review gate: {}",
            if config.stop_review_gate {
                "enabled"
            } else {
                "disabled"
            }
        ),
        String::new(),
    ];

    if !actions.is_empty() {
        lines.push("## Actions taken".into());
        for a in actions {
            lines.push(format!("- {a}"));
        }
        lines.push(String::new());
    }

    if !next_steps.is_empty() {
        lines.push("## Next steps".into());
        for s in next_steps {
            lines.push(format!("- {s}"));
        }
        lines.push(String::new());
    }

    lines.join("\n")
}

pub fn render_status_report(jobs: &[Job]) -> String {
    if jobs.is_empty() {
        return "No Grok companion jobs found for this repository.\n".into();
    }

    let mut lines = vec!["# Grok Companion Jobs".into(), String::new()];
    for job in jobs {
        lines.push(format!(
            "- {}  [{}]  {}",
            job.id,
            job.status.as_deref().unwrap_or("?"),
            job.title.as_deref().unwrap_or("(untitled)")
        ));
        lines.push(format!(
            "  kind={}  updated={}",
            job.kind_label
                .as_deref()
                .or(job.kind.as_deref())
                .unwrap_or("?"),
            job.updated_at.as_deref().unwrap_or("?")
        ));
        if let Some(msg) = &job.progress_message {
            lines.push(format!("  progress: {msg}"));
        }
        if let Some(sid) = &job.grok_session_id {
            lines.push(format!("  grok-session: {sid}"));
        }
        lines.push(String::new());
    }
    lines.join("\n")
}

pub fn render_job_status(job: &Job) -> String {
    let mut lines = vec![
        format!("# Job {}", job.id),
        String::new(),
        format!("Status: {}", job.status.as_deref().unwrap_or("?")),
        format!("Title: {}", job.title.as_deref().unwrap_or("?")),
        format!(
            "Kind: {}",
            job.kind_label
                .as_deref()
                .or(job.kind.as_deref())
                .unwrap_or("?")
        ),
        format!("Created: {}", job.created_at.as_deref().unwrap_or("?")),
        format!("Updated: {}", job.updated_at.as_deref().unwrap_or("?")),
    ];
    if let Some(pid) = job.pid {
        let alive = crate::process::is_process_alive(pid);
        lines.push(format!(
            "PID: {pid} ({})",
            if alive { "alive" } else { "dead" }
        ));
    }
    if let Some(msg) = &job.progress_message {
        lines.push(format!("Progress: {msg}"));
    }
    if let Some(err) = &job.error {
        lines.push(format!("Error: {err}"));
    }
    if let Some(log) = &job.log_file {
        lines.push(format!("Log: {log}"));
    }
    if let Some(result) = &job.result_file {
        lines.push(format!("Result: {result}"));
    }
    lines.push(String::new());
    lines.join("\n")
}

pub fn render_review_result(review_label: &str, target_label: &str, body: &str, exit: i32) -> String {
    format!(
        "# Grok {review_label}\nTarget: {target_label}\nExit: {exit}\n\n{}\n",
        body.trim_end()
    )
}

pub fn render_task_result(title: &str, job_id: Option<&str>, write: bool, body: &str) -> String {
    let mut lines = vec![format!("# {title}")];
    if let Some(id) = job_id {
        lines.push(format!("Job: {id}"));
    }
    lines.push(format!(
        "Mode: {}",
        if write {
            "write-capable"
        } else {
            "read-only"
        }
    ));
    lines.push(String::new());
    lines.push(body.trim_end().to_string());
    lines.push(String::new());
    lines.join("\n")
}

pub fn render_stored_job_result(job: &Job, stored: Option<&Value>) -> String {
    if let Some(stored) = stored {
        if let Some(rendered) = stored.get("rendered").and_then(|v| v.as_str()) {
            let mut header = vec![format!(
                "# Grok result: {}",
                job.title.as_deref().unwrap_or(&job.id)
            )];
            header.push(format!("Job: {}", job.id));
            header.push(String::new());
            header.push("---".into());
            header.push(String::new());
            return format!("{}{}", header.join("\n"), rendered);
        }
    }
    if let Some(err) = &job.error {
        return format!("Job {} failed: {err}\n", job.id);
    }
    if matches!(job.status.as_deref(), Some("running") | Some("queued")) {
        return format!(
            "Job {} is still {}. Check status {}.\n",
            job.id,
            job.status.as_deref().unwrap_or("?"),
            job.id
        );
    }
    format!("No rendered output stored for {}.\n", job.id)
}

pub fn render_cancel_report(job_id: &str, title: &str, canceled: bool, detail: &str) -> String {
    if canceled {
        format!("Canceled job {job_id} ({title}).\n")
    } else {
        format!("Could not cancel job {job_id}: {detail}\n")
    }
}

pub fn first_meaningful_line(text: &str, fallback: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

pub fn json_pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".into())
}

pub fn shorten(text: &str, limit: usize) -> String {
    let normalized: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.len() <= limit {
        return normalized;
    }
    format!("{}...", &normalized[..limit.saturating_sub(3)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorten_trims() {
        assert_eq!(shorten("hello world", 100), "hello world");
        assert!(shorten("abcdefghijklmnopqrstuvwxyz", 10).ends_with("..."));
    }

    #[test]
    fn render_task_result_modes() {
        let w = render_task_result("Grok Task", Some("t1"), true, "done");
        assert!(w.contains("write-capable"));
        let r = render_task_result("Grok Task", None, false, "ok");
        assert!(r.contains("read-only"));
    }
}
