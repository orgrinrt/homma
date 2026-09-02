//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Tests for the shared-config comparison.

use super::*;

const DENY: &str = "[bans]\nmultiple-versions = \"deny\"\n";
const NIGHTLY: &str = "[toolchain]\nchannel = \"nightly-2026-05-28\"\n";

/// A workspace with the canonical configs in it and repo directories to
/// compare, plus the `Root` a placement writes through.
struct Fixture {
    _dir:  tempfile::TempDir,
    _home: tempfile::TempDir,
    root:  Root,
    ws:    PathBuf,
}

impl Fixture {
    /// `configs` is `(tag directory, file name, body)`. An empty tag directory
    /// puts the file loose at the top level, which is the untagged case.
    fn new(configs: &[(&str, &str, &str)]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        // Resolved, because a temp dir on macOS is under `/var`, which is a
        // symlink to `/private/var`, and an unresolved root compares equal to
        // nothing beneath it.
        let ws = dir.path().canonicalize().unwrap();
        for (tag_dir, name, body) in configs {
            let at = if tag_dir.is_empty() {
                ws.join(CONFIGS_DIR)
            } else {
                ws.join(CONFIGS_DIR).join(tag_dir)
            };
            std::fs::create_dir_all(&at).unwrap();
            std::fs::write(at.join(name), body).unwrap();
        }
        std::fs::create_dir_all(ws.join(CONFIGS_DIR)).unwrap();
        // A real deny list, built from a home that is somewhere else, so the
        // test runs the code path that ships rather than a permissive variant
        // of it. The workspace is not under this home, so nothing here is
        // denied and the placements are the ones being tested.
        let home = tempfile::tempdir().unwrap();
        let home_abs = homma_api::AbsPath::new(home.path().canonicalize().unwrap()).unwrap();
        let abs = homma_api::AbsPath::new(ws.clone()).unwrap();
        let root = Root::new(&abs, homma_api::Denied::under_home(&home_abs)).unwrap();
        Self {
            _dir: dir,
            _home: home,
            root,
            ws,
        }
    }

    /// A repo directory under the workspace, Rust if `rust`.
    fn repo(&self, name: &str, rust: bool) -> ContainedPath {
        let at = self.ws.join(name);
        std::fs::create_dir_all(&at).unwrap();
        if rust {
            std::fs::write(at.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        }
        let abs = homma_api::AbsPath::new(at).unwrap();
        self.root.contain(&abs).unwrap()
    }

    /// Write a file inside an already-made repo, creating parents.
    fn put(&self, repo: &str, rel: &str, body: &str) {
        let at = self.ws.join(repo).join(rel);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, body).unwrap();
    }

    fn templates(&self) -> Vec<Template> {
        templates(&self.ws).unwrap()
    }

    /// Every path under a repo, sorted. What a read-only claim is checked with.
    fn listing(&self, repo: &str) -> Vec<String> {
        let base = self.ws.join(repo);
        let mut out = Vec::new();
        let mut stack = vec![base.clone()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap() {
                let p = e.unwrap().path();
                out.push(p.strip_prefix(&base).unwrap().display().to_string());
                if p.is_dir() {
                    stack.push(p);
                }
            }
        }
        out.sort();
        out
    }
}

#[test]
fn a_repo_missing_a_config_gets_it_and_the_bytes_are_the_templates() {
    // The control that matters: asserting `Placed` alone would pass on a stage
    // that reported placement and wrote nothing, which is the exact shape of a
    // fail-open. The file is read back.
    let f = Fixture::new(&[("rust_required", "deny.toml", DENY)]);
    let repo = f.repo("arvo", true);
    assert_eq!(ensure(&f.root, &repo, &f.templates()), vec![
        Finding::Placed("deny.toml".into())
    ]);
    assert_eq!(
        std::fs::read_to_string(repo.as_path().join("deny.toml")).unwrap(),
        DENY
    );
}

#[test]
fn a_repo_whose_copy_matches_is_reported_and_not_rewritten() {
    let f = Fixture::new(&[("rust_required", "deny.toml", DENY)]);
    let repo = f.repo("arvo", true);
    f.put("arvo", "deny.toml", DENY);
    let before = std::fs::metadata(repo.as_path().join("deny.toml"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(ensure(&f.root, &repo, &f.templates()), vec![
        Finding::Matches("deny.toml".into())
    ]);
    assert_eq!(
        std::fs::metadata(repo.as_path().join("deny.toml"))
            .unwrap()
            .modified()
            .unwrap(),
        before,
        "a matching config was written over"
    );
}

#[test]
fn a_repo_whose_copy_differs_keeps_its_own_and_is_only_reported() {
    // The whole asymmetry. A difference may be deliberate, so this says so and
    // changes nothing; a stage that "fixed" it would silently undo somebody's
    // exception.
    let f = Fixture::new(&[("rust_required", "deny.toml", DENY)]);
    let repo = f.repo("arvo", true);
    f.put(
        "arvo",
        "deny.toml",
        "[bans]\nmultiple-versions = \"warn\"\n",
    );
    let found = ensure(&f.root, &repo, &f.templates());
    assert_eq!(found, vec![Finding::Differs("deny.toml".into())]);
    assert_eq!(
        std::fs::read_to_string(repo.as_path().join("deny.toml")).unwrap(),
        "[bans]\nmultiple-versions = \"warn\"\n",
        "the repo's own config was overwritten"
    );
    assert!(
        !found[0].needs_a_human(),
        "a difference is a warning, not an error"
    );
    assert!(!found[0].blocks(), "a difference stopped a commit");
}

#[test]
fn a_repo_that_is_not_in_the_set_does_not_get_the_template() {
    let f = Fixture::new(&[
        ("rust_required", "deny.toml", "x\n"),
        ("any_required", "editorconfig", "y\n"),
    ]);
    let repo = f.repo("viola-grammar-ts", false);
    assert_eq!(ensure(&f.root, &repo, &f.templates()), vec![
        Finding::Placed("editorconfig".into())
    ]);
    assert!(
        !repo.as_path().join("deny.toml").exists(),
        "a TypeScript repo was given a Rust config"
    );
}

#[test]
fn a_rust_repo_under_mock_is_recognised_too() {
    // The shape most repos here have: the cargo workspace is under `mock/`, not
    // at the root. A check for a root `Cargo.toml` alone would decide that none
    // of them are Rust.
    let f = Fixture::new(&[("rust_required", "deny.toml", "x\n")]);
    let repo = f.repo("homma", false);
    f.put("homma", "mock/Cargo.toml", "[workspace]\n");
    assert_eq!(ensure(&f.root, &repo, &f.templates()), vec![
        Finding::Placed("deny.toml".into())
    ]);
}

#[test]
fn a_config_under_mock_satisfies_the_template() {
    // A repo that keeps its toolchain pin only under `mock/` is not missing
    // anything. Placing a second copy at the root would leave two pins that can
    // disagree, resolved by whichever is nearest the working directory.
    let f = Fixture::new(&[("rust_required", "rust-toolchain.toml", NIGHTLY)]);
    let repo = f.repo("vehje", false);
    f.put("vehje", "mock/Cargo.toml", "[workspace]\n");
    f.put("vehje", "mock/rust-toolchain.toml", NIGHTLY);
    assert_eq!(ensure(&f.root, &repo, &f.templates()), vec![
        Finding::Matches("rust-toolchain.toml".into())
    ]);
    assert!(
        !repo.as_path().join("rust-toolchain.toml").exists(),
        "a second pin was placed at the root beside the one under mock/"
    );

    // The control, so the pass above is not a check that always says satisfied.
    let bare = f.repo("tassu", true);
    assert_eq!(ensure(&f.root, &bare, &f.templates()), vec![
        Finding::Placed("rust-toolchain.toml".into())
    ]);
}

#[test]
fn a_differing_copy_under_mock_is_read_from_there() {
    // The other half of satisfying paths: the comparison has to read the copy
    // it found, not the one at the root that is not there.
    let f = Fixture::new(&[("rust_required", "rust-toolchain.toml", NIGHTLY)]);
    let repo = f.repo("vehje", false);
    f.put("vehje", "mock/Cargo.toml", "[workspace]\n");
    f.put(
        "vehje",
        "mock/rust-toolchain.toml",
        "[toolchain]\nchannel = \"nightly\"\n",
    );
    assert_eq!(inspect(repo.as_path(), &f.templates()), vec![
        Finding::Differs("rust-toolchain.toml".into())
    ]);
}

#[test]
fn a_template_nobody_has_placed_is_reported_rather_than_spread_everywhere() {
    let f = Fixture::new(&[("", "mystery.toml", "?\n")]);
    let repo = f.repo("arvo", true);
    let found = ensure(&f.root, &repo, &f.templates());
    assert_eq!(found, vec![Finding::CannotInfer("mystery.toml".into())]);
    assert!(
        found[0].needs_a_human(),
        "an unplaceable config must reach somebody"
    );
    assert!(
        !found[0].blocks(),
        "a fault in the shared directory stopped a commit in a repo that cannot fix it"
    );
    assert!(
        !repo.as_path().join("mystery.toml").exists(),
        "a template of unknown applicability was placed anyway"
    );
}

#[test]
fn an_unknown_template_the_repo_already_has_is_not_reported() {
    // The control on the case above: this asks for a decision it does not have,
    // and once the repo has the file there is no decision left.
    let f = Fixture::new(&[("", "mystery.toml", "?\n")]);
    let repo = f.repo("arvo", true);
    f.put("arvo", "mystery.toml", "anything\n");
    assert_eq!(ensure(&f.root, &repo, &f.templates()), vec![]);
}

#[test]
fn the_readme_beside_the_templates_is_not_one() {
    let f = Fixture::new(&[
        ("", "README.md", "# the configs\n"),
        ("rust_required", "README.md", "# the rust ones\n"),
        ("rust_required", "deny.toml", "x\n"),
    ]);
    let names: Vec<_> = f.templates().into_iter().map(|t| t.file_name).collect();
    assert_eq!(names, vec!["deny.toml".to_string()]);
}

#[test]
fn a_tag_directory_decides_who_wants_the_template() {
    let f = Fixture::new(&[
        ("rust_required", "deny.toml", "a\n"),
        ("deno_required", "deno-lint.json", "b\n"),
        ("any_required", "editorconfig", "c\n"),
    ]);
    let by_name = |n: &str| {
        f.templates()
            .into_iter()
            .find(|t| t.file_name == n)
            .unwrap()
    };
    assert_eq!(by_name("deny.toml").tags, vec![Tag {
        ecosystem: Ecosystem::Rust,
        severity:  Severity::Required,
    }]);
    assert_eq!(by_name("deno-lint.json").tags[0].ecosystem, Ecosystem::Deno);
    assert_eq!(by_name("editorconfig").tags[0].ecosystem, Ecosystem::Any);

    // And the repo only gets the one it is in the set for.
    let rust = f.repo("arvo", true);
    let placed: Vec<_> = ensure(&f.root, &rust, &f.templates())
        .into_iter()
        .map(|x| x.file_name().to_string())
        .collect();
    assert_eq!(placed, vec![
        "deny.toml".to_string(),
        "editorconfig".to_string()
    ]);
}

#[test]
fn a_severity_suffix_is_optional_and_required_is_the_default() {
    let f = Fixture::new(&[("rust", "deny.toml", "a\n")]);
    assert_eq!(f.templates()[0].tags[0].severity, Severity::Required);
    let repo = f.repo("arvo", true);
    assert!(inspect(repo.as_path(), &f.templates())[0].blocks());
}

#[test]
fn a_suggested_config_is_reported_and_does_not_block() {
    // What lets a new config reach every repo before it starts refusing
    // anything.
    let f = Fixture::new(&[("rust_suggested", "clippy.toml", "a\n")]);
    let repo = f.repo("arvo", true);
    let found = inspect(repo.as_path(), &f.templates());
    assert_eq!(found, vec![Finding::Missing(
        "clippy.toml".into(),
        Severity::Suggested
    )]);
    assert!(!found[0].blocks(), "a suggested config stopped a commit");
    // The control on the same shape at the other severity.
    let g = Fixture::new(&[("rust_required", "clippy.toml", "a\n")]);
    let repo = g.repo("arvo", true);
    assert!(inspect(repo.as_path(), &g.templates())[0].blocks());
}

#[test]
fn the_strongest_severity_among_the_sets_a_repo_is_in_wins() {
    // A repo in both sets is held to the stronger claim, because that is the
    // one somebody made.
    let f = Fixture::new(&[("rust_required+deno_suggested", "shared.json", "a\n")]);
    let both = f.repo("hybrid", true);
    f.put("hybrid", "deno.json", "{}\n");
    assert_eq!(inspect(both.as_path(), &f.templates()), vec![
        Finding::Missing("shared.json".into(), Severity::Required)
    ]);
    // Only deno: the weaker claim is the only one that applies.
    let deno = f.repo("scripts", false);
    f.put("scripts", "deno.json", "{}\n");
    assert_eq!(inspect(deno.as_path(), &f.templates()), vec![
        Finding::Missing("shared.json".into(), Severity::Suggested)
    ]);
    // Neither: nothing to say at all.
    let other = f.repo("plain", false);
    assert_eq!(inspect(other.as_path(), &f.templates()), vec![]);
}

#[test]
fn a_directory_naming_an_unknown_ecosystem_is_refused_at_load() {
    // Not skipped and not treated as untagged. A tag directory somebody spelled
    // wrong would otherwise turn a required config into a silently unplaced
    // one, which is the failure this whole directory exists to prevent.
    let f = Fixture::new(&[("ruby_required", "gemfile", "a\n")]);
    assert!(matches!(templates(&f.ws), Err(TemplateError::BadTag(..))));
}

#[test]
fn one_file_under_two_tag_directories_is_refused_at_load() {
    let f = Fixture::new(&[
        ("rust_required", "deny.toml", "a\n"),
        ("deno_suggested", "deny.toml", "b\n"),
    ]);
    assert!(matches!(templates(&f.ws), Err(TemplateError::Conflict(..))));
}

#[test]
fn a_tagged_file_conflicts_with_an_untagged_one_of_the_same_name() {
    let f = Fixture::new(&[("", "deny.toml", "a\n"), ("rust_required", "deny.toml", "b\n")]);
    assert!(matches!(templates(&f.ws), Err(TemplateError::Conflict(..))));
}

#[test]
fn one_directory_naming_an_ecosystem_twice_is_refused_at_load() {
    let f = Fixture::new(&[("rust_required+rust_suggested", "deny.toml", "a\n")]);
    assert!(matches!(templates(&f.ws), Err(TemplateError::BadTag(..))));
}

#[test]
fn a_missing_configs_directory_is_an_error_rather_than_an_empty_list() {
    // An empty list would make every repo pass, which is a check reporting
    // success because it could not run.
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        templates(dir.path()),
        Err(TemplateError::Missing(_))
    ));
}

#[test]
fn several_templates_are_each_answered_and_the_order_is_stable() {
    let f = Fixture::new(&[
        ("rust_required", "clippy.toml", "a\n"),
        ("rust_required", "deny.toml", "b\n"),
        ("any_required", "editorconfig", "c\n"),
    ]);
    let repo = f.repo("arvo", true);
    f.put("arvo", "deny.toml", "b\n");
    f.put("arvo", "editorconfig", "different\n");
    assert_eq!(ensure(&f.root, &repo, &f.templates()), vec![
        Finding::Placed("clippy.toml".into()),
        Finding::Matches("deny.toml".into()),
        Finding::Differs("editorconfig".into()),
    ]);
}

#[test]
fn a_rust_repo_on_stable_is_told_no_variant_fits_rather_than_given_the_wrong_one() {
    // The case the refinement relation exists for. A repo pinning nothing would
    // otherwise take a nightly-only config, print a warning per unstable option
    // and format to the defaults regardless. Silence would leave a Rust repo
    // with no formatting config and no sign of it.
    let f = Fixture::new(&[(
        "rust_nightly_required",
        "rustfmt.toml",
        "wrap_comments = true\n",
    )]);
    let repo = f.repo("tassu", true);
    let found = ensure(&f.root, &repo, &f.templates());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].file_name(), "rustfmt.toml");
    assert!(matches!(found[0], Finding::NoVariantFits(..)), "{found:?}");
    assert!(found[0].needs_a_human());
    assert!(
        !found[0].blocks(),
        "a repo was refused over a variant nobody has written and nobody there can write"
    );
    assert!(
        !repo.as_path().join("rustfmt.toml").exists(),
        "the nightly-only config was written into a stable repo anyway"
    );
}

#[test]
fn a_repo_outside_the_wider_set_is_not_told_about_the_variant() {
    // The other control: a near miss is for a repo that wants the config and
    // cannot have this copy, never for one the template was always going to
    // skip.
    let f = Fixture::new(&[(
        "rust_nightly_required",
        "rustfmt.toml",
        "wrap_comments = true\n",
    )]);
    let repo = f.repo("viola-grammar-ts", false);
    assert_eq!(ensure(&f.root, &repo, &f.templates()), vec![]);
}

#[test]
fn a_stable_repo_that_already_has_its_own_is_left_alone_and_not_reported() {
    let f = Fixture::new(&[(
        "rust_nightly_required",
        "rustfmt.toml",
        "wrap_comments = true\n",
    )]);
    let repo = f.repo("tassu", true);
    f.put("tassu", "rustfmt.toml", "max_width = 100\n");
    assert_eq!(ensure(&f.root, &repo, &f.templates()), vec![]);
    assert_eq!(
        std::fs::read_to_string(repo.as_path().join("rustfmt.toml")).unwrap(),
        "max_width = 100\n",
        "a repo's own config was overwritten"
    );
}

#[test]
fn a_pin_under_mock_counts_the_same_as_one_at_the_root() {
    let f = Fixture::new(&[(
        "rust_nightly_required",
        "rustfmt.toml",
        "wrap_comments = true\n",
    )]);
    let repo = f.repo("hilavitkutin", false);
    f.put("hilavitkutin", "mock/Cargo.toml", "[workspace]\n");
    assert!(matches!(
        ensure(&f.root, &repo, &f.templates())[0],
        Finding::NoVariantFits(..)
    ));
    f.put(
        "hilavitkutin",
        "mock/rust-toolchain.toml",
        "[toolchain]\nchannel = \"nightly\"\n",
    );
    assert_eq!(ensure(&f.root, &repo, &f.templates()), vec![
        Finding::Placed("rustfmt.toml".into())
    ]);
}

#[test]
fn only_a_missing_required_config_blocks() {
    // Asserted over every variant rather than over the one a fix happens to
    // produce, so a variant added later cannot quietly start or stop blocking.
    let cases = [
        (Finding::Matches("x".into()), false),
        (Finding::Differs("x".into()), false),
        (Finding::Missing("x".into(), Severity::Required), true),
        (Finding::Missing("x".into(), Severity::Suggested), false),
        (Finding::Placed("x".into()), false),
        (Finding::CannotInfer("x".into()), false),
        (Finding::NoVariantFits("x".into(), "why".into()), false),
        (Finding::Failed("x".into(), "why".into()), false),
    ];
    for (finding, want) in cases {
        assert_eq!(finding.blocks(), want, "{finding:?}");
    }
}

#[test]
fn everything_that_blocks_also_needs_a_human() {
    // The two predicates are separate and must not disagree: something that
    // stops a commit and is reported to nobody is a stop with no explanation
    // attached.
    for finding in [
        Finding::Matches("x".into()),
        Finding::Differs("x".into()),
        Finding::Missing("x".into(), Severity::Required),
        Finding::Missing("x".into(), Severity::Suggested),
        Finding::Placed("x".into()),
        Finding::CannotInfer("x".into()),
        Finding::NoVariantFits("x".into(), "why".into()),
        Finding::Failed("x".into(), "why".into()),
    ] {
        if finding.blocks() {
            assert!(
                finding.needs_a_human(),
                "{finding:?} blocks and is reported to nobody"
            );
        }
    }
}

#[test]
fn inspect_writes_nothing() {
    // Without this, read-only is a comment. The case chosen is the one that
    // would be placed, because that is the only one with anything to write.
    let f = Fixture::new(&[("rust_required", "deny.toml", DENY), ("", "mystery.toml", "?\n")]);
    let repo = f.repo("arvo", true);
    let before = f.listing("arvo");
    let found = inspect(repo.as_path(), &f.templates());
    assert_eq!(f.listing("arvo"), before, "inspect wrote into the repo");
    assert!(
        found.iter().any(|x| matches!(x, Finding::Missing(..))),
        "the case that would have been written was not exercised: {found:?}"
    );

    // The control: the same fixture through `ensure` does write, so the
    // assertion above is about `inspect` and not about a fixture nothing could
    // ever write to.
    ensure(&f.root, &repo, &f.templates());
    assert_ne!(
        f.listing("arvo"),
        before,
        "ensure wrote nothing either, so the read-only claim proves nothing"
    );
}

#[test]
fn ensure_and_inspect_agree_on_what_is_missing() {
    let f = Fixture::new(&[
        ("rust_required", "deny.toml", DENY),
        ("rust_suggested", "clippy.toml", "a\n"),
        ("any_required", "editorconfig", "c\n"),
        ("", "mystery.toml", "?\n"),
        ("rust_nightly_required", "rustfmt.toml", "b\n"),
    ]);
    let repo = f.repo("arvo", true);
    f.put("arvo", "editorconfig", "different\n");
    let seen = inspect(repo.as_path(), &f.templates());
    let missing: Vec<_> = seen
        .iter()
        .filter(|x| matches!(x, Finding::Missing(..)))
        .map(|x| x.file_name().to_string())
        .collect();
    let placed: Vec<_> = ensure(&f.root, &repo, &f.templates())
        .into_iter()
        .filter(|x| matches!(x, Finding::Placed(_)))
        .map(|x| x.file_name().to_string())
        .collect();
    assert_eq!(missing, placed);
    assert_eq!(missing, vec![
        "clippy.toml".to_string(),
        "deny.toml".to_string()
    ]);
}

#[test]
fn the_shared_rustfmt_config_is_the_reason_the_nightly_set_exists() {
    // The hand check, kept and re-run rather than written down as a number. The
    // shared copy is not merely a config we happen to use on nightly: most of
    // what it sets does not exist on stable at all, so a stable repo given it
    // formats to the defaults and warns once per option while doing so. That is
    // a property of the file, and the only thing that knows which options are
    // unstable is rustfmt itself.
    //
    // Skipped where a stable rustfmt is not installed. The set's behaviour is
    // covered by the cases above regardless; this one is about the actual file,
    // and a machine without stable cannot ask it.
    let base = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .join(CONFIGS_DIR);
    let Some(body) = ["rustfmt.toml", "rust_nightly_required/rustfmt.toml"]
        .iter()
        .find_map(|rel| std::fs::read(base.join(rel)).ok())
    else {
        return;
    };
    let probe = tempfile::tempdir().unwrap();
    std::fs::write(probe.path().join("rustfmt.toml"), &body).unwrap();
    std::fs::write(probe.path().join("x.rs"), "fn main() {}\n").unwrap();
    let Ok(out) = std::process::Command::new("rustfmt")
        .args(["+stable", "--check", "--edition", "2021", "x.rs"])
        .current_dir(probe.path())
        .output()
    else {
        return;
    };
    let complaints = String::from_utf8_lossy(&out.stderr);
    if complaints.contains("toolchain 'stable' is not installed")
        || complaints.contains("no such command")
    {
        return;
    }
    let unstable = complaints
        .lines()
        .filter(|l| l.contains("unstable features are only available in nightly"))
        .count();
    assert!(
        unstable >= 40,
        "only {unstable} of the shared rustfmt options are nightly-only, out of {}; if that is \
         real, a stable variant is now writable and the nightly set can go.\n{complaints}",
        String::from_utf8_lossy(&body)
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
            .count()
    );
}
