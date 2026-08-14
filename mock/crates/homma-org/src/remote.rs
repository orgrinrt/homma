//! When two remote URLs name the same repository.
//!
//! Its own module because it is a pure function over strings with a table of
//! cases behind it, while `provision` is filesystem work. Mixing them meant the
//! comparison shipped with one line and no cases.

/// A remote, reduced to what identifies the repository it points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Remote {
    /// A repository on a host, which is what nearly every remote is.
    Hosted {
        host: String,
        /// Everything between the host and the name, which is an owner on every
        /// forge worth naming and occasionally a group above one.
        owner: String,
        name: String,
    },
    /// A path on this machine, resolved so that two spellings of one directory
    /// compare equal.
    Local(std::path::PathBuf),
}

/// Whether two remotes name the same repository.
///
/// **Not the last path segment.** Comparing that treats a fork as its upstream
/// and a mirror as the thing it mirrors, and a workspace keeping a public
/// repository beside a private one collides by design rather than by accident.
pub fn same_repo(a: &str, b: &str) -> bool {
    parse(a) == parse(b)
}

/// What a remote points at.
pub fn parse(url: &str) -> Remote {
    let trimmed = url.trim().trim_end_matches('/');

    // scp-like: user@host:owner/name.git, which has no scheme and one colon
    // before a path that does not start with a slash.
    if let Some((before, after)) = trimmed.split_once(':') {
        if !after.starts_with('/') && !before.contains('/') && before.contains('@') {
            let host = before.rsplit('@').next().unwrap_or(before);
            return hosted(host, after);
        }
    }

    // scheme://host/owner/name.git
    if let Some((_scheme, rest)) = trimmed.split_once("://") {
        if let Some((host, path)) = rest.split_once('/') {
            // Credentials in the authority are not part of the identity.
            let host = host.rsplit('@').next().unwrap_or(host);
            return hosted(host, path);
        }
    }

    // Anything else is a path. Resolved where it exists, because two spellings
    // of one directory are one directory; left as written where it does not, so
    // that a path which has not been created yet still compares to itself.
    let path = std::path::PathBuf::from(trimmed.trim_end_matches(".git"));
    Remote::Local(std::fs::canonicalize(&path).unwrap_or(path))
}

fn hosted(host: &str, path: &str) -> Remote {
    let path = path.trim_matches('/').trim_end_matches(".git");
    match path.rsplit_once('/') {
        Some((owner, name)) => Remote::Hosted {
            host: host.to_ascii_lowercase(),
            owner: owner.trim_matches('/').to_string(),
            name: name.to_string(),
        },
        // A host and a bare name, with nobody owning it.
        None => Remote::Hosted {
            host: host.to_ascii_lowercase(),
            owner: String::new(),
            name: path.to_string(),
        },
    }
}

/// The repository's own name, for a message that has to say which one.
pub fn repo_name(url: &str) -> String {
    match parse(url) {
        Remote::Hosted { owner, name, .. } if !owner.is_empty() => format!("{owner}/{name}"),
        Remote::Hosted { name, .. } => name,
        Remote::Local(p) => p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| p.to_string_lossy().into_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fork_is_not_its_upstream() {
        // The defect this module exists for. Comparing the last segment made
        // these equal, so a workspace cloned from the wrong one was accepted
        // and became permanent.
        assert!(!same_repo(
            "git@github.com:ownerA/content.git",
            "git@github.com:ownerB/content.git"
        ));
    }

    #[test]
    fn the_same_repository_in_three_spellings_is_one_repository() {
        let scp = "git@github.com:orgrinrt/clause-dev.git";
        let https = "https://github.com/orgrinrt/clause-dev.git";
        let no_suffix = "https://github.com/orgrinrt/clause-dev";
        assert!(same_repo(scp, https));
        assert!(same_repo(https, no_suffix));
        assert!(same_repo(scp, no_suffix));
    }

    #[test]
    fn the_same_name_on_two_hosts_is_two_repositories() {
        assert!(!same_repo(
            "git@github.com:orgrinrt/clause-dev.git",
            "git@codeberg.org:orgrinrt/clause-dev.git"
        ));
    }

    #[test]
    fn credentials_and_case_in_the_authority_are_not_the_identity() {
        assert!(same_repo(
            "https://token@GitHub.com/orgrinrt/clause-dev.git",
            "https://github.com/orgrinrt/clause-dev"
        ));
    }

    #[test]
    fn two_spellings_of_one_directory_are_one_directory() {
        let d = tempfile::tempdir().unwrap();
        let inner = d.path().join("content");
        std::fs::create_dir_all(&inner).unwrap();
        let indirect = d.path().join("elsewhere").join("..").join("content");
        std::fs::create_dir_all(d.path().join("elsewhere")).unwrap();
        assert!(same_repo(
            inner.to_str().unwrap(),
            indirect.to_str().unwrap()
        ));
    }

    #[test]
    fn two_local_paths_sharing_a_name_are_not_one_repository() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a").join("content");
        let b = d.path().join("b").join("content");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        assert!(!same_repo(a.to_str().unwrap(), b.to_str().unwrap()));
    }

    #[test]
    fn a_local_path_is_never_the_same_as_a_hosted_one() {
        assert!(!same_repo(
            "/srv/content",
            "git@github.com:orgrinrt/content.git"
        ));
    }

    #[test]
    fn a_name_is_reported_with_its_owner_so_a_message_can_tell_them_apart() {
        // The refusal message names what it expected and what it found, and
        // two bare names would read as the same thing twice.
        assert_eq!(
            repo_name("git@github.com:ownerA/content.git"),
            "ownerA/content"
        );
        assert_eq!(
            repo_name("git@github.com:ownerB/content.git"),
            "ownerB/content"
        );
    }
}
