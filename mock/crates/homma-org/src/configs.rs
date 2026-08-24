//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The tool configs every repo is meant to share, and whether it has them.
//!
//! `rustfmt.toml`, `deny.toml` and `.taplo.toml` are the same file in every repo
//! that has one. What differs between repos is only whether one got copied, and
//! a repo without a `rustfmt.toml` formats to rustfmt's defaults rather than
//! ours, which surfaces months later as noise inside somebody's unrelated diff.
//!
//! So the canonical copies live in one place and this compares against them.
//!
//! **Absence is acted on and difference is not.** A missing config is a fact:
//! nothing was decided, and placing the template is what somebody would have
//! done by hand. A config that differs is a question this module cannot answer,
//! because a deliberate exception and a drifted copy look identical on disk. It
//! is reported and left exactly as it is.

use std::path::{Path, PathBuf};

use homma_api::{ContainedPath, Root};

/// Where the canonical copies live, relative to the workspace root.
pub const CONFIGS_DIR: &str = ".shared/configs";

/// Which repos a template belongs in.
///
/// Named cases rather than a closure, so the set is data a test can enumerate
/// and a reader can see without following a function pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applies {
    /// Anything with a `Cargo.toml` at the root or under `mock/`.
    RustRepo,
    /// A Rust repo whose toolchain is pinned to a nightly.
    ///
    /// `rustfmt.toml` needs this and the others do not. The shared copy sets
    /// options rustfmt only accepts on nightly, so a stable repo given it
    /// formats to the defaults anyway and prints a warning per option while
    /// doing so. A Rust repo on stable therefore gets [`Finding::NoVariantFits`]
    /// rather than the file, until somebody writes the stable variant.
    NightlyRustRepo,
    /// Every repo, whatever it is written in.
    AnyRepo,
    /// **Not known, and therefore not placed.** A template nobody has said
    /// where to put. Reported for a human rather than guessed at, because a
    /// config in the wrong repo is quieter than one that is missing.
    Unknown,
}

impl Applies {
    /// What a template's filename says about where it belongs.
    ///
    /// Deliberately a short closed list. A new template lands here in the same
    /// change that adds it, and until it does the stage says so instead of
    /// spreading it everywhere.
    pub fn of(file_name: &str) -> Self {
        match file_name {
            "rustfmt.toml" => Self::NightlyRustRepo,
            "deny.toml" | "clippy.toml" => Self::RustRepo,
            ".taplo.toml" | "taplo.toml" => Self::AnyRepo,
            _ => Self::Unknown,
        }
    }

    /// Whether a repo at `dir` wants a template carrying this predicate.
    pub fn wants(self, dir: &Path) -> bool {
        match self {
            Self::AnyRepo => true,
            Self::RustRepo => is_a_rust_repo(dir),
            Self::NightlyRustRepo => is_a_rust_repo(dir) && is_pinned_to_nightly(dir),
            Self::Unknown => false,
        }
    }
}

fn is_a_rust_repo(dir: &Path) -> bool {
    dir.join("Cargo.toml").is_file() || dir.join("mock").join("Cargo.toml").is_file()
}

/// Whether a repo commits a `rust-toolchain.toml` naming a nightly channel.
///
/// Both the root and `mock/` are read, because a repo carrying a Cargo
/// workspace under `mock/` pins there and sometimes only there. Either one
/// naming a nightly is enough: the question is whether nightly rustfmt is what
/// runs, and a repo pinning nightly anywhere is a repo where it does.
///
/// **No pin means not nightly.** An unpinned repo builds on whatever default
/// the machine has, which is a fact about that machine rather than about the
/// repo, so nothing here can claim it.
fn is_pinned_to_nightly(dir: &Path) -> bool {
    [dir.join("rust-toolchain.toml"), dir.join("mock").join("rust-toolchain.toml")]
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .any(|text| names_a_nightly_channel(&text))
}

/// Whether a `rust-toolchain.toml` body names a nightly channel.
///
/// Deliberately a line scan rather than a parse: the file has one key that
/// matters and pulling a toml parser in for it would be the larger change. A
/// commented-out channel is skipped so a repo that pinned nightly, thought
/// better of it, and left the old line behind is not read as pinned.
fn names_a_nightly_channel(text: &str) -> bool {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.strip_prefix("channel"))
        .filter_map(|rest| rest.trim_start().strip_prefix('='))
        .any(|v| v.trim().trim_matches(['"', '\'']).starts_with("nightly"))
}

/// One canonical config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    /// The name it deploys as, which is also the name it is stored under. No
    /// mapping, so there is nothing to drift.
    pub file_name: String,
    /// Which repos want it.
    pub applies:   Applies,
    /// The bytes.
    pub body:      Vec<u8>,
}

/// Reading `.shared/configs/` failed.
#[derive(Debug)]
pub enum TemplateError {
    /// The directory is not there.
    Missing(PathBuf),
    /// It is there and could not be read.
    Io(PathBuf, std::io::Error),
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(p) => {
                write!(
                    f,
                    "no shared configs at {}; nothing to compare a repo against",
                    p.display()
                )
            },
            Self::Io(p, e) => write!(f, "reading {}: {e}", p.display()),
        }
    }
}

impl std::error::Error for TemplateError {}

/// Every canonical config under `dir`, which is a workspace root.
///
/// `README.md` is documentation for the directory rather than a config and is
/// skipped by name. Anything else there is a template, including one whose
/// applicability is [`Applies::Unknown`]: it is loaded so the stage can report
/// it rather than pass over it silently.
pub fn templates(dir: &Path) -> Result<Vec<Template>, TemplateError> {
    let at = dir.join(CONFIGS_DIR);
    if !at.is_dir() {
        return Err(TemplateError::Missing(at));
    }
    let entries = std::fs::read_dir(&at).map_err(|e| TemplateError::Io(at.clone(), e))?;
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| TemplateError::Io(at.clone(), e))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name == "README.md" {
            continue;
        }
        let body = std::fs::read(&path).map_err(|e| TemplateError::Io(path.clone(), e))?;
        out.push(Template {
            file_name: name.to_string(),
            applies: Applies::of(name),
            body,
        });
    }
    out.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    Ok(out)
}

/// What the stage found for one config in one repo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// It was missing and now is not.
    Placed(String),
    /// It is there and is the canonical copy.
    Matches(String),
    /// It is there and is something else. **Left alone**, because the
    /// difference may be meant and this cannot tell.
    Differs(String),
    /// It is missing and where it belongs is not known, so a human places it.
    CannotInfer(String),
    /// The repo wants a config of this name and the shared copy does not fit
    /// it. The variant that would fit has not been written, so a human writes
    /// it. Carries what does not fit.
    NoVariantFits(String, String),
    /// It is missing and placing it did not work.
    Failed(String, String),
}

impl Finding {
    /// The config this is about.
    pub fn file_name(&self) -> &str {
        match self {
            Self::Placed(n)
            | Self::Matches(n)
            | Self::Differs(n)
            | Self::CannotInfer(n)
            | Self::NoVariantFits(n, _)
            | Self::Failed(n, _) => n,
        }
    }

    /// Whether this is something an operator has to act on.
    ///
    /// [`Finding::Differs`] is **not** one: a warning is not an error, and a
    /// tool that refuses to run over a workspace whose configs are fine is a
    /// tool somebody switches off.
    pub fn needs_a_human(&self) -> bool {
        matches!(
            self,
            Self::CannotInfer(_) | Self::NoVariantFits(..) | Self::Failed(..)
        )
    }
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Placed(n) => write!(f, "placed {n}"),
            Self::Matches(n) => write!(f, "{n} matches"),
            Self::Differs(n) => write!(f, "{n} differs from the shared copy, left as it is"),
            Self::CannotInfer(n) => {
                write!(f, "{n} is not there and nothing says which repos want it")
            },
            Self::NoVariantFits(n, why) => {
                write!(
                    f,
                    "{n} is not there and the shared copy does not fit: {why}"
                )
            },
            Self::Failed(n, e) => write!(f, "could not place {n}: {e}"),
        }
    }
}

/// Compare one repo against the canonical configs, placing what is missing.
///
/// `repo_dir` is contained under `root` already, which is what makes a
/// placement a contained write rather than a bare `std::fs` one.
pub fn ensure(root: &Root, repo_dir: &ContainedPath, templates: &[Template]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for t in templates {
        if t.applies == Applies::Unknown {
            // Reported once per repo rather than once per workspace, because a
            // template with no home is a question about every repo equally and
            // the operator is looking at a per-repo table.
            if !repo_dir.as_path().join(&t.file_name).exists() {
                findings.push(Finding::CannotInfer(t.file_name.clone()));
            }
            continue;
        }
        if !t.applies.wants(repo_dir.as_path()) {
            // A Rust repo on stable is not a repo this template misses; it is a
            // repo the template cannot serve. Say so once, rather than leaving
            // the only Rust repo without a formatting config and no sign of it.
            if t.applies == Applies::NightlyRustRepo
                && is_a_rust_repo(repo_dir.as_path())
                && !repo_dir.as_path().join(&t.file_name).exists()
            {
                findings.push(Finding::NoVariantFits(
                    t.file_name.clone(),
                    "it sets nightly-only options and this repo pins no nightly toolchain".into(),
                ));
            }
            continue;
        }
        let at = repo_dir.as_path().join(&t.file_name);
        match std::fs::read(&at) {
            Ok(have) if have == t.body => findings.push(Finding::Matches(t.file_name.clone())),
            Ok(_) => findings.push(Finding::Differs(t.file_name.clone())),
            Err(_) => {
                let placed = root
                    .contain_under(repo_dir, &t.file_name)
                    .map_err(|e| e.to_string())
                    .and_then(|p| root.write(&p, &t.body).map_err(|e| e.to_string()));
                findings.push(match placed {
                    Ok(()) => Finding::Placed(t.file_name.clone()),
                    Err(e) => Finding::Failed(t.file_name.clone(), e),
                });
            },
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A workspace with the canonical configs in it and a repo directory to
    /// compare, returning both plus the `Root` a placement writes through.
    struct Fixture {
        _dir:  tempfile::TempDir,
        _home: tempfile::TempDir,
        root:  Root,
        ws:    PathBuf,
    }

    impl Fixture {
        fn new(configs: &[(&str, &str)]) -> Self {
            let dir = tempfile::tempdir().unwrap();
            // Resolved, because a temp dir on macOS is under `/var`, which is a
            // symlink to `/private/var`, and an unresolved root compares equal
            // to nothing beneath it.
            let ws = dir.path().canonicalize().unwrap();
            std::fs::create_dir_all(ws.join(CONFIGS_DIR)).unwrap();
            for (name, body) in configs {
                std::fs::write(ws.join(CONFIGS_DIR).join(name), body).unwrap();
            }
            // A real deny list, built from a home that is somewhere else, so
            // the test runs the code path that ships rather than a permissive
            // variant of it. The workspace is not under this home, so nothing
            // here is denied and the placements are the ones being tested.
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

        /// Pin an already-made repo's toolchain. `at` is `""` for the root and
        /// `"mock"` for a repo whose Cargo workspace lives under `mock/`.
        fn pin(&self, name: &str, at: &str, body: &str) {
            let dir = self.ws.join(name).join(at);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("rust-toolchain.toml"), body).unwrap();
        }

        fn templates(&self) -> Vec<Template> {
            templates(&self.ws).unwrap()
        }
    }

    #[test]
    fn a_repo_missing_a_config_gets_it_and_the_bytes_are_the_templates() {
        // The control that matters: asserting `Placed` alone would pass on a
        // stage that reported placement and wrote nothing, which is the exact
        // shape of a fail-open. The file is read back.
        let f = Fixture::new(&[("deny.toml", "[bans]\nmultiple-versions = \"deny\"\n")]);
        let repo = f.repo("arvo", true);
        let found = ensure(&f.root, &repo, &f.templates());
        assert_eq!(found, vec![Finding::Placed("deny.toml".into())]);
        assert_eq!(
            std::fs::read_to_string(repo.as_path().join("deny.toml")).unwrap(),
            "[bans]\nmultiple-versions = \"deny\"\n"
        );
    }

    #[test]
    fn a_repo_whose_copy_matches_is_reported_and_not_rewritten() {
        let f = Fixture::new(&[("deny.toml", "[bans]\nmultiple-versions = \"deny\"\n")]);
        let repo = f.repo("arvo", true);
        std::fs::write(
            repo.as_path().join("deny.toml"),
            "[bans]\nmultiple-versions = \"deny\"\n",
        )
        .unwrap();
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
        // The whole asymmetry. A difference may be deliberate, so the stage
        // says so and changes nothing; a stage that "fixed" this would silently
        // undo somebody's exception.
        let f = Fixture::new(&[("deny.toml", "[bans]\nmultiple-versions = \"deny\"\n")]);
        let repo = f.repo("arvo", true);
        std::fs::write(
            repo.as_path().join("deny.toml"),
            "[bans]\nmultiple-versions = \"warn\"\n",
        )
        .unwrap();
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
    }

    #[test]
    fn a_repo_that_is_not_rust_does_not_get_the_rust_templates() {
        let f = Fixture::new(&[("deny.toml", "x\n"), (".taplo.toml", "y\n")]);
        let repo = f.repo("viola-grammar-ts", false);
        let found = ensure(&f.root, &repo, &f.templates());
        assert_eq!(found, vec![Finding::Placed(".taplo.toml".into())]);
        assert!(
            !repo.as_path().join("deny.toml").exists(),
            "a TypeScript repo was given a deny config"
        );
    }

    #[test]
    fn a_rust_repo_under_mock_is_recognised_too() {
        // The shape every repo here actually has: the cargo workspace is under
        // `mock/`, not at the root. A check for a root `Cargo.toml` alone would
        // decide that none of them are Rust.
        let f = Fixture::new(&[("deny.toml", "x\n")]);
        let repo = f.repo("homma", false);
        std::fs::create_dir_all(repo.as_path().join("mock")).unwrap();
        std::fs::write(
            repo.as_path().join("mock").join("Cargo.toml"),
            "[workspace]\n",
        )
        .unwrap();
        assert_eq!(ensure(&f.root, &repo, &f.templates()), vec![
            Finding::Placed("deny.toml".into())
        ]);
    }

    #[test]
    fn a_template_nobody_has_placed_is_reported_rather_than_spread_everywhere() {
        let f = Fixture::new(&[("mystery.toml", "?\n")]);
        let repo = f.repo("arvo", true);
        let found = ensure(&f.root, &repo, &f.templates());
        assert_eq!(found, vec![Finding::CannotInfer("mystery.toml".into())]);
        assert!(
            found[0].needs_a_human(),
            "an unplaceable config must reach somebody"
        );
        assert!(
            !repo.as_path().join("mystery.toml").exists(),
            "a template of unknown applicability was placed anyway"
        );
    }

    #[test]
    fn an_unknown_template_the_repo_already_has_is_not_reported() {
        // The control on the case above: the stage asks for a decision it does
        // not have, and once the repo has the file there is no decision left.
        let f = Fixture::new(&[("mystery.toml", "?\n")]);
        let repo = f.repo("arvo", true);
        std::fs::write(repo.as_path().join("mystery.toml"), "anything\n").unwrap();
        assert_eq!(ensure(&f.root, &repo, &f.templates()), vec![]);
    }

    #[test]
    fn the_readme_beside_the_templates_is_not_one() {
        let f = Fixture::new(&[("README.md", "# the configs\n"), ("deny.toml", "x\n")]);
        let names: Vec<_> = f.templates().into_iter().map(|t| t.file_name).collect();
        assert_eq!(names, vec!["deny.toml".to_string()]);
    }

    #[test]
    fn applicability_is_decided_by_name_and_the_list_is_the_one_a_reader_sees() {
        // rustfmt is the one that is not merely "a Rust repo": the shared copy
        // is nightly-only, so a stable Rust repo is not a place it goes.
        assert_eq!(Applies::of("rustfmt.toml"), Applies::NightlyRustRepo);
        assert_eq!(Applies::of("deny.toml"), Applies::RustRepo);
        assert_eq!(Applies::of("clippy.toml"), Applies::RustRepo);
        assert_eq!(Applies::of(".taplo.toml"), Applies::AnyRepo);
        assert_eq!(Applies::of("taplo.toml"), Applies::AnyRepo);
        // The default, and the reason the stage has a `CannotInfer` at all.
        assert_eq!(Applies::of("whatever.toml"), Applies::Unknown);
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
        let f =
            Fixture::new(&[("clippy.toml", "a\n"), ("deny.toml", "b\n"), (".taplo.toml", "c\n")]);
        let repo = f.repo("arvo", true);
        std::fs::write(repo.as_path().join("deny.toml"), "b\n").unwrap();
        std::fs::write(repo.as_path().join(".taplo.toml"), "different\n").unwrap();
        assert_eq!(ensure(&f.root, &repo, &f.templates()), vec![
            Finding::Differs(".taplo.toml".into()),
            Finding::Placed("clippy.toml".into()),
            Finding::Matches("deny.toml".into()),
        ]);
    }

    #[test]
    fn a_rust_repo_pinned_to_nightly_gets_the_nightly_only_config() {
        let f = Fixture::new(&[("rustfmt.toml", "wrap_comments = true\n")]);
        let repo = f.repo("arvo", true);
        f.pin(
            "arvo",
            "",
            "[toolchain]\nchannel = \"nightly-2026-05-28\"\n",
        );
        assert_eq!(ensure(&f.root, &repo, &f.templates()), vec![
            Finding::Placed("rustfmt.toml".into())
        ]);
    }

    #[test]
    fn a_rust_repo_on_stable_is_told_no_variant_fits_rather_than_given_the_wrong_one() {
        // The control on the test above, and the case that made this predicate
        // exist. renki pins nothing, so the shared rustfmt.toml would land in a
        // stable repo, print a warning per nightly-only option, and format to
        // the defaults regardless. Silence would leave the one Rust repo in the
        // workspace with no formatting config and no sign of it.
        let f = Fixture::new(&[("rustfmt.toml", "wrap_comments = true\n")]);
        let repo = f.repo("renki", true);
        let found = ensure(&f.root, &repo, &f.templates());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file_name(), "rustfmt.toml");
        assert!(matches!(found[0], Finding::NoVariantFits(..)), "{found:?}");
        assert!(found[0].needs_a_human());
        assert!(
            !repo.as_path().join("rustfmt.toml").exists(),
            "the nightly-only config was written into a stable repo anyway"
        );
    }

    #[test]
    fn a_repo_that_is_not_rust_at_all_is_not_told_about_the_rustfmt_variant() {
        // The other control: `NoVariantFits` is for a repo that wants the
        // config and cannot have this copy, never for one the template was
        // always going to skip.
        let f = Fixture::new(&[("rustfmt.toml", "wrap_comments = true\n")]);
        let repo = f.repo("viola-grammar-ts", false);
        assert_eq!(ensure(&f.root, &repo, &f.templates()), vec![]);
    }

    #[test]
    fn a_stable_repo_that_already_has_its_own_rustfmt_is_left_alone_and_not_reported() {
        let f = Fixture::new(&[("rustfmt.toml", "wrap_comments = true\n")]);
        let repo = f.repo("renki", true);
        std::fs::write(repo.as_path().join("rustfmt.toml"), "max_width = 100\n").unwrap();
        assert_eq!(ensure(&f.root, &repo, &f.templates()), vec![]);
        assert_eq!(
            std::fs::read_to_string(repo.as_path().join("rustfmt.toml")).unwrap(),
            "max_width = 100\n",
            "a repo's own config was overwritten"
        );
    }

    #[test]
    fn a_pin_under_mock_counts_the_same_as_one_at_the_root() {
        // mockspace pins in both places and homma pins in both; a repo whose
        // Cargo workspace is under `mock/` may pin only there.
        let f = Fixture::new(&[("rustfmt.toml", "wrap_comments = true\n")]);
        let repo = f.repo("hilavitkutin", false);
        std::fs::create_dir_all(repo.as_path().join("mock")).unwrap();
        std::fs::write(
            repo.as_path().join("mock").join("Cargo.toml"),
            "[workspace]\n",
        )
        .unwrap();
        assert_eq!(ensure(&f.root, &repo, &f.templates()), vec![
            Finding::NoVariantFits(
                "rustfmt.toml".into(),
                "it sets nightly-only options and this repo pins no nightly toolchain".into()
            )
        ]);
        f.pin(
            "hilavitkutin",
            "mock",
            "[toolchain]\nchannel = \"nightly\"\n",
        );
        assert_eq!(ensure(&f.root, &repo, &f.templates()), vec![
            Finding::Placed("rustfmt.toml".into())
        ]);
    }

    #[test]
    fn what_counts_as_a_nightly_channel_and_what_does_not() {
        for yes in [
            "[toolchain]\nchannel = \"nightly\"\n",
            "[toolchain]\nchannel = \"nightly-2026-05-28\"\n",
            "[toolchain]\nchannel=\"nightly\"\n",
            "[toolchain]\nchannel = 'nightly'\n",
            "[toolchain]\n  channel  =  \"nightly\"\ncomponents = [\"clippy\"]\n",
        ] {
            assert!(names_a_nightly_channel(yes), "{yes:?}");
        }
        for no in [
            "",
            "[toolchain]\nchannel = \"stable\"\n",
            "[toolchain]\nchannel = \"1.89.0\"\n",
            "[toolchain]\nchannel = \"beta\"\n",
            // a pin somebody thought better of, left behind as a comment
            "[toolchain]\n# channel = \"nightly\"\nchannel = \"stable\"\n",
            // the word appears and is not the channel
            "[toolchain]\nchannel = \"stable\"\n# was nightly until 2026\n",
            // near-misses on the key
            "[toolchain]\nchannels = \"nightly\"\n",
        ] {
            assert!(!names_a_nightly_channel(no), "{no:?}");
        }
    }

    #[test]
    fn the_shared_rustfmt_config_is_the_reason_this_predicate_exists() {
        // The hand check, kept and re-run rather than written down as a number.
        // The shared copy is not merely "a config we happen to use on
        // nightly": most of what it sets does not exist on stable at all, so a
        // stable repo given it formats to the defaults and warns once per
        // option while doing so. That is a property of the file, and the only
        // thing that knows which options are unstable is rustfmt itself.
        //
        // Skipped where a stable rustfmt is not installed. The predicate's
        // behaviour is covered by the cases above regardless; this one is about
        // the actual file, and a machine without stable cannot ask it.
        let at = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../..")
            .join(CONFIGS_DIR)
            .join("rustfmt.toml");
        let Ok(body) = std::fs::read(&at) else {
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
            "only {unstable} of the shared rustfmt options are nightly-only, out of {}; if that \
             is real, a stable variant is now writable and this predicate can go.\n{complaints}",
            String::from_utf8_lossy(&body)
                .lines()
                .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
                .count()
        );
    }
}
