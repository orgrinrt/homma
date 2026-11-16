//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The release against a forge whose rulesets take `dev` and `main` only
//! through a pull request: the fixture's origin refuses a push to either the
//! way GitHub does, and the routed fake lands a pull request in the origin
//! itself, a squash or a merge commit at the head it was given.

use super::*;

/// The words GitHub's ruleset refuses a push with.
const GH013: &str =
    "GH013: Repository rule violations found. Changes must be made through a pull request.";

/// The origin refuses every push to the named branches, as a ruleset does.
pub(super) fn refuse_pushes_to(f: &Fixture, branches: &[&str]) {
    let refs: Vec<String> = branches.iter().map(|b| format!("refs/heads/{b}")).collect();
    std::fs::create_dir_all(f.origin().join("hooks")).unwrap();
    let hook = f.origin().join("hooks/pre-receive");
    std::fs::write(
        &hook,
        format!(
            "#!/bin/sh\nwhile read -r old new ref; do\n  case \" {} \" in *\" $ref \"*) echo \"error: {GH013}\" >&2; exit 1 ;; esac\ndone\nexit 0\n",
            refs.join(" ")
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
}

pub(super) fn bare(origin: &Path, args: &[&str]) -> Result<String, String> {
    let mut all = vec!["-c", "user.name=forge", "-c", "user.email=forge@forge"];
    all.extend_from_slice(args);
    let out = sh::run(origin, "git", &all).map_err(|e| e.to_string())?;
    if out.ok() { Ok(out.stdout.trim().to_string()) } else { Err(out.log()) }
}

/// `pr <root> <head> <base> <title> <squash|merge> <sha>` done in the origin
/// as the forge does it, past the ruleset: refused where the head is not at
/// `sha`, a squash a commit of the head's tree on the base, a merge a merge
/// commit of the two.
pub(super) fn land_in(origin: &Path, args: &[&str]) -> Result<(), String> {
    let ["pr", _root, head, base, title, how, sha] = args else {
        return Err(format!(
            "forge-route: a pull request takes six arguments: {args:?}"
        ));
    };
    let at = bare(origin, &["rev-parse", &format!("refs/heads/{head}")])?;
    if at != *sha {
        return Err(format!("forge-route: pr: {head} is at {at}, not {sha}"));
    }
    let onto = bare(origin, &["rev-parse", &format!("refs/heads/{base}")])?;
    let made = match *how {
        "squash" => {
            bare(origin, &[
                "commit-tree",
                &format!("{sha}^{{tree}}"),
                "-p",
                &onto,
                "-m",
                title,
                // a body of its own, so the squash is never the clone's commit
                // under a fixed author and date
                "-m",
                "squashed by the forge",
            ])?
        },
        "merge" => {
            let tree = bare(origin, &["merge-tree", "--write-tree", &onto, sha])?;
            bare(origin, &[
                "commit-tree",
                &tree,
                "-p",
                &onto,
                "-p",
                sha,
                "-m",
                title,
            ])?
        },
        other => return Err(format!("forge-route: pr: no merge method {other}")),
    };
    bare(origin, &[
        "update-ref",
        &format!("refs/heads/{base}"),
        &made,
        &onto,
    ])
    .map(|_| ())
}

pub(super) fn routed<'a>(runner: &'a Routed<'a>) -> ByRoute<'a> {
    ByRoute {
        route: Path::new(ROUTE),
        runner,
    }
}

pub(super) fn pr_calls(runner: &Routed<'_>) -> Vec<String> {
    let pre = format!("{ROUTE} pr ");
    runner
        .calls
        .borrow()
        .iter()
        .filter(|(c, _)| c.starts_with(&pre))
        .map(|(c, _)| c.clone())
        .collect()
}

pub(super) fn subjects_on(f: &Fixture, rev: &str) -> Vec<String> {
    f.git(&["log", "--format=%s", rev])
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn a_trunk_that_takes_no_push_gets_the_bump_through_a_squashed_pull_request() {
    // Mutation: the refused push of `dev` reported as the run's failure, or
    // the clone's `dev` left on its own bump beside the forge's squash of it.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["dev"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Routed::new(&f);
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    let done = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false)
        .unwrap()
        .unwrap();
    let root = f.root().to_string_lossy().into_owned();
    let calls = pr_calls(&runner);
    assert_eq!(calls.len(), 1, "{calls:?}");
    let bump = f.git(&["rev-parse", "origin/release/x-0.1.1"]);
    assert_eq!(
        calls[0],
        format!("{ROUTE} pr {root} release/x-0.1.1 dev chore: release 0.1.1 squash {bump}")
    );
    assert_eq!(f.git(&["rev-parse", "origin/dev"]), done.bump_sha);
    assert_eq!(
        f.git(&["rev-parse", "dev"]),
        done.bump_sha,
        "the clone's dev is the forge's"
    );
    assert_eq!(f.git(&["rev-parse", "dev^"]), tip);
    assert_eq!(subjects_on(&f, "dev")[0], "chore: release 0.1.1");
    assert!(
        f.git(&["show", "dev:Cargo.toml"])
            .contains("version = \"0.1.1\"")
    );
    // main took its push, a merge of the forge's dev, and the tag is on it
    assert_eq!(git::parent_count(f.root(), "origin/main").unwrap(), 2);
    assert_eq!(f.git(&["rev-parse", "origin/main^2"]), done.bump_sha);
    assert_eq!(git::tag_target(f.root(), "v0.1.1").unwrap(), done.tag_sha);
    assert_eq!(
        git::current_branch(f.root()).unwrap().as_deref(),
        Some("dev")
    );
    assert!(git::is_clean(f.root()).unwrap());
}

#[test]
fn a_release_line_that_takes_no_push_gets_the_release_pull_request_merged() {
    // Mutation: the refused push of `main` reported as the run's failure, the
    // pull request squashed, or the tag put on the clone's own local merge
    // rather than on the commit the forge made.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["main"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let main_before = f.git(&["rev-parse", "main"]);
    let runner = Routed::new(&f);
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    let done = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false)
        .unwrap()
        .unwrap();
    let root = f.root().to_string_lossy().into_owned();
    assert_eq!(pr_calls(&runner), vec![format!(
        "{ROUTE} pr {root} dev main release: 0.1.1 merge {}",
        done.bump_sha
    )]);
    let main = f.git(&["rev-parse", "origin/main"]);
    assert_eq!(done.tag_sha, main);
    assert_eq!(
        f.git(&["rev-parse", "main"]),
        main,
        "the clone's main is the forge's"
    );
    assert_eq!(git::parent_count(f.root(), &main).unwrap(), 2);
    assert_eq!(f.git(&["rev-parse", &format!("{main}^1")]), main_before);
    assert_eq!(f.git(&["rev-parse", &format!("{main}^2")]), done.bump_sha);
    assert_eq!(subjects_on(&f, &main)[0], "release: 0.1.1");
    assert_eq!(git::tag_target(f.root(), "v0.1.1").unwrap(), main);
    assert_eq!(
        git::current_branch(f.root()).unwrap().as_deref(),
        Some("dev")
    );
    assert!(git::is_clean(f.root()).unwrap());
}

#[test]
fn a_rerun_after_a_stop_at_the_bump_lands_the_same_commit_and_makes_no_second() {
    // Mutation: the rerun refused on its own unpushed bump, gated or planned
    // on it, or bumping and committing again on top of it.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["dev", "main"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Routed::new(&f);
    *runner.pr_said.borrow_mut() = Some("forge-route: pr o/x release/x-0.1.1: gh: HTTP 502");
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    assert!(
        matches!(&err, ReleaseError::Route(said) if said.contains("HTTP 502")),
        "{err}"
    );
    let stopped = f.git(&["rev-parse", "dev"]);
    assert_eq!(
        f.git(&["rev-parse", "dev^"]),
        tip,
        "the stop left the bump on dev"
    );
    assert_eq!(f.git(&["rev-parse", "origin/dev"]), tip);
    *runner.pr_said.borrow_mut() = None;
    let done = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false)
        .unwrap()
        .unwrap();
    let root = f.root().to_string_lossy().into_owned();
    let calls = pr_calls(&runner);
    assert_eq!(
        calls.last().map(String::as_str),
        Some(
            format!(
                "{ROUTE} pr {root} dev main release: 0.1.1 merge {}",
                done.bump_sha
            )
            .as_str()
        )
    );
    assert!(
        calls.contains(&format!(
            "{ROUTE} pr {root} release/x-0.1.1 dev chore: release 0.1.1 squash {stopped}"
        )),
        "the commit the first run made is the one landed: {calls:?}"
    );
    let bumps = subjects_on(&f, "origin/dev")
        .into_iter()
        .filter(|s| s.starts_with("chore: release"))
        .count();
    assert_eq!(bumps, 1, "one bump commit on dev");
    assert_eq!(f.git(&["rev-parse", "origin/dev^"]), tip);
    assert_eq!(git::tag_target(f.root(), "v0.1.1").unwrap(), done.tag_sha);
    assert_eq!(f.git(&["rev-parse", "dev"]), done.bump_sha);
}

#[test]
fn a_rerun_planning_another_version_than_the_stopped_bump_refuses_and_puts_it_back() {
    // Mutation: a stopped `chore: release 0.1.1` taken back under a plan for
    // 0.2.0, or the commit left set aside when the run refuses.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["dev"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Routed::new(&f);
    *runner.pr_said.borrow_mut() = Some("forge-route: pr: gh: HTTP 502");
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    let stopped = f.git(&["rev-parse", "dev"]);
    *runner.pr_said.borrow_mut() = None;
    let err = release(&s, f.root(), Level::Minor, Some(&green_run(&tip)), false).unwrap_err();
    assert!(
        matches!(&err, ReleaseError::PendingBump { found, planned } if found == "chore: release 0.1.1" && planned == "0.2.0"),
        "{err}"
    );
    assert_eq!(
        f.git(&["rev-parse", "dev"]),
        stopped,
        "the stopped bump is back on dev"
    );
    // and a step before the bump refusing puts it back as well
    let err = release(&s, f.root(), Level::Patch, None, false).unwrap_err();
    assert!(matches!(err, ReleaseError::NoGreenRun { .. }), "{err}");
    assert_eq!(f.git(&["rev-parse", "dev"]), stopped);
}

#[test]
fn a_refused_push_with_no_route_stays_refused_with_its_reason() {
    // Mutation: the client's git push retried or reported as done.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["dev"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Fake(RefCell::new(Vec::new()));
    let forge = Recorder::default();
    let p = published();
    let w = client(&forge);
    let s = setup(&runner, &w, &p, "dev");
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    assert!(
        matches!(&err, ReleaseError::NoPullRequest(base) if base == "dev"),
        "{err}"
    );
    assert_eq!(f.git(&["rev-parse", "origin/dev"]), tip);
}

/// Another merge landed on `base` in the origin, an empty commit on its tip,
/// as one landing between the release's push and its pull request does.
pub(super) fn interlope(origin: &Path, base: &str) -> Result<(), String> {
    let onto = bare(origin, &["rev-parse", &format!("refs/heads/{base}")])?;
    let made = bare(origin, &[
        "commit-tree",
        &format!("{onto}^{{tree}}"),
        "-p",
        &onto,
        "-m",
        "fix: something else",
    ])?;
    bare(origin, &[
        "update-ref",
        &format!("refs/heads/{base}"),
        &made,
        &onto,
    ])
    .map(|_| ())
}

/// `delete-branch --queue <root> <branch> <head>` done in the origin as the
/// route does it: the branch deleted where it is at `head`, and a branch
/// already gone taken as deleted.
pub(super) fn delete_in(origin: &Path, branch: &str, head: &str) -> Result<(), String> {
    let r = format!("refs/heads/{branch}");
    if bare(origin, &["rev-parse", "--verify", "--quiet", &r]).is_err() {
        return Ok(());
    }
    bare(origin, &["update-ref", "-d", &r, head]).map(|_| ())
}
