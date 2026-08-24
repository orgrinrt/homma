//! Cross-command helpers.

use std::path::{Path, PathBuf};

use homma_api::AbsPath;

/// Resolve a repo-relative path against the workspace root.
///
/// **A second, untyped copy of [`AbsPath::resolve`] stood here** with six
/// callers, written before the type existed and left standing when it arrived.
/// It now delegates, so `..` is normalised the same way everywhere and one
/// definition governs.
///
/// Returns a `PathBuf` because these callers do not take `AbsPath`, and
/// converting them is work for the round that needs it rather than this one.
/// The resolution itself is no longer duplicated, which is the part that
/// mattered: two spellings of it disagreed about `..`.
pub(crate) fn resolve_local_path(workspace_root: &Path, repo_path: &Path) -> PathBuf {
    match AbsPath::new(workspace_root) {
        Ok(root) => AbsPath::resolve(&root, repo_path).into_path_buf(),
        // A relative workspace root is not something this can fix, and
        // inventing a base would hide it. The old shape, unchanged.
        Err(_) => {
            if repo_path.is_absolute() {
                repo_path.to_path_buf()
            } else {
                workspace_root.join(repo_path)
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_path_anchors_at_the_root() {
        assert_eq!(
            resolve_local_path(Path::new("/srv/ws"), Path::new("notko")),
            PathBuf::from("/srv/ws/notko")
        );
    }

    #[test]
    fn an_absolute_path_passes_through() {
        assert_eq!(
            resolve_local_path(Path::new("/srv/ws"), Path::new("/elsewhere/notko")),
            PathBuf::from("/elsewhere/notko")
        );
    }

    #[test]
    fn a_parent_component_is_normalised_the_same_way_the_type_does_it() {
        // The reason this delegates. Two spellings of one resolution disagreed
        // about `..`, and a containment check walking the un-normalised one
        // read a sibling as nested.
        assert_eq!(
            resolve_local_path(Path::new("/srv/ws"), Path::new("../out/notko")),
            PathBuf::from("/srv/out/notko")
        );
    }
}
