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

/// Render a path with its no-op `.` components dropped, using `/` separators.
///
/// A `.` carries no information and every consumer downstream compares
/// textually: the aggregated wrapper's scope check, the workspace gate's
/// repo-table lookup, and anything else that joins the value to a root. So
/// `./arvo` joined to `/ws` gives `/ws/./arvo`, which is not a textual prefix
/// of `/ws/arvo/src/lib.rs`, and the check that was supposed to fire does not.
///
/// Normalising here rather than in the emitted shell is deliberate: the shell
/// would have to be fixed once per consumer, and each one would drift.
///
/// A path that normalises to nothing renders as `.`, because a repo whose
/// `local_path` is the workspace root is a repo at the workspace root rather
/// than a repo with no path at all.
pub(crate) fn relative_str(p: &Path) -> String {
    use std::path::Component;
    let mut out = String::new();
    for c in p.components() {
        match c {
            Component::CurDir => continue,
            Component::RootDir => {
                out.push('/');
                continue;
            },
            other => {
                if !out.is_empty() && !out.ends_with('/') {
                    out.push('/');
                }
                out.push_str(&other.as_os_str().to_string_lossy());
            },
        }
    }
    if out.is_empty() {
        out.push('.');
    }
    out
}

#[cfg(test)]
mod relative_str_tests {
    use super::*;

    #[test]
    fn a_curdir_component_is_dropped() {
        assert_eq!(relative_str(Path::new("./arvo")), "arvo");
        assert_eq!(relative_str(Path::new("a/./b")), "a/b");
    }

    #[test]
    fn a_path_with_nothing_to_drop_is_unchanged() {
        // The control. Without it a helper that rewrote paths generally would
        // satisfy the assertions above.
        assert_eq!(relative_str(Path::new("arvo")), "arvo");
        assert_eq!(relative_str(Path::new("a/b/c")), "a/b/c");
    }

    #[test]
    fn a_path_that_normalises_to_nothing_is_the_current_directory() {
        assert_eq!(relative_str(Path::new(".")), ".");
        assert_eq!(relative_str(Path::new("")), ".");
    }

    #[test]
    fn an_absolute_path_keeps_its_leading_separator() {
        // Both the wrapper and the gate branch on that separator to decide
        // whether to join against the workspace, so losing it silently changes
        // which arm runs.
        assert_eq!(
            relative_str(Path::new("/elsewhere/arvo")),
            "/elsewhere/arvo"
        );
        assert_eq!(relative_str(Path::new("/./a")), "/a");
    }

    #[test]
    fn a_climbing_component_is_kept_rather_than_resolved() {
        // This drops no-op components and nothing else. Collapsing `x/..`
        // would resolve a symlink the way the shell prints it rather than the
        // way the kernel walks it, which is a different decision than this
        // helper is entitled to make.
        assert_eq!(relative_str(Path::new("../arvo")), "../arvo");
    }
}
