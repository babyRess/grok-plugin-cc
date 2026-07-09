use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::Result;
use crate::workspace::resolve_workspace_root;

const STATE_VERSION: u32 = 1;
const STATE_FILE_NAME: &str = "state.json";
const JOBS_DIR_NAME: &str = "jobs";
const MAX_JOBS: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default)]
    pub stop_review_gate: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            stop_review_gate: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub kind_label: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub workspace_root: Option<String>,
    #[serde(default)]
    pub job_class: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub write: Option<bool>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub phase: Option<String>,
    #[serde(default)]
    pub progress_message: Option<String>,
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub grok_session_id: Option<String>,
    #[serde(default)]
    pub log_file: Option<String>,
    #[serde(default)]
    pub result_file: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub config: Config,
    #[serde(default)]
    pub jobs: Vec<Job>,
}

fn default_version() -> u32 {
    STATE_VERSION
}

impl Default for State {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            config: Config::default(),
            jobs: Vec::new(),
        }
    }
}

/// Whether a plugin-data path looks like it belongs to the Grok plugin.
///
/// Claude Code sometimes leaves `CLAUDE_PLUGIN_DATA` pointed at another plugin
/// (e.g. `codex-openai-codex`) in the Bash environment. Writing Grok jobs there
/// makes status/result miss the real store and confuses debugging.
fn looks_like_grok_plugin_data(path: &Path) -> bool {
    let s = path.to_string_lossy().to_ascii_lowercase();
    s.contains("grok")
}

fn plugin_data_root() -> Option<PathBuf> {
    // Prefer an explicit Grok-owned root first.
    if let Ok(v) = std::env::var("GROK_PLUGIN_DATA") {
        if !v.is_empty() {
            return Some(PathBuf::from(v));
        }
    }
    // Accept CLAUDE_PLUGIN_DATA only when it is clearly this plugin's data dir.
    if let Ok(v) = std::env::var("CLAUDE_PLUGIN_DATA") {
        if !v.is_empty() {
            let p = PathBuf::from(&v);
            if looks_like_grok_plugin_data(&p) {
                return Some(p);
            }
        }
    }
    // Stable home fallback so status/result work outside Claude plugin env.
    if let Some(home) = directories::UserDirs::new().map(|u| u.home_dir().to_path_buf()) {
        let stable = home.join(".grok").join("companion-state");
        return Some(stable);
    }
    None
}

fn fallback_state_root() -> PathBuf {
    std::env::temp_dir().join("grok-companion")
}

fn slugify(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-' {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "workspace".into()
    } else {
        trimmed.to_string()
    }
}

pub fn resolve_state_dir(cwd: &Path) -> PathBuf {
    let workspace_root = resolve_workspace_root(cwd);
    let canonical = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.clone());

    let slug_source = workspace_root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("workspace");
    let slug = slugify(slug_source);

    let mut hasher = Sha256::new();
    hasher.update(canonical.to_string_lossy().as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    let hash16 = &hash[..16.min(hash.len())];

    let state_root = plugin_data_root()
        .map(|p| p.join("state"))
        .unwrap_or_else(fallback_state_root);

    state_root.join(format!("{slug}-{hash16}"))
}

pub fn resolve_state_file(cwd: &Path) -> PathBuf {
    resolve_state_dir(cwd).join(STATE_FILE_NAME)
}

pub fn resolve_jobs_dir(cwd: &Path) -> PathBuf {
    resolve_state_dir(cwd).join(JOBS_DIR_NAME)
}

pub fn ensure_state_dir(cwd: &Path) -> Result<()> {
    fs::create_dir_all(resolve_jobs_dir(cwd))?;
    Ok(())
}

pub fn load_state(cwd: &Path) -> State {
    let state_file = resolve_state_file(cwd);
    if !state_file.exists() {
        return State::default();
    }
    match fs::read_to_string(&state_file) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => State::default(),
    }
}

fn prune_jobs(jobs: Vec<Job>) -> Vec<Job> {
    let mut jobs = jobs;
    jobs.sort_by(|a, b| {
        b.updated_at
            .as_deref()
            .unwrap_or("")
            .cmp(a.updated_at.as_deref().unwrap_or(""))
    });
    jobs.truncate(MAX_JOBS);
    jobs
}

pub fn save_state(cwd: &Path, state: &State) -> Result<()> {
    ensure_state_dir(cwd)?;
    let mut next = state.clone();
    next.version = STATE_VERSION;
    next.jobs = prune_jobs(next.jobs);
    let path = resolve_state_file(cwd);
    let body = serde_json::to_string_pretty(&next)?;
    fs::write(path, format!("{body}\n"))?;
    Ok(())
}

pub fn get_config(cwd: &Path) -> Config {
    load_state(cwd).config
}

pub fn set_config(cwd: &Path, stop_review_gate: Option<bool>) -> Result<Config> {
    let mut state = load_state(cwd);
    if let Some(v) = stop_review_gate {
        state.config.stop_review_gate = v;
    }
    save_state(cwd, &state)?;
    Ok(state.config)
}

pub fn list_jobs(cwd: &Path) -> Vec<Job> {
    load_state(cwd).jobs
}

pub fn upsert_job(cwd: &Path, job: Job) -> Result<()> {
    let mut state = load_state(cwd);
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    if let Some(existing) = state.jobs.iter_mut().find(|j| j.id == job.id) {
        let created = existing.created_at.clone();
        *existing = job;
        if existing.created_at.is_none() {
            existing.created_at = created;
        }
        existing.updated_at = Some(now);
    } else {
        let mut job = job;
        if job.created_at.is_none() {
            job.created_at = Some(now.clone());
        }
        job.updated_at = Some(now);
        state.jobs.insert(0, job);
    }
    save_state(cwd, &state)
}

pub fn generate_job_id(prefix: &str) -> String {
    let ts = format!("{:x}", Utc::now().timestamp_millis());
    let rand: u32 = {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        std::time::Instant::now().hash(&mut h);
        std::process::id().hash(&mut h);
        (h.finish() as u32) % 1_000_000
    };
    format!("{prefix}-{ts}-{rand:x}")
}

pub fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn write_job_result(cwd: &Path, job_id: &str, payload: &serde_json::Value) -> Result<PathBuf> {
    ensure_state_dir(cwd)?;
    let path = resolve_jobs_dir(cwd).join(format!("{job_id}-result.json"));
    fs::write(&path, format!("{}\n", serde_json::to_string_pretty(payload)?))?;
    Ok(path)
}

pub fn read_job_result(path: &Path) -> Result<Option<serde_json::Value>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&raw)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env::lock_env;
    use tempfile::tempdir;

    #[test]
    fn config_roundtrip() {
        let _guard = lock_env();
        let dir = tempdir().unwrap();
        let git = dir.path().join(".git");
        fs::create_dir_all(&git).unwrap();
        let pdata = dir.path().join("pdata");
        fs::create_dir_all(&pdata).unwrap();
        std::env::set_var("GROK_PLUGIN_DATA", &pdata);
        set_config(dir.path(), Some(true)).unwrap();
        assert!(get_config(dir.path()).stop_review_gate);
        std::env::remove_var("GROK_PLUGIN_DATA");
    }

    #[test]
    fn job_upsert_and_list() {
        let _guard = lock_env();
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".git")).unwrap();
        let pdata = dir.path().join("pdata");
        fs::create_dir_all(&pdata).unwrap();
        std::env::set_var("GROK_PLUGIN_DATA", &pdata);

        let id = generate_job_id("task");
        upsert_job(
            dir.path(),
            Job {
                id: id.clone(),
                kind: Some("task".into()),
                kind_label: Some("rescue".into()),
                title: Some("T".into()),
                workspace_root: Some(dir.path().display().to_string()),
                job_class: Some("task".into()),
                summary: Some("s".into()),
                write: Some(true),
                status: Some("completed".into()),
                phase: Some("completed".into()),
                progress_message: None,
                pid: None,
                session_id: None,
                grok_session_id: None,
                log_file: None,
                result_file: None,
                exit_code: Some(0),
                error: None,
                created_at: Some(now_iso()),
                updated_at: Some(now_iso()),
                started_at: None,
                finished_at: Some(now_iso()),
            },
        )
        .unwrap();
        let jobs = list_jobs(dir.path());
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, id);
        std::env::remove_var("GROK_PLUGIN_DATA");
    }
}
