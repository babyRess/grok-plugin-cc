use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use crate::error::{CompanionError, Result};

#[derive(Debug, Clone)]
pub struct CommandResult {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CommandResult {
    pub fn from_output(output: Output) -> Self {
        Self {
            status: output.status.code().unwrap_or(1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }

    pub fn ok(&self) -> bool {
        self.status == 0
    }
}

pub fn run_command(
    program: &str,
    args: &[&str],
    cwd: Option<&Path>,
    _timeout: Option<Duration>,
) -> Result<CommandResult> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    let output = cmd
        .output()
        .map_err(|e| CompanionError::msg(format!("failed to spawn `{program}`: {e}")))?;

    Ok(CommandResult::from_output(output))
}

pub fn binary_available(program: &str, version_args: &[&str], cwd: Option<&Path>) -> (bool, String) {
    match run_command(program, version_args, cwd, Some(Duration::from_secs(15))) {
        Ok(result) if result.ok() || !result.stdout.trim().is_empty() => {
            let detail = result
                .stdout
                .lines()
                .next()
                .or_else(|| result.stderr.lines().next())
                .unwrap_or(program)
                .trim()
                .to_string();
            (true, detail)
        }
        Ok(result) => {
            let detail = result.stderr.trim();
            (
                false,
                if detail.is_empty() {
                    format!("{program} failed")
                } else {
                    detail.to_string()
                },
            )
        }
        Err(e) => (false, e.to_string()),
    }
}

/// Best-effort process-tree terminate (Unix process group first).
pub fn terminate_process_tree(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        // Try process group (negative pid)
        let group_ok = unsafe { libc_kill(-(pid as i32), 15) };
        if group_ok {
            return true;
        }
        return unsafe { libc_kill(pid as i32, 15) };
    }
    #[cfg(not(unix))]
    {
        let status = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        matches!(status, Ok(s) if s.success())
    }
}

#[cfg(unix)]
unsafe fn libc_kill(pid: i32, sig: i32) -> bool {
    // Use libc via nix-less raw syscall through Command alternative when libc crate absent.
    // Prefer /bin/kill for portability without extra deps.
    let _ = (pid, sig);
    let arg = if pid < 0 {
        format!("-{pid}")
    } else {
        pid.to_string()
    };
    Command::new("kill")
        .args(["-TERM", &arg])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_available_node() {
        let (ok, detail) = binary_available("node", &["--version"], None);
        assert!(ok, "{detail}");
        assert!(detail.contains('v') || detail.contains("node"), "{detail}");
    }
}
