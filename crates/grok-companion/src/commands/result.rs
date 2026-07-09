use std::path::PathBuf;

use serde_json::json;

use crate::error::Result;
use crate::jobutil::sort_jobs_newest;
use crate::render::{json_pretty, render_stored_job_result};
use crate::state::{list_jobs, read_job_result};
use crate::workspace::resolve_workspace_root;

pub struct ResultArgs {
    pub job_id: Option<String>,
    pub json: bool,
    pub cwd: PathBuf,
}

pub fn run(args: ResultArgs) -> Result<i32> {
    let workspace = resolve_workspace_root(&args.cwd);
    let jobs = sort_jobs_newest(list_jobs(&workspace));

    let job = if let Some(id) = &args.job_id {
        jobs.into_iter().find(|j| j.id == *id)
    } else {
        jobs.into_iter()
            .find(|j| matches!(j.status.as_deref(), Some("completed") | Some("failed")))
            .or_else(|| {
                let jobs = sort_jobs_newest(list_jobs(&workspace));
                jobs.into_iter().next()
            })
    };

    let Some(job) = job else {
        if args.json {
            println!("{}", json_pretty(&json!({ "error": "not_found" })));
        } else {
            print!("No stored Grok result found.\n");
        }
        return Ok(1);
    };

    let stored = job
        .result_file
        .as_ref()
        .and_then(|p| read_job_result(std::path::Path::new(p)).ok().flatten());

    if args.json {
        println!(
            "{}",
            json_pretty(&json!({
                "job": job,
                "stored": stored
            }))
        );
    } else {
        print!("{}", render_stored_job_result(&job, stored.as_ref()));
    }
    Ok(0)
}
