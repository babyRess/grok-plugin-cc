use std::path::PathBuf;

use serde_json::json;

use crate::commands::task::resume_candidate;
use crate::error::Result;
use crate::render::json_pretty;

pub struct ResumeCandidateArgs {
    pub json: bool,
    pub cwd: PathBuf,
}

pub fn run(args: ResumeCandidateArgs) -> Result<i32> {
    let latest = resume_candidate(&args.cwd);
    let available = latest.is_some();
    let payload = json!({
        "available": available,
        "jobId": latest.as_ref().map(|j| &j.id),
        "title": latest.as_ref().and_then(|j| j.title.as_ref()),
        "summary": latest.as_ref().and_then(|j| j.summary.as_ref()),
        "status": latest.as_ref().and_then(|j| j.status.as_ref()),
        "grokSessionId": latest.as_ref().and_then(|j| j.grok_session_id.as_ref()),
    });

    if args.json {
        println!("{}", json_pretty(&payload));
    } else if let Some(j) = latest {
        println!(
            "Resumable task available: {} ({})\n",
            j.id,
            j.title.as_deref().unwrap_or("task")
        );
    } else {
        print!("No resumable Grok task thread for this repository.\n");
    }
    Ok(0)
}
