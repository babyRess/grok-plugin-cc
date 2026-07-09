use std::path::PathBuf;

use serde_json::json;

use crate::claude_context::{
    maybe_inject_claude_context_with_detail, ContextDetail,
};
use crate::error::Result;
use crate::git::{collect_review_context, resolve_review_target};
use crate::grok::{
    build_adversarial_prompt, build_review_prompt, ensure_grok_ready, normalize_effort,
    run_grok_review,
};
use crate::jobutil::{finish_job, finish_job_with_session, mark_running, new_job};
use crate::render::{first_meaningful_line, json_pretty, render_review_result, shorten};
use crate::state::upsert_job;
use crate::worker::{
    spawn_detached_worker, write_worker_request, ReviewWorkerRequest, WorkerRequest,
};
use crate::workspace::resolve_workspace_root;

pub struct ReviewArgs {
    pub base: Option<String>,
    pub scope: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub focus_text: Option<String>,
    pub adversarial: bool,
    pub background: bool,
    pub json: bool,
    /// Inject on-disk Claude skills + MCP names into the Grok prompt (default true).
    pub inherit_claude_context: bool,
    pub inherit_claude_context_full: bool,
    pub cwd: PathBuf,
}

pub fn run(args: ReviewArgs) -> Result<i32> {
    let workspace = resolve_workspace_root(&args.cwd);
    ensure_grok_ready(&args.cwd)?;

    let effort = match args.effort.as_deref() {
        Some(e) => Some(normalize_effort(e)?),
        None => None,
    };

    let target = resolve_review_target(
        &args.cwd,
        args.base.as_deref(),
        args.scope.as_deref(),
    )?;

    let review_name = if args.adversarial {
        "Adversarial Review"
    } else {
        "Review"
    };
    let kind = if args.adversarial {
        "adversarial-review"
    } else {
        "review"
    };
    let focus = args.focus_text.clone().unwrap_or_default();
    let summary = format!(
        "{review_name} {}{}",
        target.label,
        if focus.is_empty() {
            String::new()
        } else {
            format!(": {}", shorten(&focus, 48))
        }
    );

    let mut job = new_job(
        if args.adversarial { "adv" } else { "rev" },
        kind,
        if args.adversarial {
            "adversarial-review"
        } else {
            "review"
        },
        &format!("Grok {review_name}"),
        &workspace,
        "review",
        &summary,
        false,
    );
    upsert_job(&workspace, job.clone())?;

    if args.background {
        let req = ReviewWorkerRequest {
            cwd: args.cwd.clone(),
            base: args.base.clone(),
            scope: args.scope.clone(),
            model: args.model.clone(),
            effort: effort.clone(),
            focus_text: args.focus_text.clone(),
            adversarial: args.adversarial,
            inherit_claude_context: args.inherit_claude_context,
            inherit_claude_context_full: args.inherit_claude_context_full,
        };
        let payload = if args.adversarial {
            WorkerRequest::AdversarialReview { request: req }
        } else {
            WorkerRequest::Review { request: req }
        };
        write_worker_request(&workspace, &job.id, &payload)?;
        let pid = spawn_detached_worker(&args.cwd, &job.id)?;
        job.pid = Some(pid);
        job.status = Some("queued".into());
        job.phase = Some("queued".into());
        upsert_job(&workspace, job.clone())?;
        let message = format!(
            "{} started in the background as {}. Check status {} for progress.\n",
            job.title.as_deref().unwrap_or("Review"),
            job.id,
            job.id
        );
        if args.json {
            println!(
                "{}",
                json_pretty(&json!({
                    "jobId": job.id,
                    "status": "queued",
                    "title": job.title
                }))
            );
        } else {
            print!("{message}");
        }
        return Ok(0);
    }

    execute_review(&mut job, &workspace, &args, effort.as_deref())
}

pub fn execute_review(
    job: &mut crate::state::Job,
    workspace: &std::path::Path,
    args: &ReviewArgs,
    effort: Option<&str>,
) -> Result<i32> {
    mark_running(workspace, job)?;

    let target = resolve_review_target(
        &args.cwd,
        args.base.as_deref(),
        args.scope.as_deref(),
    )?;
    let review_name = if args.adversarial {
        "Adversarial Review"
    } else {
        "Review"
    };
    let focus = args.focus_text.clone().unwrap_or_default();

    if target.empty && focus.is_empty() {
        let message = format!("Nothing to review for {}.", target.label);
        let rendered = format!("{message}\n");
        finish_job(
            workspace,
            job,
            0,
            json!({ "empty": true, "message": message, "target": target.label }),
            &rendered,
            &message,
        )?;
        if args.json {
            println!(
                "{}",
                json_pretty(&json!({
                    "review": review_name,
                    "empty": true,
                    "message": message
                }))
            );
        } else {
            print!("{rendered}");
        }
        return Ok(0);
    }

    let context = collect_review_context(&args.cwd, &target)?;
    let mut prompt = if args.adversarial {
        build_adversarial_prompt(&context, &focus)
    } else {
        build_review_prompt(&context)
    };
    let detail = if args.inherit_claude_context_full {
        ContextDetail::Full
    } else {
        ContextDetail::Compact
    };
    prompt = maybe_inject_claude_context_with_detail(
        &prompt,
        &args.cwd,
        args.inherit_claude_context,
        detail,
    );

    job.progress_message = Some("Running Grok review…".into());
    upsert_job(workspace, job.clone())?;

    let result = run_grok_review(
        &context.repo_root,
        &prompt,
        args.model.as_deref(),
        effort,
    )?;

    let body = if result.stdout.trim().is_empty() {
        result.stderr.trim()
    } else {
        result.stdout.trim()
    };
    let rendered = render_review_result(review_name, &target.label, body, result.status);
    let summary = first_meaningful_line(body, &format!("{review_name} completed."));

    let mut rendered = rendered;
    if let Some(ref sid) = result.session_id {
        rendered.push_str(&format!("\n\nGrok session: `{sid}`\n"));
    }

    finish_job_with_session(
        workspace,
        job,
        result.status,
        json!({
            "review": review_name,
            "target": { "label": target.label, "kind": target.kind },
            "grok": {
                "status": result.status,
                "stdout": result.stdout,
                "stderr": result.stderr,
                "sessionId": result.session_id
            }
        }),
        &rendered,
        &summary,
        result.session_id.clone(),
    )?;

    if args.json {
        println!(
            "{}",
            json_pretty(&json!({
                "review": review_name,
                "target": { "label": target.label, "kind": target.kind },
                "grok": {
                    "status": result.status,
                    "stdout": result.stdout,
                    "stderr": result.stderr
                }
            }))
        );
    } else {
        print!("{rendered}");
    }

    Ok(if result.status == 0 { 0 } else { 1 })
}
