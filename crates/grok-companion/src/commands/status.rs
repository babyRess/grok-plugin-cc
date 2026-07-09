use std::path::PathBuf;

use serde_json::json;

use crate::error::Result;
use crate::jobutil::{is_active, sort_jobs_newest};
use crate::render::{json_pretty, render_job_status, render_status_report};
use crate::state::{list_jobs, read_job_result, Job};
use crate::workspace::resolve_workspace_root;

pub struct StatusArgs {
    pub job_id: Option<String>,
    pub all: bool,
    pub json: bool,
    pub cwd: PathBuf,
}

pub fn run(args: StatusArgs) -> Result<i32> {
    let workspace = resolve_workspace_root(&args.cwd);
    let all_jobs = sort_jobs_newest(list_jobs(&workspace));

    if let Some(ref id) = args.job_id {
        let job = all_jobs.iter().find(|j| j.id == *id);
        match job {
            None => {
                if args.json {
                    println!("{}", json_pretty(&json!({ "error": "not_found" })));
                } else {
                    print!("Job not found: {id}\n");
                }
                return Ok(1);
            }
            Some(job) => {
                if args.json {
                    let result = job
                        .result_file
                        .as_ref()
                        .and_then(|p| read_job_result(std::path::Path::new(p)).ok().flatten());
                    println!(
                        "{}",
                        json_pretty(&json!({
                            "job": job,
                            "result": result
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
        println!(
            "{}",
            json_pretty(&json!({
                "workspace": workspace.display().to_string(),
                "jobs": jobs,
                "count": jobs.len(),
                "runtime": "rust"
            }))
        );
    } else {
        print!("{}", render_status_report(&jobs));
    }
    Ok(0)
}
