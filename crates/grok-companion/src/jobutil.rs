//! Shared job helpers for command modules.

use std::path::Path;

use crate::error::Result;
use crate::state::{generate_job_id, now_iso, upsert_job, write_job_result, Job};
use serde_json::Value;

pub fn new_job(
    prefix: &str,
    kind: &str,
    kind_label: &str,
    title: &str,
    workspace: &Path,
    job_class: &str,
    summary: &str,
    write: bool,
) -> Job {
    Job {
        id: generate_job_id(prefix),
        kind: Some(kind.into()),
        kind_label: Some(kind_label.into()),
        title: Some(title.into()),
        workspace_root: Some(workspace.display().to_string()),
        job_class: Some(job_class.into()),
        summary: Some(summary.into()),
        write: Some(write),
        status: Some("queued".into()),
        phase: Some("queued".into()),
        progress_message: None,
        pid: Some(std::process::id()),
        session_id: std::env::var("GROK_COMPANION_SESSION_ID")
            .ok()
            .or_else(|| std::env::var("CLAUDE_SESSION_ID").ok()),
        grok_session_id: None,
        log_file: None,
        result_file: None,
        exit_code: None,
        error: None,
        created_at: Some(now_iso()),
        updated_at: Some(now_iso()),
        started_at: None,
        finished_at: None,
    }
}

pub fn mark_running(workspace: &Path, job: &mut Job) -> Result<()> {
    job.status = Some("running".into());
    job.phase = Some("running".into());
    job.started_at = Some(now_iso());
    job.pid = Some(std::process::id());
    upsert_job(workspace, job.clone())
}

pub fn finish_job(
    workspace: &Path,
    job: &mut Job,
    exit_status: i32,
    payload: Value,
    rendered: &str,
    summary: &str,
) -> Result<()> {
    finish_job_with_session(workspace, job, exit_status, payload, rendered, summary, None)
}

pub fn finish_job_with_session(
    workspace: &Path,
    job: &mut Job,
    exit_status: i32,
    payload: Value,
    rendered: &str,
    summary: &str,
    grok_session_id: Option<String>,
) -> Result<()> {
    if let Some(ref sid) = grok_session_id {
        job.grok_session_id = Some(sid.clone());
    }
    let result_path = write_job_result(
        workspace,
        &job.id,
        &serde_json::json!({
            "jobId": job.id,
            "exitStatus": exit_status,
            "payload": payload,
            "rendered": rendered,
            "summary": summary,
            "grokSessionId": job.grok_session_id,
            "finishedAt": now_iso()
        }),
    )?;
    job.status = Some(if exit_status == 0 {
        "completed".into()
    } else {
        "failed".into()
    });
    job.phase = job.status.clone();
    job.exit_code = Some(exit_status);
    job.finished_at = Some(now_iso());
    job.result_file = Some(result_path.display().to_string());
    job.progress_message = Some(summary.into());
    if exit_status != 0 {
        job.error = Some(summary.into());
    }
    upsert_job(workspace, job.clone())
}

/// Latest completed/failed task job with a Grok session id, if any.
pub fn latest_task_session(workspace: &Path) -> Option<String> {
    sort_jobs_newest(crate::state::list_jobs(workspace))
        .into_iter()
        .find(|j| {
            (j.job_class.as_deref() == Some("task") || j.kind.as_deref() == Some("task"))
                && j.grok_session_id.as_ref().is_some_and(|s| !s.is_empty())
        })
        .and_then(|j| j.grok_session_id)
}

pub fn is_active(status: Option<&str>) -> bool {
    matches!(status, Some("queued") | Some("running"))
}

pub fn sort_jobs_newest(mut jobs: Vec<Job>) -> Vec<Job> {
    jobs.sort_by(|a, b| {
        b.updated_at
            .as_deref()
            .or(b.created_at.as_deref())
            .unwrap_or("")
            .cmp(
                a.updated_at
                    .as_deref()
                    .or(a.created_at.as_deref())
                    .unwrap_or(""),
            )
    });
    jobs
}
