//! Integration tests against the real local `grok` binary.
//! These are not fakes — they require `grok` on PATH and local auth.
//!
//! CI runs unit tests only (`cargo test --bin grok-companion`).
//! Run these locally with:
//!   cargo test -p grok-companion --test cli_real_grok -- --test-threads=1

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tempfile::tempdir;

fn bin() -> PathBuf {
    // Prefer the freshly built debug binary next to cargo test
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates
    p.pop(); // repo root
    let release = p.join("target/release/grok-companion");
    let debug = p.join("target/debug/grok-companion");
    if debug.exists() {
        debug
    } else if release.exists() {
        release
    } else {
        // fallback: run via cargo
        PathBuf::from("cargo")
    }
}

fn run_companion(args: &[&str], cwd: &std::path::Path, plugin_data: &std::path::Path) -> (i32, String, String) {
    let binary = bin();
    let mut cmd = if binary.file_name().and_then(|s| s.to_str()) == Some("cargo") {
        let mut c = Command::new("cargo");
        c.args(["run", "-q", "-p", "grok-companion", "--"]);
        c.args(args);
        c
    } else {
        let mut c = Command::new(&binary);
        c.args(args);
        c
    };
    cmd.current_dir(cwd);
    cmd.env("GROK_PLUGIN_DATA", plugin_data);
    // Ensure real grok is used (clear any override)
    cmd.env_remove("GROK_BIN");
    let out = cmd.output().expect("spawn companion");
    (
        out.status.code().unwrap_or(1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn make_git_repo() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    assert!(Command::new("git")
        .args(["init"])
        .current_dir(&root)
        .status()
        .unwrap()
        .success());
    // identity for commits if needed
    let _ = Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(&root)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(&root)
        .status();
    fs::write(root.join("hello.txt"), "hello world\n").unwrap();
    let _ = Command::new("git")
        .args(["add", "hello.txt"])
        .current_dir(&root)
        .status();
    let _ = Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(&root)
        .status();
    // unstaged change so review has something
    fs::write(root.join("hello.txt"), "hello world\nchange\n").unwrap();
    let pdata = root.join("plugin-data");
    fs::create_dir_all(&pdata).unwrap();
    (dir, root, pdata)
}

#[test]
fn real_grok_setup_ready() {
    let (_tmp, root, pdata) = make_git_repo();
    let (code, stdout, stderr) = run_companion(&["setup", "--json"], &root, &pdata);
    assert!(
        code == 0 || code == 1,
        "setup exit {code}\nstdout={stdout}\nstderr={stderr}"
    );
    let v: serde_json::Value = serde_json::from_str(&stdout).expect(&format!(
        "json parse failed: {stdout}\nstderr={stderr}"
    ));
    assert_eq!(v["runtime"], "rust");
    assert!(
        v["grok"]["available"].as_bool().unwrap_or(false),
        "expected real grok available: {v}"
    );
    // On this machine we expect auth; if not, still surface clearly
    assert!(
        v["auth"]["loggedIn"].as_bool().unwrap_or(false),
        "expected logged-in grok auth on this machine: {v}"
    );
    assert!(v["ready"].as_bool().unwrap_or(false), "ready: {v}");
}

#[test]
fn real_grok_status_empty_then_task_resume_candidate() {
    let (_tmp, root, pdata) = make_git_repo();
    let (code, stdout, _) = run_companion(&["status"], &root, &pdata);
    assert_eq!(code, 0);
    assert!(stdout.contains("No Grok companion jobs") || stdout.contains("Grok Companion Jobs"));

    let (code, stdout, stderr) =
        run_companion(&["task-resume-candidate", "--json"], &root, &pdata);
    assert_eq!(code, 0, "stderr={stderr}");
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["available"], false);
}

#[test]
fn real_grok_task_read_only_tiny() {
    let (_tmp, root, pdata) = make_git_repo();
    // Tiny prompt, force minimal work
    let (code, stdout, stderr) = run_companion(
        &[
            "task",
            "--read-only",
            "--",
            "Reply with exactly: PONG. Do not use tools. Do not read files.",
        ],
        &root,
        &pdata,
    );
    assert!(
        code == 0 || !stdout.is_empty() || !stderr.is_empty(),
        "task failed: code={code}\nstdout={stdout}\nstderr={stderr}"
    );
    // Job should be recorded
    let (scode, sstdout, _) = run_companion(&["status", "--json"], &root, &pdata);
    assert_eq!(scode, 0);
    let status: serde_json::Value = serde_json::from_str(&sstdout).unwrap();
    assert!(
        status["count"].as_u64().unwrap_or(0) >= 1,
        "expected a job: {status}"
    );

    let (rcode, rstdout, _) = run_companion(&["result", "--json"], &root, &pdata);
    assert_eq!(rcode, 0, "result: {rstdout}");
    let result: serde_json::Value = serde_json::from_str(&rstdout).unwrap();
    assert!(result.get("job").is_some(), "{result}");
}

#[test]
fn real_grok_review_working_tree() {
    let (_tmp, root, pdata) = make_git_repo();
    let (code, stdout, stderr) = run_companion(&["review"], &root, &pdata);
    // Review may take a while; accept 0 or non-zero if grok produced output
    assert!(
        !stdout.is_empty() || !stderr.is_empty(),
        "review produced no output code={code}"
    );
    assert!(
        stdout.contains("Grok Review") || stdout.contains("Nothing to review") || code == 0,
        "stdout={stdout}\nstderr={stderr}"
    );
}

#[test]
fn real_grok_setup_toggle_gate() {
    let (_tmp, root, pdata) = make_git_repo();
    let (code, stdout, _) = run_companion(&["setup", "--enable-review-gate", "--json"], &root, &pdata);
    assert!(code == 0 || code == 1);
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["config"]["stopReviewGate"], true);

    let (code, stdout, _) =
        run_companion(&["setup", "--disable-review-gate", "--json"], &root, &pdata);
    assert!(code == 0 || code == 1);
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["config"]["stopReviewGate"], false);
}
