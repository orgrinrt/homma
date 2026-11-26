//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! What the estate has to get right about the two ecosystems it reads.

use super::*;

fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    for (rel, body) in files {
        let at = d.path().join(rel);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, body).unwrap();
    }
    d
}

fn sh(d: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .current_dir(d)
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}");
}

/// The files committed and tagged once, annotated.
fn tagged(files: &[(&str, &str)], tag: &str) -> tempfile::TempDir {
    let d = tree(files);
    let p = d.path();
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "t@t"],
        vec!["config", "user.name", "t"],
        vec!["config", "commit.gpgsign", "false"],
        vec!["config", "tag.gpgsign", "false"],
        vec!["add", "-A"],
        vec!["commit", "-q", "-m", "one"],
        vec!["tag", "-a", tag, "-m", tag],
    ] {
        sh(p, &args);
    }
    d
}

fn crates_pkg(manifest: &str) -> Package {
    Package {
        registry: Registry::CratesIo,
        name:     "x".into(),
        manifest: manifest.into(),
        version:  None,
    }
}

fn jsr_pkg() -> Package {
    Package {
        registry: Registry::Jsr,
        name:     "@meadow/x".into(),
        manifest: "deno.json".into(),
        version:  None,
    }
}

const WORKSPACE_INHERITING: &str = "[workspace]\nmembers = [\".\", \"inner\"]\n[workspace.package]\nversion = \"1.2.3\"\n[package]\nname = \"top\"\nversion.workspace = true\n";
const INNER: &str = "[package]\nname = \"inner\"\nversion.workspace = true\n";

#[test]
fn a_workspace_member_inherits_the_root_version() {
    let d = tree(&[("Cargo.toml", WORKSPACE_INHERITING), ("inner/Cargo.toml", INNER)]);
    let got: Vec<(String, Option<String>)> = cargo_packages(d.path())
        .into_iter()
        .map(|p| (p.name, p.version))
        .collect();
    assert_eq!(got, [
        ("top".to_string(), Some("1.2.3".to_string())),
        ("inner".to_string(), Some("1.2.3".to_string()))
    ]);
}

#[test]
fn publish_false_withholds_and_nothing_else_does() {
    let d = tree(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\".\", \"held\", \"listed\"]\n[package]\nname = \"top\"\nversion = \"0.1.0\"\n",
        ),
        (
            "held/Cargo.toml",
            "[package]\nname = \"held\"\nversion = \"0.1.0\"\npublish = false\n",
        ),
        (
            "listed/Cargo.toml",
            "[package]\nname = \"listed\"\nversion = \"0.1.0\"\npublish = [\"my-registry\"]\n",
        ),
    ]);
    let mut names: Vec<String> = cargo_packages(d.path())
        .into_iter()
        .map(|p| p.name)
        .collect();
    names.sort();
    assert_eq!(names, ["listed", "top"]);
}

#[test]
fn a_manifest_that_is_not_a_workspace_is_still_one_package() {
    let d = tree(&[(
        "Cargo.toml",
        "[package]\nname = \"lone\"\nversion = \"0.0.2\"\n",
    )]);
    let got: Vec<(String, Option<String>, String)> = cargo_packages(d.path())
        .into_iter()
        .map(|p| (p.name, p.version, p.manifest))
        .collect();
    assert_eq!(got, [(
        "lone".to_string(),
        Some("0.0.2".to_string()),
        "Cargo.toml".to_string()
    )]);
}

#[test]
fn a_directory_with_no_manifest_declares_nothing() {
    assert!(cargo_packages(tree(&[("README.md", "hi")]).path()).is_empty());
}

#[test]
fn a_scoped_package_is_read_when_the_scope_is_ours() {
    let d = tree(&[(
        "deno.json",
        r#"{"name": "@meadow/fx-gates", "version": "0.4.0"}"#,
    )]);
    assert_eq!(jsr_packages(d.path(), Some("meadow")), [Package {
        registry: Registry::Jsr,
        name:     "@meadow/fx-gates".into(),
        manifest: "deno.json".into(),
        version:  Some("0.4.0".into()),
    }]);
}

#[test]
fn someone_else_s_scope_is_not_ours_to_publish() {
    let d = tree(&[("deno.json", r#"{"name": "@std/path", "version": "1.0.0"}"#)]);
    assert!(jsr_packages(d.path(), Some("meadow")).is_empty());
    // with no scope configured, every name counts
    assert_eq!(jsr_packages(d.path(), None).len(), 1);
}

#[test]
fn a_manifest_with_no_name_declares_nothing() {
    let d = tree(&[("deno.json", r#"{"version": "1.0.0"}"#)]);
    assert!(jsr_packages(d.path(), None).is_empty());
}

#[test]
fn a_member_reads_the_root_version_out_of_the_same_tag() {
    let d = tagged(
        &[("Cargo.toml", WORKSPACE_INHERITING), ("inner/Cargo.toml", INNER)],
        "1.2.3",
    );
    assert_eq!(
        declared_at(d.path(), "1.2.3", &crates_pkg("inner/Cargo.toml")).unwrap(),
        Some("1.2.3".into())
    );
}

#[test]
fn the_working_tree_is_not_what_is_read() {
    let d = tagged(
        &[(
            "Cargo.toml",
            "[package]\nname = \"lone\"\nversion = \"1.2.3\"\n",
        )],
        "1.2.3",
    );
    std::fs::write(
        d.path().join("Cargo.toml"),
        "[package]\nname = \"lone\"\nversion = \"9.9.9\"\n",
    )
    .unwrap();
    assert_eq!(
        declared_at(d.path(), "1.2.3", &crates_pkg("Cargo.toml")).unwrap(),
        Some("1.2.3".into())
    );
}

#[test]
fn a_jsr_manifest_is_read_as_json_and_not_as_toml() {
    let d = tagged(
        &[("deno.json", r#"{"name": "@meadow/x", "version": "0.4.0"}"#)],
        "v0.4.0",
    );
    assert_eq!(
        declared_at(d.path(), "v0.4.0", &jsr_pkg()).unwrap(),
        Some("0.4.0".into())
    );
}

#[test]
fn a_manifest_absent_from_the_tag_is_none_rather_than_a_crash() {
    let d = tagged(
        &[(
            "Cargo.toml",
            "[package]\nname = \"lone\"\nversion = \"1.2.3\"\n",
        )],
        "1.2.3",
    );
    assert_eq!(
        declared_at(d.path(), "1.2.3", &crates_pkg("later/Cargo.toml")).unwrap(),
        None
    );
}

#[test]
fn a_manifest_that_is_not_toml_at_all_is_none() {
    let d = tagged(&[("Cargo.toml", "this is not toml [[[\n")], "1.2.3");
    assert_eq!(
        declared_at(d.path(), "1.2.3", &crates_pkg("Cargo.toml")).unwrap(),
        None
    );
}

#[test]
fn only_directories_that_are_repositories_are_read() {
    let root = tree(&[
        ("member/.git/HEAD", ""),
        (
            "member/Cargo.toml",
            "[package]\nname = \"m\"\nversion = \"0.1.0\"\n",
        ),
        (
            "loose/Cargo.toml",
            "[package]\nname = \"l\"\nversion = \"0.1.0\"\n",
        ),
    ]);
    let got: Vec<(String, String)> = packages_under(root.path(), None)
        .into_iter()
        .map(|(d, p)| {
            (
                d.file_name().unwrap().to_string_lossy().into_owned(),
                p.name,
            )
        })
        .collect();
    assert_eq!(got, [("member".to_string(), "m".to_string())]);
}

#[test]
fn a_repository_declaring_both_reports_both() {
    let root = tree(&[
        ("both/.git/HEAD", ""),
        (
            "both/Cargo.toml",
            "[package]\nname = \"c\"\nversion = \"0.1.0\"\n",
        ),
        (
            "both/deno.json",
            r#"{"name": "@meadow/d", "version": "0.2.0"}"#,
        ),
    ]);
    let mut got: Vec<&str> = packages_under(root.path(), Some("meadow"))
        .iter()
        .map(|(_, p)| p.registry.key())
        .collect();
    got.sort();
    assert_eq!(got, ["crates-io", "jsr"]);
}

fn tags(list: &str) -> Vec<String> {
    list.split_whitespace().map(str::to_string).collect()
}

#[test]
fn each_ecosystem_reads_its_own_prefix_and_neither_reads_the_other_s() {
    let t = tags("v0.1.0 v0.2.0 0.0.1 0.0.2 archive/old round/202608251345/end main");
    let names = |r| -> Vec<(String, String)> {
        tag_versions(&t, r)
            .into_iter()
            .map(|(t, v)| (t, v.to_string()))
            .collect()
    };
    assert_eq!(names(Registry::Jsr), [
        ("v0.1.0".to_string(), "0.1.0".to_string()),
        ("v0.2.0".to_string(), "0.2.0".to_string())
    ]);
    assert_eq!(names(Registry::CratesIo), [
        ("0.0.1".to_string(), "0.0.1".to_string()),
        ("0.0.2".to_string(), "0.0.2".to_string())
    ]);
}

#[test]
fn a_prerelease_sorts_below_its_own_release() {
    let got: Vec<String> = tag_versions(&tags("0.0.1 0.0.0-d01 0.0.0-d02"), Registry::CratesIo)
        .into_iter()
        .map(|(_, v)| v.to_string())
        .collect();
    assert_eq!(got, ["0.0.0-d01", "0.0.0-d02", "0.0.1"]);
}

fn vs(l: &[&str]) -> Vec<Version> {
    l.iter().map(|v| v.parse().unwrap()).collect()
}

#[test]
fn a_skipped_patch_is_a_jump_and_a_taken_one_is_not() {
    assert_eq!(jumps(&vs(&["0.0.1", "0.0.3"])), ["0.0.1 -> 0.0.3"]);
    assert!(jumps(&vs(&["0.0.1", "0.0.2", "0.1.0", "1.0.0"])).is_empty());
}

#[test]
fn prereleases_do_not_participate() {
    assert!(jumps(&vs(&["0.0.0-d01", "0.0.1", "0.0.2"])).is_empty());
}

#[test]
fn a_registry_holding_nothing_reads_as_nothing_published_and_no_latest() {
    let d = tagged(
        &[(
            "Cargo.toml",
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        )],
        "0.1.0",
    );
    let row = measure(d.path(), crates_pkg("Cargo.toml"), &Holding::default()).unwrap();
    assert!(row.published.is_empty());
    assert_eq!(row.latest, None);
    assert_eq!(row.missing, ["0.1.0"]);
    assert!(row.disagreeing.is_empty());
}

#[test]
fn a_yanked_version_is_not_missing() {
    let d = tagged(
        &[(
            "Cargo.toml",
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        )],
        "0.1.0",
    );
    let h = Holding {
        live:   vec![],
        yanked: vs(&["0.1.0"]),
    };
    assert!(
        measure(d.path(), crates_pkg("Cargo.toml"), &h)
            .unwrap()
            .missing
            .is_empty()
    );
}
