mod claude_context;
mod commands;
mod error;
mod git;
mod grok;
mod jobutil;
mod process;
mod render;
mod state;
#[cfg(test)]
mod test_env;
mod worker;
mod workspace;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use commands::cancel::{self, CancelArgs};
use commands::result::{self, ResultArgs};
use commands::resume_candidate::{self, ResumeCandidateArgs};
use commands::review::{self, ReviewArgs};
use commands::setup::{self, SetupArgs};
use commands::status::{self, StatusArgs};
use commands::task::{self, TaskArgs};
use commands::task_worker::{self, TaskWorkerArgs};

#[derive(Debug, Parser)]
#[command(
    name = "grok-companion",
    version,
    about = "Rust companion for the Grok Claude Code plugin (full CLI parity)"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Check Grok CLI readiness and optionally toggle the stop review gate
    Setup {
        #[arg(long)]
        enable_review_gate: bool,
        #[arg(long)]
        disable_review_gate: bool,
        #[arg(long)]
        json: bool,
        #[arg(long, short = 'C')]
        cwd: Option<PathBuf>,
    },

    /// Show running and recent companion jobs
    Status {
        job_id: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        json: bool,
        #[arg(long, short = 'C')]
        cwd: Option<PathBuf>,
    },

    /// Run a read-only Grok code review against local git state
    Review {
        #[arg(long)]
        base: Option<String>,
        #[arg(long, value_parser = ["auto", "working-tree", "branch"])]
        scope: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        effort: Option<String>,
        #[arg(long)]
        background: bool,
        #[arg(long)]
        json: bool,
        /// Skip injecting on-disk Claude skills/MCP into the prompt
        #[arg(long = "no-inherit-claude-context")]
        no_inherit_claude_context: bool,
        /// Verbose Claude context (slow; large prompt)
        #[arg(long = "inherit-claude-context-full")]
        inherit_claude_context_full: bool,
        #[arg(long, short = 'C')]
        cwd: Option<PathBuf>,
    },

    /// Steerable adversarial review (optional focus text as trailing args)
    #[command(name = "adversarial-review")]
    AdversarialReview {
        #[arg(long)]
        base: Option<String>,
        #[arg(long, value_parser = ["auto", "working-tree", "branch"])]
        scope: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        effort: Option<String>,
        #[arg(long)]
        background: bool,
        #[arg(long)]
        json: bool,
        #[arg(long = "no-inherit-claude-context")]
        no_inherit_claude_context: bool,
        #[arg(long = "inherit-claude-context-full")]
        inherit_claude_context_full: bool,
        #[arg(long, short = 'C')]
        cwd: Option<PathBuf>,
        /// Focus text for the adversarial review
        #[arg(trailing_var_arg = true, allow_hyphen_values = false)]
        focus: Vec<String>,
    },

    /// Delegate a write-capable (or read-only) Grok task
    Task {
        #[arg(long)]
        background: bool,
        #[arg(long)]
        write: bool,
        #[arg(long = "read-only")]
        read_only: bool,
        #[arg(long = "resume-last")]
        resume_last: bool,
        #[arg(long)]
        resume: bool,
        #[arg(long)]
        fresh: bool,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        effort: Option<String>,
        #[arg(long = "prompt-file")]
        prompt_file: Option<PathBuf>,
        #[arg(long)]
        json: bool,
        #[arg(long = "no-inherit-claude-context")]
        no_inherit_claude_context: bool,
        #[arg(long = "inherit-claude-context-full")]
        inherit_claude_context_full: bool,
        #[arg(long, short = 'C')]
        cwd: Option<PathBuf>,
        /// Task prompt words
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        prompt: Vec<String>,
    },

    /// Internal: run a queued background job
    #[command(name = "task-worker")]
    TaskWorker {
        #[arg(long = "job-id")]
        job_id: String,
        #[arg(long, short = 'C')]
        cwd: Option<PathBuf>,
    },

    /// Show stored output for a finished job
    Result {
        job_id: Option<String>,
        #[arg(long)]
        json: bool,
        #[arg(long, short = 'C')]
        cwd: Option<PathBuf>,
    },

    /// Cancel an active background job
    Cancel {
        job_id: Option<String>,
        #[arg(long)]
        json: bool,
        #[arg(long, short = 'C')]
        cwd: Option<PathBuf>,
    },

    /// Check whether a prior task can be resumed
    #[command(name = "task-resume-candidate")]
    TaskResumeCandidate {
        #[arg(long)]
        json: bool,
        #[arg(long, short = 'C')]
        cwd: Option<PathBuf>,
    },
}

fn resolve_cwd(cwd: Option<PathBuf>) -> PathBuf {
    cwd.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Setup {
            enable_review_gate,
            disable_review_gate,
            json,
            cwd,
        } => setup::run(SetupArgs {
            enable_review_gate,
            disable_review_gate,
            json,
            cwd: resolve_cwd(cwd),
        }),
        Commands::Status {
            job_id,
            all,
            json,
            cwd,
        } => status::run(StatusArgs {
            job_id,
            all,
            json,
            cwd: resolve_cwd(cwd),
        }),
        Commands::Review {
            base,
            scope,
            model,
            effort,
            background,
            json,
            no_inherit_claude_context,
            inherit_claude_context_full,
            cwd,
        } => review::run(ReviewArgs {
            base,
            scope,
            model,
            effort,
            focus_text: None,
            adversarial: false,
            background,
            json,
            inherit_claude_context: !no_inherit_claude_context,
            inherit_claude_context_full,
            cwd: resolve_cwd(cwd),
        }),
        Commands::AdversarialReview {
            base,
            scope,
            model,
            effort,
            background,
            json,
            no_inherit_claude_context,
            inherit_claude_context_full,
            cwd,
            focus,
        } => review::run(ReviewArgs {
            base,
            scope,
            model,
            effort,
            focus_text: Some(focus.join(" ")).filter(|s| !s.trim().is_empty()),
            adversarial: true,
            background,
            json,
            inherit_claude_context: !no_inherit_claude_context,
            inherit_claude_context_full,
            cwd: resolve_cwd(cwd),
        }),
        Commands::Task {
            background,
            write,
            read_only,
            resume_last,
            resume,
            fresh,
            model,
            effort,
            prompt_file,
            json,
            no_inherit_claude_context,
            inherit_claude_context_full,
            cwd,
            prompt,
        } => {
            let write_mode = if read_only { false } else { true };
            let _ = write; // --write is default; accepted for CLI compatibility
            let resume_flag = (resume_last || resume) && !fresh;
            task::run(TaskArgs {
                prompt: Some(prompt.join(" ")),
                prompt_file,
                model,
                effort,
                write: write_mode,
                resume_last: resume_flag,
                background,
                json,
                inherit_claude_context: !no_inherit_claude_context,
                inherit_claude_context_full,
                cwd: resolve_cwd(cwd),
                job_id: None,
            })
        }
        Commands::TaskWorker { job_id, cwd } => task_worker::run(TaskWorkerArgs {
            job_id,
            cwd: resolve_cwd(cwd),
        }),
        Commands::Result { job_id, json, cwd } => result::run(ResultArgs {
            job_id,
            json,
            cwd: resolve_cwd(cwd),
        }),
        Commands::Cancel { job_id, json, cwd } => cancel::run(CancelArgs {
            job_id,
            json,
            cwd: resolve_cwd(cwd),
        }),
        Commands::TaskResumeCandidate { json, cwd } => {
            resume_candidate::run(ResumeCandidateArgs {
                json,
                cwd: resolve_cwd(cwd),
            })
        }
    };

    match result {
        Ok(code) => ExitCode::from(code as u8),
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(1)
        }
    }
}
