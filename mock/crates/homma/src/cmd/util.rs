//! Cross-command helpers.

use std::path::{Path, PathBuf};

/// Resolve a repo-relative path against the workspace root.
///
/// Absolute paths pass through unchanged. Relative paths are joined onto
/// `workspace_root`. The result is purely a path-shape transform; existence
/// is not checked here.
pub(crate) fn resolve_local_path(workspace_root: &Path, repo_path: &Path) -> PathBuf {
    if repo_path.is_absolute() {
        repo_path.to_path_buf()
    } else {
        workspace_root.join(repo_path)
    }
}
