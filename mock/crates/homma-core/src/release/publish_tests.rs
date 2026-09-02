//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! Publishing against a runner that records what it was asked, so no registry
//! and no token is touched.

use std::cell::RefCell;

use super::*;
use crate::release::sh;

struct Fake {
    fail_prefix: Option<&'static str>,
    seen:        RefCell<Vec<(String, Vec<(String, String)>)>>,
}

impl Fake {
    fn new(fail_prefix: Option<&'static str>) -> Self {
        Self {
            fail_prefix,
            seen: RefCell::new(Vec::new()),
        }
    }

    fn lines(&self) -> Vec<String> {
        self.seen.borrow().iter().map(|(l, _)| l.clone()).collect()
    }
}

impl Runner for Fake {
    fn run(
        &self,
        cwd: &Path,
        program: &str,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> Result<sh::Output, sh::Spawn> {
        let line = format!(
            "{} $ {program} {}",
            cwd.file_name().unwrap_or_default().to_string_lossy(),
            args.join(" ")
        );
        self.seen.borrow_mut().push((
            line.clone(),
            env.iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        ));
        let fails = self
            .fail_prefix
            .is_some_and(|p| format!("{program} {}", args.join(" ")).starts_with(p));
        Ok(sh::Output {
            program: program.into(),
            args:    args.iter().map(|a| a.to_string()).collect(),
            status:  Some(if fails { 1 } else { 0 }),
            stdout:  String::new(),
            stderr:  if fails { "refused".into() } else { String::new() },
        })
    }
}

fn token(r: Registry) -> Result<String, String> {
    Ok(format!("tok-{r}"))
}

fn no_token(_: Registry) -> Result<String, String> {
    Err("nothing stored".into())
}

fn served_now(_: Registry, _: &str, _: &Version) -> Result<bool, registry::Unreachable> {
    Ok(true)
}

fn never_served(_: Registry, _: &str, _: &Version) -> Result<bool, registry::Unreachable> {
    Ok(false)
}

fn workspace(members: &[(&str, &[&str])]) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let names: Vec<String> = members
        .iter()
        .map(|(n, _)| format!("\"crates/{n}\""))
        .collect();
    std::fs::write(
        d.path().join("Cargo.toml"),
        format!("[workspace]\nmembers = [{}]\n", names.join(", ")),
    )
    .unwrap();
    for (name, deps) in members {
        let dir = d.path().join("crates").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let mut text =
            format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n[dependencies]\n");
        for dep in *deps {
            text.push_str(&format!("{dep} = {{ path = \"../{dep}\" }}\n"));
        }
        std::fs::write(dir.join("Cargo.toml"), text).unwrap();
    }
    d
}

#[test]
fn crates_publish_in_dependency_order_with_the_alphabet_breaking_ties() {
    let d = workspace(&[("c", &["a", "b"]), ("a", &[]), ("b", &["a"]), ("z", &[])]);
    let names: Vec<String> = ["a", "b", "c", "z"].iter().map(|s| s.to_string()).collect();
    let order = crate_order(d.path(), &names).unwrap();
    let just: Vec<&str> = order.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(just, ["a", "b", "c", "z"]);
    assert!(order[2].1.ends_with("crates/c"));
}

#[test]
fn a_renamed_dependency_counts_by_its_package_name_and_a_cycle_is_named() {
    let d = workspace(&[("a", &[]), ("b", &[])]);
    std::fs::write(
        d.path().join("crates/b/Cargo.toml"),
        "[package]\nname = \"b\"\nversion = \"0.1.0\"\n[dependencies]\nalias = { package = \"a\", path = \"../a\" }\n",
    )
    .unwrap();
    let names = vec!["a".to_string(), "b".to_string()];
    let order = crate_order(d.path(), &names).unwrap();
    assert_eq!(order[0].0, "a");
    std::fs::write(
        d.path().join("crates/a/Cargo.toml"),
        "[package]\nname = \"a\"\nversion = \"0.1.0\"\n[dependencies]\nb = { path = \"../b\" }\n",
    )
    .unwrap();
    assert_eq!(crate_order(d.path(), &names).unwrap_err(), "a, b");
}

#[test]
fn a_dependency_outside_the_publishable_set_does_not_order_anything() {
    let d = workspace(&[("a", &["private"]), ("private", &[])]);
    let names = vec!["a".to_string()];
    let order = crate_order(d.path(), &names).unwrap();
    assert_eq!(order.len(), 1);
}

#[test]
fn a_crate_publishes_with_the_token_in_its_own_environment_and_waits() {
    let d = workspace(&[("a", &[])]);
    let fake = Fake::new(None);
    publish_crate(
        &fake,
        d.path(),
        "a",
        &Version::new(0, 1, 0),
        &token,
        &served_now,
    )
    .unwrap();
    let seen = fake.seen.borrow();
    assert!(
        seen[0].0.ends_with("$ cargo publish -p a --locked"),
        "{}",
        seen[0].0
    );
    assert_eq!(seen[0].1, vec![(
        "CARGO_REGISTRY_TOKEN".to_string(),
        "tok-crates-io".to_string()
    )]);
    assert!(
        std::env::var("CARGO_REGISTRY_TOKEN").is_err(),
        "nothing leaked into this process"
    );
}

#[test]
fn no_token_refuses_before_running_anything_and_a_refused_publish_carries_its_log() {
    let d = workspace(&[("a", &[])]);
    let fake = Fake::new(None);
    let err = publish_crate(
        &fake,
        d.path(),
        "a",
        &Version::new(0, 1, 0),
        &no_token,
        &served_now,
    )
    .unwrap_err();
    assert!(matches!(err, PublishError::NoToken(Registry::CratesIo, _)));
    assert!(fake.lines().is_empty());
    let fake = Fake::new(Some("cargo publish"));
    let err = publish_crate(
        &fake,
        d.path(),
        "a",
        &Version::new(0, 1, 0),
        &token,
        &served_now,
    )
    .unwrap_err();
    match err {
        PublishError::Failed {
            command,
            log,
        } => {
            assert_eq!(command, "cargo publish -p a --locked");
            assert!(log.contains("refused"));
        },
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_version_the_registry_never_serves_is_reported_after_the_wait() {
    let d = workspace(&[("a", &[])]);
    let fake = Fake::new(None);
    let err = publish_crate(
        &fake,
        d.path(),
        "a",
        &Version::new(0, 1, 0),
        &token,
        &never_served,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        PublishError::NotServed(Registry::CratesIo, ..)
    ));
    let calls = RefCell::new(0);
    let eventually = |_: Registry, _: &str, _: &Version| {
        *calls.borrow_mut() += 1;
        Ok(*calls.borrow() >= 3)
    };
    wait_until_served(Registry::Jsr, "x", &Version::new(1, 0, 0), &eventually).unwrap();
    assert_eq!(*calls.borrow(), 3);
}

#[test]
fn jsr_takes_the_token_on_its_arguments_and_npm_builds_then_publishes_from_npm_dir() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(
        d.path().join("deno.json"),
        r#"{"name": "@h/x", "version": "0.1.0", "tasks": {"build:npm": "deno run -A build.ts"}}"#,
    )
    .unwrap();
    std::fs::create_dir(d.path().join("npm")).unwrap();
    std::fs::write(d.path().join("npm/package.json"), "{}").unwrap();
    let fake = Fake::new(None);
    publish_jsr(
        &fake,
        d.path(),
        "@h/x",
        &Version::new(0, 1, 0),
        &token,
        &served_now,
    )
    .unwrap();
    publish_npm(
        &fake,
        d.path(),
        "x",
        &Version::new(0, 1, 0),
        &token,
        &served_now,
    )
    .unwrap();
    let seen = fake.seen.borrow();
    assert!(
        seen[0].0.ends_with("$ deno publish --token tok-jsr"),
        "the tool takes it on its arguments and nowhere else: {}",
        seen[0].0
    );
    assert!(!seen[0].0.contains("--allow-dirty"));
    assert!(
        seen[1].0.ends_with("$ deno task build:npm"),
        "{}",
        seen[1].0
    );
    assert!(
        seen[2].0.starts_with("npm $ npm publish --access public"),
        "{}",
        seen[2].0
    );
    assert_eq!(seen[2].1[0].0, "NPM_CONFIG_USERCONFIG");
    assert!(
        !Path::new(&seen[2].1[0].1).exists(),
        "the npmrc is removed after the call"
    );
}

#[test]
fn npm_without_a_build_task_or_an_npm_dir_publishes_from_the_root() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("deno.json"), r#"{"name": "@h/x"}"#).unwrap();
    std::fs::write(d.path().join("package.json"), "{}").unwrap();
    let fake = Fake::new(None);
    publish_npm(
        &fake,
        d.path(),
        "x",
        &Version::new(0, 1, 0),
        &token,
        &served_now,
    )
    .unwrap();
    let lines = fake.lines();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("$ npm publish"));
    assert!(!lines[0].starts_with("npm $"));
}

#[test]
fn the_npmrc_is_readable_by_its_owner_alone_while_it_exists() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("deno.json"), r#"{"name": "@h/x"}"#).unwrap();
    std::fs::write(d.path().join("package.json"), "{}").unwrap();
    struct Peek(RefCell<Option<u32>>);
    impl Runner for Peek {
        fn run(
            &self,
            _: &Path,
            program: &str,
            args: &[&str],
            env: &[(&str, &str)],
        ) -> Result<sh::Output, sh::Spawn> {
            let path = env
                .iter()
                .find(|(k, _)| *k == "NPM_CONFIG_USERCONFIG")
                .map(|(_, v)| *v)
                .unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                *self.0.borrow_mut() =
                    Some(std::fs::metadata(path).unwrap().permissions().mode() & 0o777);
            }
            let text = std::fs::read_to_string(path).unwrap();
            assert_eq!(text, "//registry.npmjs.org/:_authToken=tok-npm\n");
            Ok(sh::Output {
                program: program.into(),
                args:    args.iter().map(|a| a.to_string()).collect(),
                status:  Some(0),
                stdout:  String::new(),
                stderr:  String::new(),
            })
        }
    }
    let peek = Peek(RefCell::new(None));
    publish_npm(
        &peek,
        d.path(),
        "x",
        &Version::new(0, 1, 0),
        &token,
        &served_now,
    )
    .unwrap();
    #[cfg(unix)]
    assert_eq!(*peek.0.borrow(), Some(0o600));
}

#[test]
fn a_failed_jsr_publish_reports_its_command_and_log_with_the_token_redacted() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("deno.json"), r#"{"name": "@h/x"}"#).unwrap();
    struct Echoing;
    impl Runner for Echoing {
        fn run(
            &self,
            _: &Path,
            program: &str,
            args: &[&str],
            _: &[(&str, &str)],
        ) -> Result<sh::Output, sh::Spawn> {
            Ok(sh::Output {
                program: program.into(),
                args:    args.iter().map(|a| a.to_string()).collect(),
                status:  Some(1),
                stdout:  String::new(),
                stderr:  format!("error: token tok-jsr was refused\n"),
            })
        }
    }
    let err = publish_jsr(
        &Echoing,
        d.path(),
        "@h/x",
        &Version::new(0, 1, 0),
        &token,
        &served_now,
    )
    .unwrap_err();
    let text = err.to_string();
    assert!(!text.contains("tok-jsr"), "{text}");
    assert!(text.contains("deno publish --token <token>"), "{text}");
    assert!(text.contains("token <token> was refused"), "{text}");
}
