use std::path::PathBuf;

use serde_json::json;

use crate::error::Result;
use crate::jobutil::{is_active, reconcile_and_list_jobs, reconcile_stale_job};
use crate::process::is_process_alive;
use crate::render::{json_pretty, render_job_status, render_status_report};
use crate::state::{read_job_result, Job};
use crate::workspace::resolve_workspace_root;

pub struct StatusArgs {
    pub job_id: Option<String>,
    pub all: bool,
    pub json: bool,
    pub cwd: PathBuf,
}

pub fn run(args: StatusArgs) -> Result<i32> {
    let workspace = resolve_workspace_root(&args.cwd);
    // Reap workers that died without calling finish_job (stale "running" markers).
    let all_jobs = reconcile_and_list_jobs(&workspace)?;

    if let Some(ref id) = args.job_id {
        let mut job = all_jobs.into_iter().find(|j| j.id == *id);
        match job.as_mut() {
            None => {
                if args.json {
                    println!("{}", json_pretty(&json!({ "error": "not_found" })));
                } else {
                    print!("Job not found: {id}\n");
                }
                return Ok(1);
            }
            Some(job) => {
                // Second pass in case list was loaded before this id was written.
                let _ = reconcile_stale_job(&workspace, job);
                if args.json {
                    let result = job
                        .result_file
                        .as_ref()
                        .and_then(|p| read_job_result(std::path::Path::new(p)).ok().flatten());
                    let process_alive = job.pid.map(is_process_alive);
                    println!(
                        "{}",
                        json_pretty(&json!({
                            "job": job,
                            "result": result,
                            "processAlive": process_alive
                        }))
                    );
                } else {
                    print!("{}", render_job_status(job));
                }
                return Ok(0);
            }
        }
    }

    let jobs: Vec<Job> = if args.all {
        all_jobs
    } else {
        let active: Vec<_> = all_jobs
            .iter()
            .filter(|j| is_active(j.status.as_deref()))
            .cloned()
            .collect();
        let recent: Vec<_> = all_jobs
            .iter()
            .filter(|j| !is_active(j.status.as_deref()))
            .take(5)
            .cloned()
            .collect();
        let mut out = active;
        out.extend(recent);
        out
    };

    if args.json {
        let annotated: Vec<_> = jobs
            .iter()
            .map(|j| {
                json!({
                    "job": j,
                    "processAlive": j.pid.map(is_process_alive)
                })
            })
            .collect();
        println!(
            "{}",
            json_pretty(&json!({
                "workspace": workspace.display().to_string(),
                "jobs": jobs,
                "jobsWithLiveness": annotated,
                "count": jobs.len(),
                "runtime": "rust"
            }))
        );
    } else {
        print!("{}", render_status_report(&jobs));
    }
    Ok(0)
}
