//! The real git behind [`homma_api::Git`].
//!
//! Every test in this module runs against an actual repository on disk, cloned
//! from a local path rather than over the network. A fake would prove that the
//! lifecycle calls these functions and would say nothing about whether the
//! identity landed where it has to land, which is the only interesting thing
//! about them.

use super::gix_impl::GixRepo;
use crate::repo::error::RepoError;
use homma_api::{AbsPath, Git};

/// Git as gix performs it.
#[derive(Debug, Default, Clone, Copy)]
pub struct GixGit;

impl Git for GixGit {
    type Error = RepoError;

    fn is_repo(&self, path: &AbsPath) -> bool {
        // Opening is the honest check. A `.git` directory can exist and be
        // unusable, and this is called to decide whether to clone over it.
        path.join(".git").exists() && GixRepo::open(path).is_ok()
    }

    fn clone_repo(&self, url: &str, dest: &AbsPath) -> Result<(), Self::Error> {
        GixRepo::clone_into(url, dest).map(|_| ())
    }

    fn set_identity(&self, path: &AbsPath, name: &str, email: &str) -> Result<(), Self::Error> {
        // The repository's own config file, edited in place. `gix`'s snapshot
        // API looked like the obvious route and is not: its `commit` updates
        // the in-memory `Repository` and never touches disk, so the identity
        // appeared to be set, read back correctly through the merged view, and
        // was absent from the file. The file is the only thing a later `git
        // commit` reads.
        let file = local_config(path)?;
        let mut file = file;
        file.set_raw_value(&"user.name", name)
            .map_err(|e| RepoError::Config(e.to_string()))?;
        file.set_raw_value(&"user.email", email)
            .map_err(|e| RepoError::Config(e.to_string()))?;
        std::fs::write(config_path(path), file.to_bstring()).map_err(|e| RepoError::Io {
            path: config_path(path).into_path_buf(),
            source: e,
        })
    }

    fn init(&self, path: &AbsPath) -> Result<(), Self::Error> {
        std::fs::create_dir_all(path).map_err(|e| RepoError::Io {
            path: path.clone().into_path_buf(),
            source: e,
        })?;
        gix::init(path)
            .map(|_| ())
            .map_err(|e| RepoError::Config(e.to_string()))
    }

    fn enclosing_repo(&self, path: &AbsPath) -> Result<Option<AbsPath>, Self::Error> {
        // **Resolved before it is walked.** Containment is a property of the
        // filesystem and the path together, and five rounds computed it from
        // the path alone. A symlink anywhere in the chain then hides the
        // repository above it, and the walk answers about a place nobody asked
        // about. The path being created does not exist yet, so resolution walks
        // the components and follows each link it meets, dangling or not, and
        // takes what is left as written. An earlier comment here described
        // resolving the longest existing prefix, which is what `resolved` did
        // until a review found that `Path::exists()` follows a link and a
        // dangling one therefore read as absent.
        let subject = path.resolved().map_err(|e| RepoError::Io {
            path: path.clone().into_path_buf(),
            source: e,
        })?;
        let mut at = subject.clone();
        loop {
            // A path that is itself a repository is not inside one. Compared
            // here, where both sides are resolved; a caller comparing a
            // resolved ancestor against its own unresolved path never matches.
            if is_repo_dir(&at) && at != subject {
                return Ok(Some(at));
            }
            match at.parent() {
                Some(p) => at = p,
                None => return Ok(None),
            }
        }
    }

    fn origin_url(&self, path: &AbsPath) -> Result<Option<String>, Self::Error> {
        // A directory that is not a repository points at nothing, which is an
        // answer rather than a failure. Erroring here made the ordinary
        // bootstrap fail outright: a directory, a registry, and a remote to
        // clone from is exactly a root that is not yet a repository.
        if !config_path(path).exists() {
            return Ok(None);
        }
        let file = local_config(path)?;
        Ok(file
            .raw_value("remote.origin.url")
            .ok()
            .map(|v| v.to_string())
            .filter(|s| !s.is_empty()))
    }

    fn identity(&self, path: &AbsPath) -> Result<Option<(String, String)>, Self::Error> {
        // Deliberately the local file rather than the merged view. A merged
        // read would report the machine's global identity as though it were
        // this repository's, which is precisely the confusion being prevented.
        let file = local_config(path)?;
        let get = |k: &str| {
            file.raw_value(k)
                .ok()
                .map(|v| v.to_string())
                .filter(|s| !s.is_empty())
        };
        Ok(match (get("user.name"), get("user.email")) {
            (Some(n), Some(e)) => Some((n, e)),
            _ => None,
        })
    }
}

/// Whether a directory is a repository.
///
/// **A bare repository has no `.git`**, so testing for one alone answers no for
/// every bare repository there is, and a workspace created inside one is
/// exactly the write this guard exists to refuse. A bare repository is a `HEAD`
/// beside `objects` and `refs`, which is what `git` itself looks for.
fn is_repo_dir(path: &AbsPath) -> bool {
    if path.join(".git").exists() {
        return true;
    }
    path.join("HEAD").exists() && path.join("objects").is_dir() && path.join("refs").is_dir()
}

/// Where a repository keeps its own configuration.
fn config_path(repo: &AbsPath) -> AbsPath {
    repo.join(".git").join("config")
}

/// The repository's own configuration, with no global or system file merged in.
fn local_config(repo: &AbsPath) -> Result<gix::config::File<'static>, RepoError> {
    let path = config_path(repo);
    if !path.exists() {
        return Err(RepoError::Config(format!(
            "{} has no configuration file, so it is not a repository",
            repo.display()
        )));
    }
    gix::config::File::from_path_no_includes(path.into_path_buf(), gix::config::Source::Local)
        .map_err(|e| RepoError::Config(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::process::Command;

    /// A tempdir path as the type the contract takes.
    fn abs(p: impl Into<std::path::PathBuf>) -> AbsPath {
        AbsPath::new(p).expect("a tempdir is absolute")
    }

    /// A repository with one commit, to clone from. Built with the git binary
    /// because this is test scaffolding rather than the thing under test.
    fn source_repo(at: &Path) {
        let run = |args: &[&str]| {
            let ok = Command::new("git")
                .args(args)
                .current_dir(at)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null")
                .status()
                .expect("git should run")
                .success();
            assert!(ok, "git {args:?} failed");
        };
        run(&["init", "-q", "-b", "main"]);
        run(&["config", "user.name", "source"]);
        run(&["config", "user.email", "source@example.invalid"]);
        std::fs::write(at.join("README.md"), "content").unwrap();
        run(&["add", "README.md"]);
        run(&["commit", "-q", "-m", "initial", "--no-gpg-sign"]);
    }

    #[test]
    fn an_identity_lands_in_the_clones_own_config_and_can_be_read_back() {
        // The defect this prevents is silent: nothing fails, no test goes red,
        // and the first sign is a commit authored by whoever the machine
        // belongs to.
        let src = tempfile::tempdir().unwrap();
        source_repo(src.path());
        let dest = tempfile::tempdir().unwrap();
        let into = dest.path().join("clone");
        let abs_into = abs(into.clone());

        let git = GixGit;
        git.clone_repo(src.path().to_str().unwrap(), &abs_into)
            .unwrap();
        assert!(git.is_repo(&abs_into));

        git.set_identity(&abs_into, "paja", "paja@example.invalid")
            .unwrap();
        assert_eq!(
            git.identity(&abs_into).unwrap(),
            Some(("paja".to_string(), "paja@example.invalid".to_string()))
        );
    }

    #[test]
    fn the_identity_is_written_to_the_local_config_file_and_nowhere_else() {
        // Reading it back through gix would pass even if it had been written
        // globally, because gix reads the merged view. The file is the test.
        // Snapshotted rather than redirected. Mutating the environment to
        // point the global config elsewhere is racy across test threads and
        // needs an unsafe call; the requirement is that the real global file
        // does not change, so that is what is asserted, directly.
        let globals = crate::testing::global_config_paths();
        // An empty list compares equal to an empty list, so the assertion below
        // would pass having checked nothing. Reported ok under
        // `env -u HOME -u XDG_CONFIG_HOME` before this line existed.
        // Non-empty is not enough: `/etc/gitconfig` is pushed unconditionally
        // and does not exist here, so the comparison ran over `[None]` and
        // passed having checked nothing. At least one has to be a real file.
        assert!(
            globals.iter().any(|p| p.exists()),
            "no global configuration exists to compare against: {globals:?}"
        );
        let before: Vec<_> = globals.iter().map(|p| std::fs::read(p).ok()).collect();

        let src = tempfile::tempdir().unwrap();
        source_repo(src.path());
        let dest = tempfile::tempdir().unwrap();
        let into = dest.path().join("clone");
        let abs_into = abs(into.clone());

        let git = GixGit;
        git.clone_repo(src.path().to_str().unwrap(), &abs_into)
            .unwrap();
        git.set_identity(&abs_into, "paja", "paja@example.invalid")
            .unwrap();

        let local = std::fs::read_to_string(into.join(".git/config")).unwrap();
        assert!(
            local.contains("paja@example.invalid"),
            "the identity must be in the clone's own config, got:\n{local}"
        );

        // The half the name claimed and the body did not check. "Never
        // globally" is the entire content of the requirement, and reading the
        // local file says nothing about whether a global one was also written.
        let after: Vec<_> = globals.iter().map(|p| std::fs::read(p).ok()).collect();
        assert_eq!(
            before, after,
            "no global git configuration may change: {globals:?}"
        );
    }

    #[test]
    fn the_origin_url_is_read_from_the_clone() {
        // This is where a content repository's clone URL comes from, so a wrong
        // answer here sends every later workspace to the wrong remote.
        let src = tempfile::tempdir().unwrap();
        source_repo(src.path());
        let dest = tempfile::tempdir().unwrap();
        let into = dest.path().join("clone");
        let abs_into = abs(into.clone());

        let git = GixGit;
        git.clone_repo(src.path().to_str().unwrap(), &abs_into)
            .unwrap();
        let url = git
            .origin_url(&abs_into)
            .unwrap()
            .expect("a clone has an origin");
        // Canonicalised on both sides: the temp directory resolves through a
        // symlink on macOS, and comparing the two spellings tests the platform
        // rather than the function.
        assert_eq!(
            std::fs::canonicalize(&url).unwrap(),
            std::fs::canonicalize(src.path()).unwrap()
        );
    }

    #[test]
    fn a_repository_with_no_origin_reports_none_rather_than_guessing() {
        let d = tempfile::tempdir().unwrap();
        source_repo(d.path());
        assert_eq!(GixGit.origin_url(&abs(d.path())).unwrap(), None);
    }

    #[test]
    fn a_directory_inside_a_repository_reports_the_repository_above_it() {
        let d = tempfile::tempdir().unwrap();
        source_repo(d.path());
        let nested = d.path().join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        // Resolved on both sides: on macOS the tempdir is reached through a
        // symlink, so comparing the spellings would test the platform.
        let found = GixGit
            .enclosing_repo(&abs(nested.clone()))
            .unwrap()
            .unwrap();
        assert_eq!(
            std::fs::canonicalize(found).unwrap(),
            std::fs::canonicalize(d.path()).unwrap()
        );
    }

    #[test]
    fn the_walk_terminates_at_the_filesystem_root() {
        // A relative path can no longer reach here at all: `AbsPath` is the
        // parameter type, so the runtime refusal that used to live here is
        // gone and the case is pinned by `tests/compile_fail/` instead. What
        // remains worth checking is the other end of the walk.
        let root = AbsPath::new("/").unwrap();
        assert!(GixGit.enclosing_repo(&root).is_ok());
    }

    #[test]
    fn a_directory_that_is_not_a_repository_has_no_origin_rather_than_erroring() {
        // Erroring here made a URI content repository against a fresh root fail
        // outright, which is the ordinary bootstrap.
        let d = tempfile::tempdir().unwrap();
        assert_eq!(GixGit.origin_url(&abs(d.path())).unwrap(), None);
    }

    #[test]
    fn a_directory_outside_any_repository_reports_none() {
        let d = tempfile::tempdir().unwrap();
        let free = d.path().join("free");
        std::fs::create_dir_all(&free).unwrap();
        assert_eq!(GixGit.enclosing_repo(&abs(free.clone())).unwrap(), None);
    }

    #[test]
    fn a_repository_is_not_inside_itself() {
        // Standing up twice depends on this. Reporting itself made a workspace
        // refuse to be re-provisioned, since it appeared nested in a repository
        // that was it.
        let d = tempfile::tempdir().unwrap();
        source_repo(d.path());
        assert_eq!(GixGit.enclosing_repo(&abs(d.path())).unwrap(), None);
    }

    #[test]
    fn a_bare_repository_is_seen_as_an_ancestor() {
        // It has no `.git`, so testing for one answered no for every bare
        // repository there is.
        let d = tempfile::tempdir().unwrap();
        let bare = d.path().join("bare.git");
        std::process::Command::new("git")
            .args(["init", "-q", "--bare", bare.to_str().unwrap()])
            .status()
            .unwrap();
        let inside = bare.join("ws").join("hand");
        let found = GixGit.enclosing_repo(&abs(inside)).unwrap().unwrap();
        assert_eq!(
            std::fs::canonicalize(found).unwrap(),
            std::fs::canonicalize(&bare).unwrap()
        );
    }

    #[test]
    fn a_symlink_in_the_chain_does_not_hide_the_repository_above_it() {
        // Walking lexically, the link's own parents were inspected and the
        // repository the link points into was never seen.
        let d = tempfile::tempdir().unwrap();
        let victim = d.path().join("victim");
        std::fs::create_dir_all(victim.join("inside")).unwrap();
        source_repo(&victim);
        let elsewhere = d.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::os::unix::fs::symlink(victim.join("inside"), elsewhere.join("link")).unwrap();

        let target = elsewhere.join("link").join("hand");
        let found = GixGit.enclosing_repo(&abs(target)).unwrap().unwrap();
        assert_eq!(
            std::fs::canonicalize(found).unwrap(),
            std::fs::canonicalize(&victim).unwrap()
        );
    }

    #[test]
    fn a_directory_that_is_not_a_repository_is_not_one() {
        let d = tempfile::tempdir().unwrap();
        assert!(!GixGit.is_repo(&abs(d.path())));
    }
}
