//! The real git behind [`homma_api::Git`].
//!
//! Every test in this module runs against an actual repository on disk, cloned
//! from a local path rather than over the network. A fake would prove that the
//! lifecycle calls these functions and would say nothing about whether the
//! identity landed where it has to land, which is the only interesting thing
//! about them.

use super::gix_impl::GixRepo;
use crate::repo::error::RepoError;
use homma_api::{AbsPath, CommitIdentity, Git};

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

    fn set_identity(&self, path: &AbsPath, id: &CommitIdentity) -> Result<(), Self::Error> {
        // The repository's own config file, edited in place. `gix`'s snapshot
        // API looked like the obvious route and is not: its `commit` updates
        // the in-memory `Repository` and never touches disk, so the identity
        // appeared to be set, read back correctly through the merged view, and
        // was absent from the file. The file is the only thing a later `git
        // commit` reads.
        //
        // All six keys, and the names are not optional extras: a **global**
        // `author.name` overrides a **local** `user.name`, so writing only the
        // `user.*` pair left a provisioned workspace committing under a name
        // homma never configured, on any machine carrying one. The same holds
        // for `author.email` against `user.email`.
        //
        // `user.name` and `user.email` are written as well as the specific four,
        // because git falls back to them for anything unset and a later tool
        // reading only those keys would otherwise see nothing.
        //
        // **`author.*` and `committer.*` need git 2.22.** Older git ignores them
        // entirely, and because `user.*` is written too there is no error: the
        // committer silently equals the author, which is the ordinary case for
        // every entry but one and wrong for that one.
        let file = local_config(path)?;
        let mut file = file;
        for (key, value) in [
            (&"user.name", &id.author_name),
            (&"user.email", &id.author_email),
            (&"author.name", &id.author_name),
            (&"author.email", &id.author_email),
            (&"committer.name", &id.committer_name),
            (&"committer.email", &id.committer_email),
        ] {
            file.set_raw_value(key, value.as_str())
                .map_err(|e| RepoError::Config(e.to_string()))?;
        }
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

    fn identity(&self, path: &AbsPath) -> Result<Option<CommitIdentity>, Self::Error> {
        let file = local_config(path)?;
        let get = |section: &str, key: &str| {
            file.raw_value(format!("{section}.{key}"))
                .ok()
                .map(|v| v.to_string())
                // An empty value is not a value, and the trait says `None` when
                // the clone configures none. Widening this method silently
                // dropped the filter, so a clone with empty values reported four
                // empty strings.
                .filter(|s| !s.is_empty())
        };
        // The author's own keys first, falling back to `user.*` the way git
        // does. Reading only `user.*` is what let a write that dropped the
        // committer pass the guard downstream.
        let author_name = get("author", "name").or_else(|| get("user", "name"));
        let author_email = get("author", "email").or_else(|| get("user", "email"));
        match (author_name, author_email) {
            (Some(an), Some(ae)) => Ok(Some(CommitIdentity {
                committer_name: get("committer", "name").unwrap_or_else(|| an.clone()),
                committer_email: get("committer", "email").unwrap_or_else(|| ae.clone()),
                author_name: an,
                author_email: ae,
            })),
            _ => Ok(None),
        }
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

    // U-3.1's exit test, and it reads the **commit** rather than the config.
    //
    // **Run against a hostile global configuration**, which is the half the
    // first version missed. It asserted the author was `ort@hiisi.digital`,
    // which is this machine's own global `user.email`, so deleting every author
    // write homma performs left the test passing. The machine was answering it.
    //
    // So the global and system files are pointed at one naming a different
    // author and committer, and homma's values have to beat them rather than
    // coincide with them. That also makes this runnable somewhere else, which it
    // was not.
    // Finding 8: widening `identity` silently dropped `.filter(|s| !s.is_empty())`,
    // so a clone configuring empty values reported four empty strings where the
    // trait says `None`. Neither case had a test.
    // The read side, which the previous round widened and swept nothing of.
    // Replacing both committer reads with the author's values left the whole
    // suite green, which restores exactly the hole the widening closed.
    // The order of the fallback, which is deliberate and was pinned by nothing:
    // reversing both reads to prefer `user.*` left the whole suite green. Git
    // prefers the specific keys over the general ones, and the comment at the
    // read site claims this does the same.
    #[test]
    fn the_specific_keys_win_over_the_user_pair() {
        let d = tempfile::tempdir().unwrap();
        let repo = d.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let at = AbsPath::new(&repo).unwrap();
        let git = GixGit;
        git.init(&at).unwrap();

        // Both pairs present and disagreeing, which is the only configuration
        // where the order is observable.
        let cfg = repo.join(".git").join("config");
        let mut text = std::fs::read_to_string(&cfg).unwrap();
        text.push_str(
            "[user]\n\tname = General\n\temail = general@example.invalid\n\
             [author]\n\tname = Specific\n\temail = specific@example.invalid\n",
        );
        std::fs::write(&cfg, text).unwrap();

        let got = git
            .identity(&at)
            .unwrap()
            .expect("an identity is configured");
        assert_eq!(
            got.author_name, "Specific",
            "author.name wins over user.name, which is what git does"
        );
        assert_eq!(got.author_email, "specific@example.invalid");
    }

    #[test]
    fn the_committer_is_read_from_its_own_keys_not_the_authors() {
        let d = tempfile::tempdir().unwrap();
        let repo = d.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let at = AbsPath::new(&repo).unwrap();
        let git = GixGit;
        git.init(&at).unwrap();

        git.set_identity(
            &at,
            &CommitIdentity {
                author_name: "Onni Armas".into(),
                author_email: "ort@hiisi.digital".into(),
                committer_name: "Vouti".into(),
                committer_email: "orgrinrt+vouti@ikiuni.dev".into(),
            },
        )
        .unwrap();

        let got = git
            .identity(&at)
            .unwrap()
            .expect("an identity is configured");
        assert_eq!(got.author_name, "Onni Armas");
        assert_eq!(got.author_email, "ort@hiisi.digital");
        assert_eq!(
            got.committer_name, "Vouti",
            "read from committer.name, not fabricated from the author"
        );
        assert_eq!(got.committer_email, "orgrinrt+vouti@ikiuni.dev");
    }

    // The `user.*` fallback, also live and also unswept. A clone configured by
    // hand, or by an older homma, carries only the `user.*` pair, and dropping
    // the fallback made `identity` report `None` for it.
    #[test]
    fn a_clone_carrying_only_the_user_pair_still_reports_an_identity() {
        let d = tempfile::tempdir().unwrap();
        let repo = d.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let at = AbsPath::new(&repo).unwrap();
        let git = GixGit;
        git.init(&at).unwrap();

        // Written by hand rather than through `set_identity`, which is the
        // shape this defends: homma did not configure this clone.
        let cfg = repo.join(".git").join("config");
        let mut text = std::fs::read_to_string(&cfg).unwrap();
        text.push_str("[user]\n\tname = By Hand\n\temail = hand@example.invalid\n");
        std::fs::write(&cfg, text).unwrap();

        let got = git
            .identity(&at)
            .unwrap()
            .expect("the user pair is an identity");
        assert_eq!(got.author_name, "By Hand");
        assert_eq!(got.author_email, "hand@example.invalid");
        assert_eq!(
            got.committer_name, "By Hand",
            "with no committer keys, the committer is the author"
        );
        assert_eq!(got.committer_email, "hand@example.invalid");
    }

    // The merged-view guarantee lives in `tests/reads_the_local_config.rs`.
    //
    // Its assertion says nothing unless a global configuration exists to be
    // wrongly reported, so it has to write one and point `GIT_CONFIG_GLOBAL` at
    // it, which is process-wide. In this binary that races with
    // `the_identity_is_written_to_the_local_config_file_and_nowhere_else` below,
    // which reads the same variable through `testing::global_config_paths`, and
    // whose own comment had already refused the trick for that reason. An
    // integration test is its own process, so the race cannot be written there.

    #[test]
    fn a_commit_carries_the_author_and_the_committer_separately() {
        let d = tempfile::tempdir().unwrap();
        let repo = d.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let at = AbsPath::new(&repo).unwrap();

        // Every key homma writes, set to something else, plus the two names it
        // did not write until this round: a global `author.name` overrides a
        // local `user.name`, which is how a workspace could commit under a name
        // homma never configured.
        let hostile = d.path().join("hostile.gitconfig");
        std::fs::write(
            &hostile,
            "[user]\n\tname = Somebody Else\n\temail = wrong@example.invalid\n\
             [author]\n\tname = Wrong Author\n\temail = wrong-author@example.invalid\n\
             [committer]\n\tname = Wrong Committer\n\temail = wrong-committer@example.invalid\n",
        )
        .unwrap();

        let git = GixGit;
        git.init(&at).unwrap();
        git.set_identity(
            &at,
            &CommitIdentity {
                author_name: "Onni Armas".into(),
                author_email: "ort@hiisi.digital".into(),
                committer_name: "Onni Armas".into(),
                committer_email: "orgrinrt+vouti@ikiuni.dev".into(),
            },
        )
        .unwrap();

        std::fs::write(repo.join("a"), "x").unwrap();
        let run = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(&repo)
                .env("GIT_CONFIG_GLOBAL", &hostile)
                .env("GIT_CONFIG_SYSTEM", &hostile)
                .output()
                .expect("git should run");
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        run(&["add", "a"]);
        run(&["commit", "-q", "-m", "one", "--no-gpg-sign"]);

        assert_eq!(
            run(&["log", "-1", "--format=%ae"]),
            "ort@hiisi.digital",
            "the author is op, per the record, and must beat the global"
        );
        assert_eq!(
            run(&["log", "-1", "--format=%ce"]),
            "orgrinrt+vouti@ikiuni.dev",
            "and the committer is the tagged address that distinguishes it"
        );
        assert_eq!(
            run(&["log", "-1", "--format=%an"]),
            "Onni Armas",
            "a global author.name overrides a local user.name, so the name is a hole too"
        );
        assert_eq!(run(&["log", "-1", "--format=%cn"]), "Onni Armas");
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

        git.set_identity(
            &abs_into,
            &CommitIdentity {
                author_name: "paja".into(),
                author_email: "paja@example.invalid".into(),
                committer_name: "paja".into(),
                committer_email: "paja@example.invalid".into(),
            },
        )
        .unwrap();
        assert_eq!(
            git.identity(&abs_into).unwrap(),
            Some(CommitIdentity {
                author_name: "paja".to_string(),
                author_email: "paja@example.invalid".to_string(),
                committer_name: "paja".to_string(),
                committer_email: "paja@example.invalid".to_string(),
            })
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
        git.set_identity(
            &abs_into,
            &CommitIdentity {
                author_name: "paja".into(),
                author_email: "paja@example.invalid".into(),
                committer_name: "paja".into(),
                committer_email: "paja@example.invalid".into(),
            },
        )
        .unwrap();

        let local = std::fs::read_to_string(into.join(".git/config")).unwrap();
        assert!(
            local.contains("paja@example.invalid"),
            "the identity must be in the clone's own config, got:\n{local}"
        );

        // **The `user.*` pair specifically**, which was written deliberately and
        // pinned by nothing: deleting both left the whole suite green. The
        // reason they exist is that git falls back to them for anything unset
        // and a later tool reading only those keys would otherwise see nothing,
        // so a test that only checks the four specific keys does not check the
        // guarantee the comment makes.
        let key = |k: &str| {
            let out = std::process::Command::new("git")
                .args(["config", "--local", "--get", k])
                .current_dir(&into)
                .output()
                .expect("git should run");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        assert_eq!(
            key("user.name"),
            "paja",
            "user.name is the documented fallback"
        );
        assert_eq!(key("user.email"), "paja@example.invalid");

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
