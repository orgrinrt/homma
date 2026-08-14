//! The real git behind [`homma_api::Git`].
//!
//! Every test in this module runs against an actual repository on disk, cloned
//! from a local path rather than over the network. A fake would prove that the
//! lifecycle calls these functions and would say nothing about whether the
//! identity landed where it has to land, which is the only interesting thing
//! about them.

use super::gix_impl::GixRepo;
use crate::repo::error::RepoError;
use homma_api::Git;
use std::path::Path;

/// Git as gix performs it.
#[derive(Debug, Default, Clone, Copy)]
pub struct GixGit;

impl Git for GixGit {
    type Error = RepoError;

    fn is_repo(&self, path: &Path) -> bool {
        // Opening is the honest check. A `.git` directory can exist and be
        // unusable, and this is called to decide whether to clone over it.
        path.join(".git").exists() && GixRepo::open(path).is_ok()
    }

    fn clone_repo(&self, url: &str, dest: &Path) -> Result<(), Self::Error> {
        GixRepo::clone_into(url, dest).map(|_| ())
    }

    fn set_identity(&self, path: &Path, name: &str, email: &str) -> Result<(), Self::Error> {
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
            path: config_path(path),
            source: e,
        })
    }

    fn init(&self, path: &Path) -> Result<(), Self::Error> {
        std::fs::create_dir_all(path).map_err(|e| RepoError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        gix::init(path)
            .map(|_| ())
            .map_err(|e| RepoError::Config(e.to_string()))
    }

    fn origin_url(&self, path: &Path) -> Result<Option<String>, Self::Error> {
        let file = local_config(path)?;
        Ok(file
            .raw_value("remote.origin.url")
            .ok()
            .map(|v| v.to_string())
            .filter(|s| !s.is_empty()))
    }

    fn identity(&self, path: &Path) -> Result<Option<(String, String)>, Self::Error> {
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

/// Where a repository keeps its own configuration.
fn config_path(repo: &Path) -> std::path::PathBuf {
    repo.join(".git").join("config")
}

/// The repository's own configuration, with no global or system file merged in.
fn local_config(repo: &Path) -> Result<gix::config::File<'static>, RepoError> {
    let path = config_path(repo);
    if !path.exists() {
        return Err(RepoError::Config(format!(
            "{} has no configuration file, so it is not a repository",
            repo.display()
        )));
    }
    gix::config::File::from_path_no_includes(path, gix::config::Source::Local)
        .map_err(|e| RepoError::Config(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

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

        let git = GixGit;
        git.clone_repo(src.path().to_str().unwrap(), &into).unwrap();
        assert!(git.is_repo(&into));

        git.set_identity(&into, "paja", "paja@example.invalid")
            .unwrap();
        assert_eq!(
            git.identity(&into).unwrap(),
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
        let globals = global_config_paths();
        // An empty list compares equal to an empty list, so the assertion below
        // would pass having checked nothing. Reported ok under
        // `env -u HOME -u XDG_CONFIG_HOME` before this line existed.
        assert!(
            !globals.is_empty(),
            "the comparison must have something to compare"
        );
        let before: Vec<_> = globals.iter().map(|p| std::fs::read(p).ok()).collect();

        let src = tempfile::tempdir().unwrap();
        source_repo(src.path());
        let dest = tempfile::tempdir().unwrap();
        let into = dest.path().join("clone");

        let git = GixGit;
        git.clone_repo(src.path().to_str().unwrap(), &into).unwrap();
        git.set_identity(&into, "paja", "paja@example.invalid")
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

    /// Every place git would look for a configuration that is not a
    /// repository's own.
    fn global_config_paths() -> Vec<std::path::PathBuf> {
        let mut out = vec![std::path::PathBuf::from("/etc/gitconfig")];
        if let Ok(explicit) = std::env::var("GIT_CONFIG_GLOBAL") {
            out.push(std::path::PathBuf::from(explicit));
        }
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            out.push(Path::new(&xdg).join("git").join("config"));
        }
        if let Ok(home) = std::env::var("HOME") {
            out.push(Path::new(&home).join(".gitconfig"));
            out.push(Path::new(&home).join(".config").join("git").join("config"));
        }
        out
    }

    #[test]
    fn the_origin_url_is_read_from_the_clone() {
        // This is where a content repository's clone URL comes from, so a wrong
        // answer here sends every later workspace to the wrong remote.
        let src = tempfile::tempdir().unwrap();
        source_repo(src.path());
        let dest = tempfile::tempdir().unwrap();
        let into = dest.path().join("clone");

        let git = GixGit;
        git.clone_repo(src.path().to_str().unwrap(), &into).unwrap();
        let url = git
            .origin_url(&into)
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
        assert_eq!(GixGit.origin_url(d.path()).unwrap(), None);
    }

    #[test]
    fn a_directory_that_is_not_a_repository_is_not_one() {
        let d = tempfile::tempdir().unwrap();
        assert!(!GixGit.is_repo(d.path()));
    }
}
