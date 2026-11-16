//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! A virtual workspace of crates versioned apart: the primary named as the
//! repo, released by its own tags, and the riders that publish only when their
//! manifests moved. The fixture is renki's shape in small.

use homma_api::Target;

use super::*;
use crate::release::{check, plan};

/// `<tmp>/w`, so the primary is the member named `w`, with `w-x` and `w-y`
/// riding; `w` depends on `w-x`.
struct Ws {
    _tmp:  tempfile::TempDir,
    root:  std::path::PathBuf,
    _bare: tempfile::TempDir,
}

impl Ws {
    fn root(&self) -> &Path {
        &self.root
    }

    fn git(&self, args: &[&str]) -> String {
        let out = sh::run(self.root(), "git", args).unwrap();
        assert!(out.ok(), "git {}: {}", args.join(" "), out.log());
        out.stdout.trim().to_string()
    }

    fn member(&self, name: &str, version: &str) {
        let dir = self.root().join("crates").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let deps = if name == "w" {
            "\n[dependencies]\nw-x = { path = \"../w-x\", version = \"0.2\" }\n"
        } else {
            ""
        };
        std::fs::write(
            dir.join("Cargo.toml"),
            format!(
                "[package]\nname = \"{name}\"\nversion = \"{version}\"\nedition = \"2024\"\n{deps}"
            ),
        )
        .unwrap();
    }

    fn commit_all(&self, subject: &str) {
        self.git(&["add", "."]);
        self.git(&["commit", "--quiet", "-m", subject]);
    }

    fn tag(&self, name: &str) {
        self.git(&["tag", "-a", name, "-m", name]);
        self.git(&["push", "--quiet", "origin", &format!("refs/tags/{name}")]);
    }

    /// Released at `0.1.0`, the tag bare as renki's are, with `w-x` at `0.2.0`
    /// and `w-y` at `0.3.0`, and one fix on `dev` since, the tree on `dev`.
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("w");
        std::fs::create_dir_all(&root).unwrap();
        let bare = tempfile::tempdir().unwrap();
        assert!(
            sh::run(bare.path(), "git", &["init", "--quiet", "--bare"])
                .unwrap()
                .ok()
        );
        let ws = Ws {
            _tmp: tmp,
            root,
            _bare: bare,
        };
        ws.git(&["init", "--quiet", "-b", "main"]);
        ws.git(&["config", "user.email", "t@t"]);
        ws.git(&["config", "user.name", "t"]);
        ws.git(&["config", "tag.gpgsign", "false"]);
        ws.git(&["config", "commit.gpgsign", "false"]);
        ws.git(&["remote", "add", "origin", ws._bare.path().to_str().unwrap()]);
        std::fs::write(
            ws.root().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\nresolver = \"3\"\n",
        )
        .unwrap();
        ws.member("w", "0.1.0");
        ws.member("w-x", "0.2.0");
        ws.member("w-y", "0.3.0");
        ws.commit_all("feat: first");
        ws.git(&["push", "--quiet", "origin", "main"]);
        ws.tag("0.1.0");
        ws.git(&["switch", "--quiet", "-c", "dev"]);
        std::fs::write(ws.root().join("crates/w/lib.rs"), "// w").unwrap();
        ws.commit_all("fix: a thing");
        ws.git(&["push", "--quiet", "origin", "dev"]);
        ws
    }

    fn check(&self, published: &Published, target: Option<Target>) -> Vec<Finding> {
        check::check(&check::Inputs {
            root: self.root(),
            repo: "w",
            remote: "origin",
            trunk: "dev",
            release: "main",
            target,
            published,
            markers: homma_api::Markers::defaults(),
        })
        .unwrap()
    }

    fn plan(&self, target: impl Into<Target>) -> Result<plan::Plan, plan::PlanError> {
        plan::plan(
            self.root(),
            "w",
            homma_api::Markers::defaults(),
            "dev",
            target,
            "2026-11-16",
        )
    }
}

fn on_crates_io(rows: &[(&str, &[&str])]) -> Published {
    let mut p = Published::default();
    for (name, versions) in rows {
        p.versions.insert(
            (Registry::CratesIo, (*name).to_string()),
            versions.iter().map(|v| v.parse().unwrap()).collect(),
        );
    }
    p
}

fn released() -> Published {
    on_crates_io(&[("w", &["0.1.0"]), ("w-x", &["0.2.0"]), ("w-y", &["0.3.0"])])
}

fn v(s: &str) -> Version {
    s.parse().unwrap()
}

#[test]
fn a_virtual_workspace_checks_each_crate_against_its_own_versions() {
    let ws = Ws::new();
    let findings = ws.check(&released(), Some(Level::Patch.into()));
    assert!(findings.is_empty(), "{findings:?}");
    // the primary's pre-release after its patch is one step, as renki's is
    let p =
        on_crates_io(&[("w", &["0.1.0", "0.1.1-rc.1"]), ("w-x", &["0.2.0"]), ("w-y", &["0.3.0"])]);
    let findings = ws.check(&p, None);
    let ids: Vec<&str> = findings.iter().map(|f| f.id.as_str()).collect();
    assert!(!ids.contains(&"semver.gaps"), "{findings:?}");
    // a rider's working version below its published one is its own finding
    let p = on_crates_io(&[("w", &["0.1.0"]), ("w-x", &["0.2.0", "0.2.1"]), ("w-y", &["0.3.0"])]);
    let findings = ws.check(&p, None);
    assert!(
        findings
            .iter()
            .any(|f| f.id == "man.current.forward" && f.message.contains("w-x")),
        "{findings:?}"
    );
}

#[test]
fn a_rider_bumped_on_main_untagged_or_tagged_and_unpublished_is_a_finding() {
    let ws = Ws::new();
    ws.git(&["switch", "--quiet", "main"]);
    ws.member("w", "0.1.1");
    ws.member("w-x", "0.2.1");
    ws.commit_all("chore: release 0.1.1");
    ws.git(&["push", "--quiet", "origin", "main"]);
    let p = on_crates_io(&[("w", &["0.1.0", "0.1.1"]), ("w-x", &["0.2.0"]), ("w-y", &["0.3.0"])]);
    let findings = ws.check(&p, None);
    assert!(
        findings
            .iter()
            .any(|f| f.id == "tag.bump.tagged" && f.message.contains("w-x")),
        "{findings:?}"
    );
    ws.tag("0.1.1");
    let findings = ws.check(&p, None);
    assert!(
        !findings.iter().any(|f| f.id == "tag.bump.tagged"),
        "{findings:?}"
    );
    let unpublished: Vec<_> = findings
        .iter()
        .filter(|f| f.id == "reg.unpublished")
        .collect();
    assert_eq!(unpublished.len(), 1, "{findings:?}");
    assert!(unpublished[0].message.contains("w-x"), "{findings:?}");
    assert!(unpublished[0].message.contains("0.2.1"), "{findings:?}");
    assert!(check::blocked(&findings));
    // published, the rider's version is the tagged bump's and nothing is left
    let p = on_crates_io(&[
        ("w", &["0.1.0", "0.1.1"]),
        ("w-x", &["0.2.0", "0.2.1"]),
        ("w-y", &["0.3.0"]),
    ]);
    let findings = ws.check(&p, None);
    assert!(
        !findings.iter().any(|f| f.id.starts_with("reg.")),
        "{findings:?}"
    );
}

#[test]
fn the_plan_bumps_the_primary_and_publishes_only_the_crates_that_moved() {
    let ws = Ws::new();
    let p = ws.plan(Level::Patch).unwrap();
    assert_eq!(p.current, v("0.1.0"));
    assert_eq!(p.next, v("0.1.1"));
    assert_eq!(p.tag, "0.1.1", "the primary's tags stay bare");
    assert_eq!(p.publishes, vec![(
        Registry::CratesIo,
        "w".to_string(),
        v("0.1.1")
    )]);
    let shown = p.to_string();
    assert!(shown.contains("`w-x` stays at 0.2.0"), "{shown}");
    assert!(shown.contains("`w-y` stays at 0.3.0"), "{shown}");
    assert!(shown.contains("w 0.1.1 to crates-io"), "{shown}");
    assert!(!p.changelog.contains("w-x"), "{}", p.changelog);
    // a rider bumped on dev rides along, ahead of the primary that needs it
    ws.member("w-x", "0.2.1");
    ws.commit_all("fix: x");
    let p = ws.plan(Level::Patch).unwrap();
    assert_eq!(p.publishes, vec![
        (Registry::CratesIo, "w-x".to_string(), v("0.2.1")),
        (Registry::CratesIo, "w".to_string(), v("0.1.1")),
    ]);
    assert!(p.to_string().contains("`w-x` moves from 0.2.0 to 0.2.1"));
    assert!(p.changelog.contains("- `w-x` 0.2.1"), "{}", p.changelog);
}

#[test]
fn a_set_with_no_member_named_as_the_repo_refuses_with_the_names() {
    let ws = Ws::new();
    std::fs::rename(ws.root().join("crates/w"), ws.root().join("crates/w-z")).unwrap();
    let text = std::fs::read_to_string(ws.root().join("crates/w-z/Cargo.toml")).unwrap();
    std::fs::write(
        ws.root().join("crates/w-z/Cargo.toml"),
        text.replace("name = \"w\"", "name = \"w-z\""),
    )
    .unwrap();
    let err = ws.plan(Level::Patch).unwrap_err();
    assert!(matches!(err, plan::PlanError::Layout(_)), "{err}");
    let said = err.to_string();
    assert!(said.contains("w-x") && said.contains("w-z"), "{said}");
}

#[test]
fn an_explicit_version_cuts_a_pre_release_the_manifest_already_carries() {
    let ws = Ws::new();
    ws.member("w", "0.1.1-rc.2");
    ws.commit_all("chore: name the candidate");
    ws.git(&["push", "--quiet", "origin", "dev"]);
    let p = ws.plan(v("0.1.1-rc.2")).unwrap();
    assert_eq!(p.current, v("0.1.1-rc.2"));
    assert_eq!(p.next, v("0.1.1-rc.2"));
    assert_eq!(p.tag, "0.1.1-rc.2");
    assert_eq!(p.publishes, vec![(
        Registry::CratesIo,
        "w".to_string(),
        v("0.1.1-rc.2")
    )]);
    // a level makes no pre-release, so the manifest is off it
    assert!(matches!(
        ws.plan(Level::Patch),
        Err(plan::PlanError::OffLevel(_))
    ));
    // and a version that is no single step above the last tag refuses
    for bad in ["0.1.3", "0.1.0", "0.0.9", "0.3.0-rc.1"] {
        let err = ws.plan(v(bad)).unwrap_err();
        assert!(
            matches!(err, plan::PlanError::NotOneStep { .. }),
            "{bad}: {err}"
        );
    }
    // the check takes the same target
    let findings = ws.check(&released(), Some(v("0.1.1-rc.2").into()));
    assert!(findings.is_empty(), "{findings:?}");
    let findings = ws.check(&released(), Some(v("0.1.1-rc.3").into()));
    assert!(
        findings.iter().any(|f| f.id == "man.current.smallest"),
        "{findings:?}"
    );
}

#[test]
fn the_whole_release_of_a_set_writes_the_primarys_manifest_and_publishes_it_alone() {
    let ws = Ws::new();
    let tip = ws.git(&["rev-parse", "dev"]);
    let runner = Fake(RefCell::new(Vec::new()));
    let forge = Recorder::default();
    let p = released();
    let w = client(&forge);
    let mut s = setup(&runner, &w, &p, "dev");
    s.repo = "w";
    let dry = release(&s, ws.root(), v("0.1.1-rc.1"), Some(&green_run(&tip)), true)
        .unwrap()
        .unwrap_err();
    assert_eq!(dry.next, v("0.1.1-rc.1"));
    assert_eq!(
        ws.git(&["rev-parse", "dev"]),
        tip,
        "a dry run moves nothing"
    );
    let done = release(&s, ws.root(), Level::Patch, Some(&green_run(&tip)), false)
        .unwrap()
        .unwrap();
    let changed = ws.git(&["show", "--name-only", "--format=", &done.bump_sha]);
    assert!(changed.contains("crates/w/Cargo.toml"), "{changed}");
    assert!(!changed.contains("crates/w-x"), "{changed}");
    assert_eq!(
        ws.git(&["show", "dev:Cargo.toml"]),
        "[workspace]\nmembers = [\"crates/*\"]\nresolver = \"3\"",
        "the root carries no version and is left alone"
    );
    assert!(
        ws.git(&["show", "dev:crates/w/Cargo.toml"])
            .contains("version = \"0.1.1\"")
    );
    assert_eq!(git::tag_target(ws.root(), "0.1.1").unwrap(), done.tag_sha);
    let ran = runner.0.borrow();
    let publishes: Vec<&String> = ran
        .iter()
        .filter(|l| l.starts_with("cargo publish"))
        .collect();
    assert_eq!(publishes, ["cargo publish -p w --locked"], "{ran:?}");
}

#[test]
fn the_primary_is_the_member_named_as_the_repository_is_named_not_the_directory() {
    let ws = Ws::new();
    // the clone sits in `w`, and the workspace manifest calls the repository `w-y`
    let layout =
        crate::release::workspace::layout(ws.root(), homma_api::RepoKind::Crate, "w-y").unwrap();
    assert!(
        matches!(&layout, crate::release::workspace::Layout::Set { primary, riders }
            if primary.name == "w-y" && riders.len() == 2),
        "{layout:?}"
    );
    // one publishable member named otherwise is the primary all the same
    let lone = tempfile::tempdir().unwrap();
    std::fs::write(
        lone.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/*\"]\n",
    )
    .unwrap();
    std::fs::create_dir_all(lone.path().join("crates/inner")).unwrap();
    std::fs::write(
        lone.path().join("crates/inner/Cargo.toml"),
        "[package]\nname = \"inner\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let layout =
        crate::release::workspace::layout(lone.path(), homma_api::RepoKind::Crate, "outer")
            .unwrap();
    assert!(
        matches!(&layout, crate::release::workspace::Layout::Set { primary, riders } if primary.name == "inner" && riders.is_empty()),
        "{layout:?}"
    );
}

#[test]
fn a_set_with_no_primary_is_a_finding_of_the_check() {
    let ws = Ws::new();
    let findings = check::check(&check::Inputs {
        root:      ws.root(),
        repo:      "nobody",
        remote:    "origin",
        trunk:     "dev",
        release:   "main",
        target:    None,
        published: &released(),
        markers:   homma_api::Markers::defaults(),
    })
    .unwrap();
    let primary: Vec<_> = findings.iter().filter(|f| f.id == "man.primary").collect();
    assert_eq!(primary.len(), 1, "{findings:?}");
    assert!(primary[0].severity.blocks());
    assert!(primary[0].message.contains("w-x"), "{findings:?}");
}

#[test]
fn a_patch_from_a_tagged_candidate_releases_the_candidates_version() {
    let ws = Ws::new();
    ws.git(&["switch", "--quiet", "main"]);
    ws.member("w", "0.1.1-rc.1");
    ws.commit_all("chore: release 0.1.1-rc.1");
    ws.git(&["push", "--quiet", "origin", "main"]);
    ws.tag("0.1.1-rc.1");
    ws.git(&["switch", "--quiet", "dev"]);
    ws.git(&["merge", "--quiet", "--no-edit", "main"]);
    let p = ws.plan(Level::Patch).unwrap();
    assert_eq!(
        p.next,
        v("0.1.1"),
        "a patch from a candidate is its release"
    );
    // a minor past the candidate leaves its version unreleased, which is a gap
    assert!(matches!(
        ws.plan(Level::Minor),
        Err(plan::PlanError::NotOneStep { .. })
    ));
}

#[test]
fn a_rider_moves_one_step_and_one_changed_without_a_bump_is_warned_of() {
    let ws = Ws::new();
    std::fs::write(ws.root().join("crates/w-x/lib.rs"), "// changed").unwrap();
    ws.commit_all("fix: x");
    let p = ws.plan(Level::Patch).unwrap();
    let shown = p.to_string();
    assert!(
        shown.contains("`w-x` changed since `0.1.0` and stays at 0.2.0"),
        "{shown}"
    );
    assert!(!shown.contains("`w-y` changed"), "{shown}");
    ws.member("w-x", "0.2.2");
    ws.commit_all("chore: skip one");
    let err = ws.plan(Level::Patch).unwrap_err();
    assert!(
        matches!(&err, plan::PlanError::NotOneStep { what, .. } if what.contains("w-x")),
        "{err}"
    );
    ws.member("w-x", "0.3.0-rc.1");
    ws.commit_all("chore: a candidate");
    assert!(ws.plan(Level::Patch).is_ok());
}

#[test]
fn a_first_release_named_outright_is_the_manifests_version_or_one_step_above() {
    let d = tempfile::tempdir().unwrap();
    let g = |args: &[&str]| assert!(sh::run(d.path(), "git", args).unwrap().ok());
    g(&["init", "--quiet", "-b", "main"]);
    g(&["config", "user.email", "t@t"]);
    g(&["config", "user.name", "t"]);
    g(&["config", "commit.gpgsign", "false"]);
    std::fs::write(
        d.path().join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    g(&["add", "."]);
    g(&["commit", "--quiet", "-m", "feat: first"]);
    let at = |s: &str| {
        plan::plan(
            d.path(),
            "x",
            homma_api::Markers::defaults(),
            "HEAD",
            v(s),
            "d",
        )
    };
    assert_eq!(at("0.1.0").unwrap().next, v("0.1.0"));
    assert_eq!(at("0.1.1-rc.1").unwrap().next, v("0.1.1-rc.1"));
    assert!(matches!(
        at("0.3.0"),
        Err(plan::PlanError::NotOneStep { .. })
    ));
    assert!(matches!(
        at("0.0.9"),
        Err(plan::PlanError::NotOneStep { .. })
    ));
}
