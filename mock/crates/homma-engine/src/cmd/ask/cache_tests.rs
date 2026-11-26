//! The cache: when an entry expires, the cap, a write in flight, and where the cache sits.

use super::cache::Cache;
use super::tests::cache;

/// Two entries, `old` read `age` ago and `new` just now, each of `len` bytes.
fn aged(c: &Cache, age: std::time::Duration, len: usize) -> std::time::SystemTime {
    let now = std::time::SystemTime::now();
    c.keep_partial("old", &"o".repeat(len)).unwrap();
    c.keep_answer("new", &"n".repeat(len)).unwrap();
    stamp(c, "partials/old", now - age);
    stamp(c, "answers/new", now);
    now
}

/// Set an entry's last read to `at`, the way a read stamps it.
fn stamp(c: &Cache, rel: &str, at: std::time::SystemTime) {
    let path = c.dir().join(rel);
    std::fs::File::options()
        .append(true)
        .open(path)
        .unwrap()
        .set_modified(at)
        .unwrap();
}

const HOUR: std::time::Duration = std::time::Duration::from_secs(3600);

/// Failing control: with the age check taken out of `sweep` the old partial stays.
#[test]
fn an_entry_last_read_past_the_time_is_swept_and_one_within_it_stays() {
    let (_t, c) = cache();
    let now = aged(&c, 3 * HOUR, 4);
    c.sweep(now, 2 * HOUR, u64::MAX);
    assert!(!c.dir().join("partials/old").exists());
    assert_eq!(c.answer("new").as_deref(), Some("nnnn"));
}

/// Failing control: with `read` not stamping, the read entry is swept.
#[test]
fn a_read_stamps_an_old_entry_so_the_sweep_keeps_it() {
    let (_t, c) = cache();
    let now = aged(&c, 3 * HOUR, 4);
    assert_eq!(c.partial("old").as_deref(), Some("oooo"));
    c.sweep(now + std::time::Duration::from_secs(1), 2 * HOUR, u64::MAX);
    assert!(c.dir().join("partials/old").exists());
}

/// Failing control: ordering by name rather than by last read removes `new`.
#[test]
fn over_the_cap_the_least_recently_read_go_first() {
    let (_t, c) = cache();
    let now = aged(&c, HOUR, 10);
    c.sweep(now, 100 * HOUR, 15);
    assert!(!c.dir().join("partials/old").exists());
    assert!(c.dir().join("answers/new").exists());
    c.sweep(now, 100 * HOUR, 10);
    assert!(
        c.dir().join("answers/new").exists(),
        "at the cap is under it"
    );
}

/// Failing controls: counting dot files toward the cap removes `new`; skipping them before the age
/// check keeps the old one; removing them regardless of age takes the fresh one.
#[test]
fn a_write_in_flight_is_neither_counted_nor_swept_and_one_left_behind_expires() {
    let (_t, c) = cache();
    let now = aged(&c, 3 * HOUR, 4);
    let fresh = c.dir().join("partials/.fresh.4242");
    std::fs::write(&fresh, "x".repeat(64)).unwrap();
    stamp(&c, "partials/.fresh.4242", now);
    let left = c.dir().join("answers/.left.4243");
    std::fs::write(&left, "x").unwrap();
    stamp(&c, "answers/.left.4243", now - 3 * HOUR);
    c.sweep(now, 2 * HOUR, 8);
    assert!(!c.dir().join("partials/old").exists());
    assert!(fresh.exists(), "another ask's write is left alone");
    assert!(
        c.dir().join("answers/new").exists(),
        "a write in flight does not count toward the cap"
    );
    assert!(
        !left.exists(),
        "a write left behind goes once past the expiry"
    );
}

/// Failing control: with the removal taken out of `git_dir_query`, the variables are inherited.
#[test]
fn the_git_directory_is_asked_without_the_callers_git_variables() {
    let q = super::cache::git_dir_query(std::path::Path::new("/nowhere"));
    for var in ["GIT_DIR", "GIT_WORK_TREE"] {
        assert!(
            q.get_envs()
                .any(|(k, v)| k == std::ffi::OsStr::new(var) && v.is_none()),
            "{var} is not removed"
        );
    }
}

/// A repository at `at`, made without the caller's git variables.
fn git_init(at: &std::path::Path) {
    let git = std::process::Command::new("git")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .arg("-C")
        .arg(at)
        .args(["init", "-q"])
        .status()
        .unwrap();
    assert!(git.success());
}

/// Set only in the child the test below starts: the planted repository the child asks for.
const CHILD_ROOT: &str = "HOMMA_TEST_CACHE_CHILD_ROOT";

/// Failing control: with `of_workspace` asking a plain `git rev-parse --absolute-git-dir`, the
/// cache lands under the repository `GIT_DIR` names.
///
/// The variable is exported in a child of this test binary that runs this one test, so no other
/// test thread sees it.
#[test]
fn the_cache_is_under_its_own_repository_whatever_git_dir_the_caller_exports() {
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let root = std::path::Path::new(&root);
        let c = Cache::of_workspace(root).unwrap();
        assert!(
            c.dir().starts_with(root.canonicalize().unwrap()) || c.dir().starts_with(root),
            "the cache is the planted root's: {}",
            c.dir().display()
        );
        return;
    }
    let planted = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    git_init(planted.path());
    git_init(other.path());
    let out = homma_fixture::alone(
        module_path!(),
        "the_cache_is_under_its_own_repository_whatever_git_dir_the_caller_exports",
    )
    .env(CHILD_ROOT, planted.path())
    .env("GIT_DIR", other.path().join(".git"))
    .env_remove("GIT_WORK_TREE")
    .output()
    .unwrap();
    homma_fixture::passed(&out);
}

/// Failing control: with the sweep taken out of `run`, the old partial is still there.
#[test]
fn an_ask_sweeps_its_cache_before_it_answers_even_a_refused_one() {
    let ws = tempfile::tempdir().unwrap();
    git_init(ws.path());
    let c = Cache::of_workspace(ws.path()).unwrap();
    assert!(
        c.dir().starts_with(ws.path().canonicalize().unwrap()) || c.dir().starts_with(ws.path()),
        "the cache is the planted root's: {}",
        c.dir().display()
    );
    c.keep_partial("old", "o").unwrap();
    stamp(&c, "partials/old", std::time::SystemTime::now() - 3 * HOUR);
    let cfg = homma_core::Config::parse(&format!(
        "content_repo = \"c\"\n[workspace]\nname = \"w\"\npath = {:?}\n[ask]\ncache_expire_hours = 2\n",
        ws.path().display().to_string()
    ))
    .unwrap();
    let out = super::run(&cfg, ws.path(), &["anything".to_string()]).unwrap();
    assert!(
        matches!(out, crate::cmd::Outcome::ReportedFailure),
        "the pass is off by default, so the ask is refused"
    );
    assert!(!c.dir().join("partials/old").exists());
}
