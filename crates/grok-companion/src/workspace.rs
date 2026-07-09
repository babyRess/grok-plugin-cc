use std::path::{Path, PathBuf};

/// Walk up from `cwd` to find the nearest `.git` directory; else return `cwd`.
pub fn resolve_workspace_root(cwd: &Path) -> PathBuf {
    let mut current = cwd
        .canonicalize()
        .unwrap_or_else(|_| cwd.to_path_buf());

    loop {
        if current.join(".git").exists() {
            return current;
        }
        if !current.pop() {
            return cwd
                .canonicalize()
                .unwrap_or_else(|_| cwd.to_path_buf());
        }
    }
}
