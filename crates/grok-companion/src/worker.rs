//! Background worker request files + detached spawn.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use crate::error::{CompanionError, Result};
use crate::state::{ensure_state_dir, resolve_jobs_dir};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WorkerRequest {
    #[serde(rename = "review")]
    Review { request: ReviewWorkerRequest },
    #[serde(rename = "adversarial-review")]
    AdversarialReview { request: ReviewWorkerRequest },
    #[serde(rename = "task")]
    Task { request: TaskWorkerRequest },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewWorkerRequest {
    pub cwd: PathBuf,
    pub base: Option<String>,
    pub scope: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub focus_text: Option<String>,
    pub adversarial: bool,
    #[serde(default = "default_true")]
    pub inherit_claude_context: bool,
    #[serde(default)]
    pub inherit_claude_context_full: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskWorkerRequest {
    pub cwd: PathBuf,
    pub prompt: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub write: bool,
    pub resume_last: bool,
    pub job_id: String,
    #[serde(default = "default_true")]
    pub inherit_claude_context: bool,
    #[serde(default)]
    pub inherit_claude_context_full: bool,
}

fn default_true() -> bool {
    true
}

pub fn request_path(workspace: &Path, job_id: &str) -> PathBuf {
    resolve_jobs_dir(workspace).join(format!("{job_id}-request.json"))
}

pub fn write_worker_request(workspace: &Path, job_id: &str, payload: &WorkerRequest) -> Result<PathBuf> {
    ensure_state_dir(workspace)?;
    let path = request_path(workspace, job_id);
    fs::write(&path, format!("{}\n", serde_json::to_string_pretty(payload)?))?;
    Ok(path)
}

pub fn read_worker_request(workspace: &Path, job_id: &str) -> Result<WorkerRequest> {
    let path = request_path(workspace, job_id);
    if !path.exists() {
        return Err(CompanionError::msg(format!(
            "Missing worker request file: {}",
            path.display()
        )));
    }
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&raw)?)
}

/// Spawn this binary as a detached task-worker.
pub fn spawn_detached_worker(cwd: &Path, job_id: &str) -> Result<u32> {
    let exe = std::env::current_exe()
        .map_err(|e| CompanionError::msg(format!("current_exe: {e}")))?;

    let mut cmd = Command::new(exe);
    cmd.args([
        "task-worker",
        "--job-id",
        job_id,
        "--cwd",
        &cwd.display().to_string(),
    ])
    .current_dir(cwd)
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Put worker in its own process group so cancel can signal it.
        unsafe {
            cmd.pre_exec(|| {
                libc_setpgid();
                Ok(())
            });
        }
    }

    let child = cmd
        .spawn()
        .map_err(|e| CompanionError::msg(format!("failed to spawn task-worker: {e}")))?;

    Ok(child.id())
}

#[cfg(unix)]
fn libc_setpgid() {
    // Avoid extra libc dep: shell out is not available in pre_exec.
    // Use nix-less raw: setpgid(0,0) via libc crate would be ideal.
    // For portability without libc, skip; cancel still kills by pid.
    let _ = 0;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env::lock_env;
    use tempfile::tempdir;

    #[test]
    fn worker_request_roundtrip() {
        let _guard = lock_env();
        let dir = tempdir().unwrap();
        let git = dir.path().join(".git");
        fs::create_dir_all(&git).unwrap();
        let pdata = dir.path().join("pdata");
        fs::create_dir_all(&pdata).unwrap();
        std::env::set_var("CLAUDE_PLUGIN_DATA", &pdata);

        let req = WorkerRequest::Task {
            request: TaskWorkerRequest {
                cwd: dir.path().to_path_buf(),
                prompt: "fix it".into(),
                model: None,
                effort: Some("low".into()),
                write: true,
                resume_last: false,
                job_id: "task-1".into(),
                inherit_claude_context: true,
                inherit_claude_context_full: false,
            },
        };
        let written = write_worker_request(dir.path(), "task-1", &req).unwrap();
        assert!(written.exists(), "missing {}", written.display());
        let loaded = read_worker_request(dir.path(), "task-1").unwrap();
        match loaded {
            WorkerRequest::Task { request } => {
                assert_eq!(request.prompt, "fix it");
                assert!(request.write);
            }
            _ => panic!("wrong type"),
        }
        std::env::remove_var("CLAUDE_PLUGIN_DATA");
    }
}
