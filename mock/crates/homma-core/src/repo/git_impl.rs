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
    }

    #[test]
    fn a_directory_that_is_not_a_repository_is_not_one() {
        let d = tempfile::tempdir().unwrap();
        assert!(!GixGit.is_repo(d.path()));
    }
}
