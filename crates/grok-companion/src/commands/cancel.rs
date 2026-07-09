use std::path::PathBuf;

use serde_json::json;

use crate::error::Result;
use crate::jobutil::{is_active, sort_jobs_newest};
use crate::process::terminate_process_tree;
use crate::render::{json_pretty, render_cancel_report};
use crate::state::{list_jobs, now_iso, upsert_job};
use crate::workspace::resolve_workspace_root;

pub struct CancelArgs {
    pub job_id: Option<String>,
    pub json: bool,
    pub cwd: PathBuf,
}

pub fn run(args: CancelArgs) -> Result<i32> {
    let workspace = resolve_workspace_root(&args.cwd);
    let jobs = sort_jobs_newest(list_jobs(&workspace));

    let job = if let Some(id) = &args.job_id {
        jobs.into_iter().find(|j| j.id == *id)
    } else {
        jobs.into_iter()
            .find(|j| is_active(j.status.as_deref()))
    };

    let Some(mut job) = job else {
        if args.json {
            println!(
                "{}",
                json_pretty(&json!({ "canceled": false, "job": null }))
            );
        } else {
            print!("No cancelable Grok job found.\n");
        }
        return Ok(1);
    };

    let mut detail = "no pid recorded".to_string();
    let mut signaled = false;
    if let Some(pid) = job.pid {
        signaled = terminate_process_tree(pid);
        detail = if signaled {
            "signal sent".into()
        } else {
            "failed to signal process".into()
        };
    }

    job.status = Some("canceled".into());
    job.phase = Some("canceled".into());
    job.finished_at = Some(now_iso());
    job.progress_message = Some("Canceled by user".into());
    job.error = None;
    upsert_job(&workspace, job.clone())?;

    if args.json {
        println!(
            "{}",
            json_pretty(&json!({
                "canceled": true,
                "signaled": signaled,
                "detail": detail,
                "job": job
            }))
        );
    } else {
        print!(
            "{}",
            render_cancel_report(
                &job.id,
                job.title.as_deref().unwrap_or("job"),
                true,
                &detail
            )
        );
    }
    Ok(0)
}
