use std::io::Read;
use std::path::PathBuf;

use serde_json::json;

use crate::claude_context::{
    maybe_inject_claude_context_with_detail, ContextDetail,
};
use crate::error::{CompanionError, Result};
use crate::grok::{ensure_grok_ready, normalize_effort, run_grok_headless, HeadlessOptions};
use crate::jobutil::{finish_job, mark_running, new_job};
use crate::render::{first_meaningful_line, json_pretty, render_task_result, shorten};
use crate::state::{list_jobs, upsert_job};
use crate::worker::{
    spawn_detached_worker, write_worker_request, TaskWorkerRequest, WorkerRequest,
};
use crate::workspace::resolve_workspace_root;

const STOP_REVIEW_MARKER: &str = "Run a stop-gate review of the previous Claude turn.";
const DEFAULT_CONTINUE: &str = "Continue from the current session state. Pick the next highest-value step and follow through until the task is resolved.";

pub struct TaskArgs {
    pub prompt: Option<String>,
    pub prompt_file: Option<PathBuf>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub write: bool,
    pub resume_last: bool,
    pub background: bool,
    pub json: bool,
    /// Inject on-disk Claude skills + MCP names into the Grok prompt (default true).
    pub inherit_claude_context: bool,
    /// Full Claude context (descriptions) instead of compact names-only.
    pub inherit_claude_context_full: bool,
    pub cwd: PathBuf,
    /// When set, reuse this job id (task-worker path).
    pub job_id: Option<String>,
}

pub fn run(args: TaskArgs) -> Result<i32> {
    let workspace = resolve_workspace_root(&args.cwd);
    ensure_grok_ready(&args.cwd)?;

    let mut prompt = read_prompt(&args)?;
    let resume_last = args.resume_last;
    if resume_last && prompt.trim().is_empty() {
        prompt = DEFAULT_CONTINUE.into();
    }
    if prompt.trim().is_empty() {
        return Err(CompanionError::msg(
            "Provide a prompt, a prompt file, piped stdin, or use --resume-last.",
        ));
    }

    let effort = match args.effort.as_deref() {
        Some(e) => Some(normalize_effort(e)?),
        None => None,
    };

    let is_stop = prompt.contains(STOP_REVIEW_MARKER);
    let title = if is_stop {
        "Grok Stop Gate Review"
    } else if resume_last {
        "Grok Resume"
    } else {
        "Grok Task"
    };

    let mut job = if let Some(id) = &args.job_id {
        let mut j = new_job(
            "task",
            "task",
            "rescue",
            title,
            &workspace,
            "task",
            &shorten(&prompt, 96),
            args.write,
        );
        j.id = id.clone();
        j
    } else {
        new_job(
            "task",
            "task",
            "rescue",
            title,
            &workspace,
            "task",
            &shorten(&prompt, 96),
            args.write,
        )
    };
    upsert_job(&workspace, job.clone())?;

    if args.background {
        let payload = WorkerRequest::Task {
            request: TaskWorkerRequest {
                cwd: args.cwd.clone(),
                prompt: prompt.clone(),
                model: args.model.clone(),
                effort: effort.clone(),
                write: args.write,
                resume_last,
                job_id: job.id.clone(),
                inherit_claude_context: args.inherit_claude_context,
                inherit_claude_context_full: args.inherit_claude_context_full,
            },
        };
        write_worker_request(&workspace, &job.id, &payload)?;
        let pid = spawn_detached_worker(&args.cwd, &job.id)?;
        job.pid = Some(pid);
        job.status = Some("queued".into());
        upsert_job(&workspace, job.clone())?;
        let message = format!(
            "{title} started in the background as {}. Check status {} for progress.\n",
            job.id, job.id
        );
        if args.json {
            println!(
                "{}",
                json_pretty(&json!({
                    "jobId": job.id,
                    "status": "queued",
                    "title": title
                }))
            );
        } else {
            print!("{message}");
        }
        return Ok(0);
    }

    execute_task(
        &mut job,
        &workspace,
        &prompt,
        args.model.as_deref(),
        effort.as_deref(),
        args.write,
        resume_last,
        args.json,
        title,
        args.inherit_claude_context,
        args.inherit_claude_context_full,
        &args.cwd,
    )
}

pub fn execute_task(
    job: &mut crate::state::Job,
    workspace: &std::path::Path,
    prompt: &str,
    model: Option<&str>,
    effort: Option<&str>,
    write: bool,
    resume_last: bool,
    json: bool,
    title: &str,
    inherit_claude_context: bool,
    inherit_claude_context_full: bool,
    cwd: &std::path::Path,
) -> Result<i32> {
    mark_running(workspace, job)?;
    job.progress_message = Some("Running Grok task…".into());
    upsert_job(workspace, job.clone())?;

    let detail = if inherit_claude_context_full {
        ContextDetail::Full
    } else {
        ContextDetail::Compact
    };
    let prompt =
        maybe_inject_claude_context_with_detail(prompt, cwd, inherit_claude_context, detail);

    let result = run_grok_headless(
        workspace,
        &prompt,
        &HeadlessOptions {
            model: model.map(str::to_string),
            effort: effort.map(str::to_string),
            write,
            continue_latest: resume_last,
            ..Default::default()
        },
    )?;

    let raw = result.stdout.trim();
    let body = if raw.is_empty() {
        result.stderr.trim()
    } else {
        raw
    };
    let failure = if result.status == 0 {
        String::new()
    } else {
        result.stderr.clone()
    };
    let rendered = render_task_result(title, Some(&job.id), write, body);
    let summary = first_meaningful_line(
        body,
        &first_meaningful_line(&failure, &format!("{title} finished.")),
    );

    finish_job(
        workspace,
        job,
        result.status,
        json!({
            "status": result.status,
            "rawOutput": result.stdout,
            "stderr": result.stderr,
            "write": write,
            "resumeLast": resume_last
        }),
        &rendered,
        &summary,
    )?;

    if json {
        println!(
            "{}",
            json_pretty(&json!({
                "status": result.status,
                "rawOutput": result.stdout,
                "stderr": result.stderr,
                "jobId": job.id
            }))
        );
    } else {
        print!("{rendered}");
    }

    Ok(if result.status == 0 { 0 } else { 1 })
}

fn read_prompt(args: &TaskArgs) -> Result<String> {
    if let Some(path) = &args.prompt_file {
        return Ok(std::fs::read_to_string(path)?);
    }
    if let Some(p) = &args.prompt {
        if !p.is_empty() {
            return Ok(p.clone());
        }
    }
    // piped stdin
    if !atty_stdin() {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        return Ok(buf);
    }
    Ok(String::new())
}

fn atty_stdin() -> bool {
    // Avoid extra dependency; treat non-tty as piped when is_terminal is false.
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}

/// Whether a resumable task exists for this workspace.
pub fn resume_candidate(cwd: &std::path::Path) -> Option<crate::state::Job> {
    let workspace = resolve_workspace_root(cwd);
    let jobs = crate::jobutil::sort_jobs_newest(list_jobs(&workspace));
    jobs.into_iter().find(|j| {
        (j.job_class.as_deref() == Some("task") || j.kind.as_deref() == Some("task"))
            && matches!(
                j.status.as_deref(),
                Some("completed") | Some("failed") | Some("running")
            )
    })
}
