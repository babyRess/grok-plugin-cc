use std::path::PathBuf;

use crate::commands::review::{self, ReviewArgs};
use crate::commands::task;
use crate::error::Result;
use crate::jobutil::new_job;
use crate::state::{list_jobs, upsert_job};
use crate::worker::{read_worker_request, WorkerRequest};
use crate::workspace::resolve_workspace_root;

pub struct TaskWorkerArgs {
    pub job_id: String,
    pub cwd: PathBuf,
}

pub fn run(args: TaskWorkerArgs) -> Result<i32> {
    let workspace = resolve_workspace_root(&args.cwd);
    let payload = read_worker_request(&workspace, &args.job_id)?;

    let existing = list_jobs(&workspace)
        .into_iter()
        .find(|j| j.id == args.job_id);

    match payload {
        WorkerRequest::Review { request } | WorkerRequest::AdversarialReview { request } => {
            let adversarial = matches!(
                // re-read kind from stored if needed
                request.adversarial,
                true
            );
            let mut job = existing.unwrap_or_else(|| {
                new_job(
                    if adversarial { "adv" } else { "rev" },
                    if adversarial {
                        "adversarial-review"
                    } else {
                        "review"
                    },
                    if adversarial {
                        "adversarial-review"
                    } else {
                        "review"
                    },
                    "Grok Worker",
                    &workspace,
                    "review",
                    "background worker",
                    false,
                )
            });
            job.id = args.job_id.clone();
            upsert_job(&workspace, job.clone())?;

            let review_args = ReviewArgs {
                base: request.base,
                scope: request.scope,
                model: request.model,
                effort: request.effort.clone(),
                focus_text: request.focus_text,
                adversarial: request.adversarial,
                background: false,
                json: false,
                inherit_claude_context: request.inherit_claude_context,
                inherit_claude_context_full: request.inherit_claude_context_full,
                cwd: request.cwd.clone(),
            };
            review::execute_review(
                &mut job,
                &workspace,
                &review_args,
                request.effort.as_deref(),
            )
        }
        WorkerRequest::Task { request } => {
            let mut job = existing.unwrap_or_else(|| {
                new_job(
                    "task",
                    "task",
                    "rescue",
                    "Grok Worker",
                    &workspace,
                    "task",
                    "background worker",
                    request.write,
                )
            });
            job.id = args.job_id.clone();
            upsert_job(&workspace, job.clone())?;

            let title = if request.resume_last {
                "Grok Resume"
            } else {
                "Grok Task"
            };
            task::execute_task(
                &mut job,
                &workspace,
                &request.prompt,
                request.model.as_deref(),
                request.effort.as_deref(),
                request.write,
                request.resume_last,
                false,
                title,
                request.inherit_claude_context,
                request.inherit_claude_context_full,
                &request.cwd,
            )
        }
    }
}
