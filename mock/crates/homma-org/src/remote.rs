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

    // A `file://` URL is a path wearing a URL. Treating it as a host made
    // `file:///srv/content` a different repository from `/srv/content`.
    if let Some(rest) = trimmed.strip_prefix("file://") {
        return local(rest);
    }

    // scheme://[user@]host[:port]/owner/name
    if let Some((_scheme, rest)) = trimmed.split_once("://") {
        if let Some((authority, path)) = rest.split_once('/') {
            return hosted(authority, path);
        }
    }

    // scp-like: [user@]host:owner/name. One colon, and what follows is not an
    // absolute path. **The user is optional**; requiring it sent every bare
    // `host:path` to the local branch, where it could never compare equal to
    // the same repository written any other way.
    if let Some((authority, path)) = trimmed.split_once(':') {
        // `C:/src/thing` is a drive letter, not a host.
        let drive_letter = authority.len() == 1;
        if !path.starts_with('/') && !authority.contains('/') && !drive_letter {
            return hosted(authority, path);
        }
    }

    local(trimmed)
}

/// A path on this machine, resolved where it exists.
///
/// Resolved because two spellings of one directory are one directory; left as
/// written where it does not exist, so a path not yet created still compares to
/// itself.
fn local(path: &str) -> Remote {
    let path = std::path::PathBuf::from(strip_git_suffix(path));
    Remote::Local(std::fs::canonicalize(&path).unwrap_or(path))
}

/// Exactly one trailing `.git`.
///
/// `trim_end_matches` strips it repeatedly, which made `owner/name.git.git` the
/// same repository as `owner/name`. That one fails **open**, so it is the one
/// worth naming.
fn strip_git_suffix(s: &str) -> &str {
    s.strip_suffix(".git").unwrap_or(s)
}

fn hosted(authority: &str, path: &str) -> Remote {
    // Credentials are not the identity, and neither is the port: the same
    // repository reached on an explicit 22 or 443 is the same repository.
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = host.split(':').next().unwrap_or(host);

    let path = strip_git_suffix(path.trim_matches('/'));
    match path.rsplit_once('/') {
        Some((owner, name)) => Remote::Hosted {
            host: host.to_ascii_lowercase(),
            // A forge treats an owner case-insensitively, so two spellings of
            // one owner are one owner.
            owner: owner.trim_matches('/').to_ascii_lowercase(),
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
/// **Not derived from [`parse`].** That folds case so two spellings of one
/// owner compare equal, which is right for a comparison and wrong for a message:
/// a refusal that echoes an owner back in a case the operator did not type reads
/// as a second mistake on top of the first.
pub fn repo_name(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    let trimmed = trimmed.strip_prefix("file://").unwrap_or(trimmed);
    let after_authority = match trimmed.split_once("://") {
        Some((_, rest)) => rest.split_once('/').map(|(_, p)| p).unwrap_or(rest),
        None => match trimmed.split_once(':') {
            Some((authority, path))
                if !path.starts_with('/') && !authority.contains('/') && authority.len() > 1 =>
            {
                path
            }
            _ => trimmed,
        },
    };
    let path = strip_git_suffix(after_authority.trim_matches('/'));
    match path.rsplit_once('/') {
        // Two segments, which is an owner and a name on every forge worth
        // naming. More than two, and the last two are the part that identifies.
        Some((owner, name)) => match owner.rsplit_once('/') {
            Some((_, nearest)) => format!("{nearest}/{name}"),
            None => format!("{owner}/{name}"),
        },
        None => path.to_string(),
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
    fn a_port_is_not_part_of_the_identity() {
        assert!(same_repo(
            "ssh://git@github.com:22/orgrinrt/clause-dev.git",
            "git@github.com:orgrinrt/clause-dev.git"
        ));
        assert!(same_repo(
            "https://github.com:443/orgrinrt/clause-dev",
            "https://github.com/orgrinrt/clause-dev"
        ));
    }

    #[test]
    fn the_scp_form_needs_no_user() {
        // Requiring one sent every bare `host:path` to the local branch, where
        // it could never compare equal to the same repository written any
        // other way, so the guard refused a correct workspace permanently.
        assert!(same_repo(
            "github.com:orgrinrt/clause-dev.git",
            "https://github.com/orgrinrt/clause-dev"
        ));
    }

    #[test]
    fn a_file_url_is_the_path_it_names() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("content");
        std::fs::create_dir_all(&p).unwrap();
        assert!(same_repo(
            &format!("file://{}", p.display()),
            p.to_str().unwrap()
        ));
    }

    #[test]
    fn an_owner_differing_only_in_case_is_the_same_owner() {
        assert!(same_repo(
            "git@github.com:OrgrinRT/clause-dev.git",
            "git@github.com:orgrinrt/clause-dev.git"
        ));
    }

    #[test]
    fn only_one_git_suffix_is_stripped() {
        // Stripping repeatedly made these one repository, and that one fails
        // open: the guard would accept a workspace cloned from somewhere else.
        assert!(!same_repo(
            "git@github.com:orgrinrt/clause-dev.git.git",
            "git@github.com:orgrinrt/clause-dev.git"
        ));
    }

    #[test]
    fn a_windows_drive_letter_is_a_path_and_not_a_host() {
        assert!(matches!(parse("C:/src/content"), Remote::Local(_)));
    }

    #[test]
    fn a_path_of_more_than_two_segments_keeps_only_the_owner_and_the_name() {
        assert_eq!(
            parse("https://git.example.org/group/sub/thing.git"),
            Remote::Hosted {
                host: "git.example.org".into(),
                owner: "group/sub".into(),
                name: "thing".into(),
            }
        );
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
