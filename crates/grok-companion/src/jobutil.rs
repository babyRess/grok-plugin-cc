//! Shared job helpers for command modules.

use std::path::Path;

use crate::error::Result;
use crate::process::is_process_alive;
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

/// Mark a job failed after it was already `running`/`queued` (error path / orphan).
pub fn fail_job(workspace: &Path, job: &mut Job, exit_status: i32, summary: &str) -> Result<()> {
    finish_job_with_session(
        workspace,
        job,
        exit_status,
        serde_json::json!({
            "status": exit_status,
            "error": summary,
            "failedWithoutResult": true
        }),
        &format!("# Job {} failed\n\n{summary}\n", job.id),
        summary,
        job.grok_session_id.clone(),
    )
}

/// If a job claims to be active but its recorded PID is dead, mark it failed.
///
/// This repairs the common failure mode where the worker/Grok process is killed
/// (SIGKILL, parent teardown, CLI abort) before `finish_job` can run, leaving a
/// permanent `status=running` marker with null result/log.
///
/// Returns `true` when the job record was updated.
pub fn reconcile_stale_job(workspace: &Path, job: &mut Job) -> Result<bool> {
    if !is_active(job.status.as_deref()) {
        return Ok(false);
    }
    let Some(pid) = job.pid else {
        return Ok(false);
    };
    if is_process_alive(pid) {
        return Ok(false);
    }

    let mut summary = format!(
        "Worker process PID {pid} is no longer running (died without writing a result)."
    );
    if let Some(log) = &job.log_file {
        summary.push_str(&format!(" Log: {log}"));
    } else {
        // Foreground `task` never sets log_file; this usually means Claude reaped
        // a blocking Bash/subagent while Grok was still running. Detached workers
        // (`task --background`) write `{jobId}.log` and survive parent exit.
        summary.push_str(
            " No worker log (foreground run, or older companion without log capture). \
If this was launched from Claude Code background/rescue, re-run with companion \
`task --background` so a detached worker keeps running after Bash exits.",
        );
    }
    summary.push_str(" Re-run the task, or resume the Grok session if one was created.");

    fail_job(workspace, job, 1, &summary)?;
    Ok(true)
}

/// Reconcile every active job in the workspace and return the refreshed list.
pub fn reconcile_and_list_jobs(workspace: &Path) -> Result<Vec<Job>> {
    let jobs = list_jobs_raw(workspace);
    for mut job in jobs {
        let _ = reconcile_stale_job(workspace, &mut job);
    }
    Ok(sort_jobs_newest(crate::state::list_jobs(workspace)))
}

fn list_jobs_raw(workspace: &Path) -> Vec<Job> {
    crate::state::list_jobs(workspace)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{now_iso, upsert_job, Job};
    use crate::test_env::lock_env;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn reconcile_marks_dead_pid_failed() {
        let _guard = lock_env();
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".git")).unwrap();
        let pdata = dir.path().join("pdata");
        fs::create_dir_all(&pdata).unwrap();
        std::env::set_var("GROK_PLUGIN_DATA", &pdata);

        let mut job = Job {
            id: "task-dead-1".into(),
            kind: Some("task".into()),
            kind_label: Some("rescue".into()),
            title: Some("Dead".into()),
            workspace_root: Some(dir.path().display().to_string()),
            job_class: Some("task".into()),
            summary: Some("s".into()),
            write: Some(true),
            status: Some("running".into()),
            phase: Some("running".into()),
            progress_message: Some("Running Grok task…".into()),
            // PID 0 is never alive in our checker.
            pid: Some(0),
            session_id: None,
            grok_session_id: None,
            log_file: None,
            result_file: None,
            exit_code: None,
            error: None,
            created_at: Some(now_iso()),
            updated_at: Some(now_iso()),
            started_at: Some(now_iso()),
            finished_at: None,
        };
        upsert_job(dir.path(), job.clone()).unwrap();

        let changed = reconcile_stale_job(dir.path(), &mut job).unwrap();
        assert!(changed);
        assert_eq!(job.status.as_deref(), Some("failed"));
        assert!(job.error.as_deref().unwrap_or("").contains("no longer running"));
        assert!(job.result_file.is_some());

        std::env::remove_var("GROK_PLUGIN_DATA");
    }

    #[test]
    fn reconcile_leaves_live_pid_running() {
        let _guard = lock_env();
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".git")).unwrap();
        let pdata = dir.path().join("pdata");
        fs::create_dir_all(&pdata).unwrap();
        std::env::set_var("GROK_PLUGIN_DATA", &pdata);

        let mut job = Job {
            id: "task-live-1".into(),
            kind: Some("task".into()),
            kind_label: Some("rescue".into()),
            title: Some("Live".into()),
            workspace_root: Some(dir.path().display().to_string()),
            job_class: Some("task".into()),
            summary: Some("s".into()),
            write: Some(true),
            status: Some("running".into()),
            phase: Some("running".into()),
            progress_message: Some("Running Grok task…".into()),
            pid: Some(std::process::id()),
            session_id: None,
            grok_session_id: None,
            log_file: None,
            result_file: None,
            exit_code: None,
            error: None,
            created_at: Some(now_iso()),
            updated_at: Some(now_iso()),
            started_at: Some(now_iso()),
            finished_at: None,
        };
        upsert_job(dir.path(), job.clone()).unwrap();

        let changed = reconcile_stale_job(dir.path(), &mut job).unwrap();
        assert!(!changed);
        assert_eq!(job.status.as_deref(), Some("running"));

        std::env::remove_var("GROK_PLUGIN_DATA");
    }
}
