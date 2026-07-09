use std::path::PathBuf;

use serde_json::json;

use crate::commands::task::{self, TaskArgs};
use crate::error::Result;
use crate::render::json_pretty;
use crate::transfer::{build_transfer_prompt, find_latest_claude_session};

pub struct TransferArgs {
    pub source: Option<PathBuf>,
    pub write: bool,
    pub background: bool,
    pub json: bool,
    pub cwd: PathBuf,
}

pub fn run(args: TransferArgs) -> Result<i32> {
    let source = match args.source {
        Some(p) => p,
        None => find_latest_claude_session(&args.cwd)?,
    };

    let handoff = build_transfer_prompt(&source)?;
    eprintln!(
        "[grok] Transfer: {} turns from {}",
        handoff.turns,
        handoff.source.display()
    );

    if args.json {
        // Only emit the handoff package; caller can decide to run task.
        println!(
            "{}",
            json_pretty(&json!({
                "source": handoff.source.display().to_string(),
                "turns": handoff.turns,
                "promptChars": handoff.prompt.len()
            }))
        );
    }

    // Run as a normal write-capable (or read-only) task with the handoff prompt.
    task::run(TaskArgs {
        prompt: Some(handoff.prompt),
        prompt_file: None,
        model: None,
        effort: None,
        write: args.write,
        resume_last: false,
        background: args.background,
        json: args.json,
        inherit_claude_context: true,
        inherit_claude_context_full: false,
        cwd: args.cwd,
        job_id: None,
    })
}
