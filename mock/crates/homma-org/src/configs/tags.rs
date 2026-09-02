//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Which repos a shared config belongs in, and how hard a rule it is.
//!
//! Both come off the directory the template sits in, so adding a config is
//! dropping a file into a directory rather than editing this crate. The
//! directory name is one or more tags joined with `+`, and a tag is an
//! ecosystem with an optional severity suffix.
//!
//! ```text
//! .shared/configs/rust_required/deny.toml
//! .shared/configs/rust_nightly_required/rustfmt.toml
//! .shared/configs/any_suggested+deno/some-editor-config
//! ```

use std::path::Path;

/// How hard a rule a template is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Reported, and nothing more. What a config ships as while it is reaching
    /// every repo that wants it.
    Suggested,
    /// A repo in the ecosystem and missing this is refused.
    Required,
}

impl Severity {
    /// The suffixes, longest first so a prefix of one cannot shadow another.
    ///
    /// Ordered rather than a match because [`Tag::parse`] strips a suffix
    /// before deciding what is left, and it needs the set to iterate.
    const SUFFIXES: [(&'static str, Self); 2] =
        [("_required", Self::Required), ("_suggested", Self::Suggested)];

    /// What a bare ecosystem name means.
    ///
    /// Required, because the ordinary case is a config every repo in the set
    /// must have, and the shorter spelling should be the ordinary case.
    pub const fn default_for_a_bare_name() -> Self {
        Self::Required
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Required => write!(f, "required"),
            Self::Suggested => write!(f, "suggested"),
        }
    }
}

/// A set of repos, identified by something on disk.
///
/// Closed, because each arm is a giveaway somebody worked out and wrote down.
/// A directory naming something not here is reported rather than guessed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ecosystem {
    /// Every repo, whatever it is written in.
    Any,
    /// `Cargo.toml` at the root or under `mock/`.
    Rust,
    /// A Rust repo whose toolchain is pinned to a nightly.
    ///
    /// Not a second axis on top of [`Ecosystem::Rust`], though it reads like
    /// one. The axis is which repos a config belongs in and every arm here is a
    /// name plus a giveaway, so a nightly pin is a giveaway like any other. It
    /// earns its place because a config can need one: a formatting config
    /// setting options that exist only on nightly does nothing in a repo pinned
    /// to stable except warn once per option and format to the defaults.
    RustNightly,
    /// `deno.json`, `deno.jsonc` or `deno.lock`.
    Deno,
    /// `package.json`.
    Node,
}

impl Ecosystem {
    /// Every ecosystem, so a test can enumerate them and a reader can see the
    /// set without following a match.
    pub const ALL: [Self; 5] = [Self::Any, Self::Rust, Self::RustNightly, Self::Deno, Self::Node];

    /// The name a tag directory spells it with.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Any => "any",
            Self::Rust => "rust",
            Self::RustNightly => "rust_nightly",
            Self::Deno => "deno",
            Self::Node => "node",
        }
    }

    /// Parse one, or `None` for a name nobody has defined.
    pub fn of(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.name() == name)
    }

    /// The wider set this one refines, where it refines one.
    ///
    /// Only [`Ecosystem::RustNightly`] does. It is what keeps the case
    /// expressible where a repo is in the wider set, is not in the narrower
    /// one, and has no config: that repo is not one the template passes over,
    /// it is one the template cannot serve, and saying nothing would leave it
    /// without a config and without a sign of it.
    pub const fn base(self) -> Option<Self> {
        match self {
            Self::RustNightly => Some(Self::Rust),
            _ => None,
        }
    }

    /// Whether the repo at `dir` is in this set.
    pub fn wants(self, dir: &Path) -> bool {
        match self {
            Self::Any => true,
            Self::Rust => is_a_rust_repo(dir),
            Self::RustNightly => is_a_rust_repo(dir) && is_pinned_to_nightly(dir),
            Self::Deno => {
                ["deno.json", "deno.jsonc", "deno.lock"]
                    .iter()
                    .any(|f| dir.join(f).is_file())
            },
            Self::Node => dir.join("package.json").is_file(),
        }
    }
}

/// One ecosystem, at one severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tag {
    pub ecosystem: Ecosystem,
    pub severity:  Severity,
}

/// A tag directory that could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownTag(pub String);

impl std::fmt::Display for UnknownTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "`{}` names no ecosystem", self.0)
    }
}

impl Tag {
    /// Parse one tag, `<ecosystem>` or `<ecosystem>_<severity>`.
    ///
    /// The severity suffix is stripped first and what is left has to be an
    /// ecosystem. That order is load-bearing rather than incidental: an
    /// ecosystem name may itself contain the separator, so reading up to the
    /// first `_` would split `rust_nightly_required` into `rust` and something
    /// unparseable, and silently hand a nightly-only config to every Rust repo.
    pub fn parse(text: &str) -> Result<Self, UnknownTag> {
        for (suffix, severity) in Severity::SUFFIXES {
            if let Some(head) = text.strip_suffix(suffix) {
                return match Ecosystem::of(head) {
                    Some(ecosystem) => {
                        Ok(Self {
                            ecosystem,
                            severity,
                        })
                    },
                    None => Err(UnknownTag(text.to_string())),
                };
            }
        }
        match Ecosystem::of(text) {
            Some(ecosystem) => {
                Ok(Self {
                    ecosystem,
                    severity: Severity::default_for_a_bare_name(),
                })
            },
            None => Err(UnknownTag(text.to_string())),
        }
    }

    /// Parse a whole directory name, several tags joined with `+`.
    ///
    /// One ecosystem twice is refused rather than resolved. Two severities for
    /// one set of repos is a contradiction, and picking either would be this
    /// deciding something the person who wrote the directory name did not.
    pub fn parse_dir(name: &str) -> Result<Vec<Self>, TagsError> {
        let mut out: Vec<Self> = Vec::new();
        for part in name.split('+') {
            let tag = Tag::parse(part).map_err(TagsError::Unknown)?;
            if out.iter().any(|t| t.ecosystem == tag.ecosystem) {
                return Err(TagsError::Repeated(tag.ecosystem));
            }
            out.push(tag);
        }
        Ok(out)
    }
}

/// A directory name that is not a usable tag list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagsError {
    /// A part named no ecosystem.
    Unknown(UnknownTag),
    /// One ecosystem appeared twice.
    Repeated(Ecosystem),
}

impl std::fmt::Display for TagsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown(u) => write!(f, "{u}"),
            Self::Repeated(e) => {
                write!(f, "`{}` is named twice, at two severities", e.name())
            },
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
/// No pin means not nightly. An unpinned repo builds on whatever default the
/// machine has, which is a fact about that machine rather than about the repo,
/// so nothing here can claim it.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (path, body) in files {
            let at = dir.path().join(path);
            std::fs::create_dir_all(at.parent().unwrap()).unwrap();
            std::fs::write(at, body).unwrap();
        }
        dir
    }

    #[test]
    fn a_bare_ecosystem_name_is_required() {
        for e in Ecosystem::ALL {
            let tag = Tag::parse(e.name()).unwrap();
            assert_eq!(tag.ecosystem, e);
            assert_eq!(
                tag.severity,
                Severity::Required,
                "`{}` alone did not mean required",
                e.name()
            );
        }
    }

    #[test]
    fn a_severity_suffix_is_honoured_for_every_ecosystem() {
        for e in Ecosystem::ALL {
            for (suffix, want) in Severity::SUFFIXES {
                let text = format!("{}{suffix}", e.name());
                let tag = Tag::parse(&text).unwrap();
                assert_eq!(tag.ecosystem, e, "{text} parsed to the wrong set");
                assert_eq!(tag.severity, want, "{text} parsed to the wrong severity");
            }
        }
    }

    #[test]
    fn an_ecosystem_name_containing_the_separator_still_parses() {
        // The case the parse order exists for. Reading up to the first `_`
        // would make this `rust`, which hands a nightly-only config to every
        // Rust repo including the ones on stable.
        let tag = Tag::parse("rust_nightly_required").unwrap();
        assert_eq!(tag.ecosystem, Ecosystem::RustNightly);
        assert_eq!(tag.severity, Severity::Required);

        // The two controls, so a pass cannot be a parser that ignores suffixes
        // or one that ignores the head.
        assert_eq!(Tag::parse("rust_nightly").unwrap(), Tag {
            ecosystem: Ecosystem::RustNightly,
            severity:  Severity::Required,
        });
        assert_eq!(Tag::parse("rust_required").unwrap(), Tag {
            ecosystem: Ecosystem::Rust,
            severity:  Severity::Required,
        });
        assert_eq!(
            Tag::parse("rust_nightly_suggested").unwrap().severity,
            Severity::Suggested
        );
    }

    #[test]
    fn a_name_that_is_not_an_ecosystem_is_refused_rather_than_guessed() {
        for bad in [
            "",
            "ruby",
            "rust_nightly_urgent",
            "_required",
            "rustx",
            "nightly",
            "any_",
            "RUST",
        ] {
            assert_eq!(
                Tag::parse(bad),
                Err(UnknownTag(bad.to_string())),
                "`{bad}` parsed as something"
            );
        }
    }

    #[test]
    fn a_directory_may_name_several_ecosystems() {
        let tags = Tag::parse_dir("rust_required+deno_suggested").unwrap();
        assert_eq!(tags, vec![
            Tag {
                ecosystem: Ecosystem::Rust,
                severity:  Severity::Required,
            },
            Tag {
                ecosystem: Ecosystem::Deno,
                severity:  Severity::Suggested,
            },
        ]);
    }

    #[test]
    fn one_ecosystem_named_twice_is_refused() {
        // Two severities for one set of repos is a contradiction, and picking
        // either would decide something the author of the name did not.
        assert_eq!(
            Tag::parse_dir("rust_required+rust_suggested"),
            Err(TagsError::Repeated(Ecosystem::Rust))
        );
        // Even spelled identically: a repeat is a mistake either way.
        assert_eq!(
            Tag::parse_dir("rust+rust"),
            Err(TagsError::Repeated(Ecosystem::Rust))
        );
        // The control: two different sets are fine.
        assert!(Tag::parse_dir("rust+deno").is_ok());
    }

    #[test]
    fn one_bad_part_fails_the_whole_directory() {
        assert_eq!(
            Tag::parse_dir("rust_required+ruby"),
            Err(TagsError::Unknown(UnknownTag("ruby".into())))
        );
    }

    #[test]
    fn any_wants_every_repo_including_an_empty_one() {
        let d = repo(&[]);
        assert!(Ecosystem::Any.wants(d.path()));
    }

    #[test]
    fn each_ecosystem_reads_its_own_giveaway_and_no_others() {
        // A table rather than one case each, so a giveaway that accidentally
        // satisfies a neighbouring set is caught. Every cell is asserted, not
        // only the diagonal.
        let cases: [(&str, Ecosystem); 5] = [
            ("Cargo.toml", Ecosystem::Rust),
            ("mock/Cargo.toml", Ecosystem::Rust),
            ("deno.json", Ecosystem::Deno),
            ("deno.lock", Ecosystem::Deno),
            ("package.json", Ecosystem::Node),
        ];
        for (file, expected) in cases {
            let d = repo(&[(file, "{}\n")]);
            for e in Ecosystem::ALL {
                let want = e == Ecosystem::Any || e == expected;
                assert_eq!(
                    e.wants(d.path()),
                    want,
                    "with only `{file}` present, `{}` answered wrongly",
                    e.name()
                );
            }
        }
    }

    #[test]
    fn deno_jsonc_counts_as_deno() {
        let d = repo(&[("deno.jsonc", "{}\n")]);
        assert!(Ecosystem::Deno.wants(d.path()));
    }

    #[test]
    fn rust_nightly_needs_both_a_cargo_manifest_and_a_nightly_pin() {
        let nightly = "[toolchain]\nchannel = \"nightly-2026-05-28\"\n";

        // Neither.
        let d = repo(&[]);
        assert!(!Ecosystem::RustNightly.wants(d.path()));
        // Cargo but no pin: the stable Rust repo.
        let d = repo(&[("Cargo.toml", "[package]\n")]);
        assert!(Ecosystem::Rust.wants(d.path()));
        assert!(!Ecosystem::RustNightly.wants(d.path()));
        // A pin but no Cargo manifest, which is not a Rust repo at all.
        let d = repo(&[("rust-toolchain.toml", nightly)]);
        assert!(!Ecosystem::Rust.wants(d.path()));
        assert!(!Ecosystem::RustNightly.wants(d.path()));
        // Both, at the root.
        let d = repo(&[("Cargo.toml", "[package]\n"), ("rust-toolchain.toml", nightly)]);
        assert!(Ecosystem::RustNightly.wants(d.path()));
        // Both, under mock/, which is the shape most repos here have.
        let d =
            repo(&[("mock/Cargo.toml", "[workspace]\n"), ("mock/rust-toolchain.toml", nightly)]);
        assert!(Ecosystem::RustNightly.wants(d.path()));
        // Manifest under mock/, pin at the root.
        let d = repo(&[("mock/Cargo.toml", "[workspace]\n"), ("rust-toolchain.toml", nightly)]);
        assert!(Ecosystem::RustNightly.wants(d.path()));
    }

    #[test]
    fn only_rust_nightly_refines_anything() {
        assert_eq!(Ecosystem::RustNightly.base(), Some(Ecosystem::Rust));
        for e in Ecosystem::ALL {
            if e != Ecosystem::RustNightly {
                assert_eq!(e.base(), None, "`{}` claimed a base", e.name());
            }
        }
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
    fn an_ecosystem_name_round_trips_through_of() {
        for e in Ecosystem::ALL {
            assert_eq!(Ecosystem::of(e.name()), Some(e));
        }
        assert_eq!(Ecosystem::of("nope"), None);
    }
}
