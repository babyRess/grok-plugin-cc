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

/// Return true if `pid` is currently alive and signalable by this process.
///
/// Uses `kill -0` on Unix (no signal delivered). On Windows, checks via `tasklist`.
pub fn is_process_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let output = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();
        match output {
            Ok(out) => {
                let text = String::from_utf8_lossy(&out.stdout);
                text.contains(&pid.to_string())
            }
            Err(_) => false,
        }
    }
}

/// Best-effort process-tree terminate (Unix process group / session first).
pub fn terminate_process_tree(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        // Prefer process group (negative pid) so detached workers die together.
        let group_ok = kill_signal(-(pid as i32), 15);
        if group_ok {
            return true;
        }
        kill_signal(pid as i32, 15)
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
fn kill_signal(pid: i32, _sig: i32) -> bool {
    // Prefer /bin/kill for portability without a libc crate dependency.
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

/// Put the current process in a new session (and process group).
///
/// Used by background workers so they survive parent exit and can be canceled
/// via process-group signals. Returns `true` on success.
#[cfg(unix)]
pub fn become_session_leader() -> bool {
    // Avoid a libc crate dependency: call POSIX setsid via extern.
    extern "C" {
        fn setsid() -> i32;
    }
    unsafe { setsid() >= 0 }
}

#[cfg(not(unix))]
pub fn become_session_leader() -> bool {
    true
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

    #[test]
    fn is_process_alive_self_and_dead() {
        let self_pid = std::process::id();
        assert!(is_process_alive(self_pid), "current process should be alive");
        // PIDs are recycled eventually, but very high unused values are typically dead.
        // 1 is init/launchd and is usually alive on Unix; use a likely-unused high pid.
        // Safer check: pid 0 is always treated as not alive by our API.
        assert!(!is_process_alive(0));
    }
}
