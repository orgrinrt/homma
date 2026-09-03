//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The hooks table: one array of entries per git event, the release gate the
//! row every workspace has without writing it, and what an entry matches and
//! runs. No I/O; `homma-core` writes the entrypoints and runs the entries.

use std::collections::BTreeMap;
use std::fmt;

/// The events git runs a hook for, by the names git gives them. A table
/// naming anything else is refused when it loads.
pub const EVENTS: &[&str] = &[
    "applypatch-msg",
    "pre-applypatch",
    "post-applypatch",
    "pre-commit",
    "pre-merge-commit",
    "prepare-commit-msg",
    "commit-msg",
    "post-commit",
    "pre-rebase",
    "post-checkout",
    "post-merge",
    "pre-push",
    "pre-receive",
    "update",
    "proc-receive",
    "post-receive",
    "post-update",
    "reference-transaction",
    "push-to-checkout",
    "pre-auto-gc",
    "post-rewrite",
    "sendemail-validate",
    "fsmonitor-watchman",
    "p4-changelist",
    "p4-prepare-changelist",
    "p4-post-changelist",
    "p4-pre-submit",
    "post-index-change",
];

/// The command the release gate runs under, the one row every workspace has.
pub const GATE: &str = "homma release gate --hook";

/// The event the gate runs on.
pub const GATE_EVENT: &str = "pre-push";

/// The placeholder in a `run` that expands to the paths that made it run.
pub const PATHS: &str = "{paths}";

/// The placeholder in a `run` that expands to what git handed the hook after
/// its own name: the message file on `commit-msg`, the remote's name and url
/// on `pre-push`. An entry that does not name it gets none of them, since a
/// checker that takes files would otherwise be handed a remote's url as one.
pub const ARGS: &str = "{args}";

/// One row of the table: what to run, and for which paths. No `paths` means
/// every invocation of the event.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HookEntry {
    pub run:   String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
}

impl HookEntry {
    /// Whether this entry runs whatever the event touched.
    pub fn always(&self) -> bool {
        self.paths.is_empty()
    }

    /// The touched paths the entry's globs match, in the order touched. An
    /// entry with no globs matches every touched path, which is what
    /// `{paths}` expands to for it.
    pub fn matching<'a>(&self, touched: &'a [String]) -> Vec<&'a str> {
        if self.always() {
            return touched.iter().map(String::as_str).collect();
        }
        let patterns: Vec<glob::Pattern> = self
            .paths
            .iter()
            .filter_map(|p| glob::Pattern::new(p).ok())
            .collect();
        touched
            .iter()
            .map(String::as_str)
            .filter(|path| patterns.iter().any(|g| g.matches_with(path, MATCH)))
            .collect()
    }

    /// Whether the entry runs for what the event touched.
    pub fn runs_for(&self, touched: &[String]) -> bool {
        self.always() || !self.matching(touched).is_empty()
    }

    /// The command line, with `{paths}` expanded to the matched paths and
    /// `{args}` to git's arguments, each quoted for the shell, either to
    /// nothing where there are none.
    pub fn command(&self, matched: &[&str], args: &[String]) -> String {
        let paths: Vec<String> = matched.iter().map(|p| quote(p)).collect();
        let args: Vec<String> = args.iter().map(|a| quote(a)).collect();
        self.run
            .replace(PATHS, &paths.join(" "))
            .replace(ARGS, &args.join(" "))
    }
}

/// `*` and `?` cross a `/`, so `*.md` names every markdown file however deep,
/// which is what a path filter written by hand means; `**/*.md` says the same
/// thing longer and matches the same set.
const MATCH: glob::MatchOptions = glob::MatchOptions {
    case_sensitive:              true,
    require_literal_separator:   false,
    require_literal_leading_dot: false,
};

/// Single quotes for the shell, with an embedded single quote spelled the one
/// way the shell accepts it.
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// The table: entries per event, the gate first on its event, then what the
/// manifest declared in the order written. `[hooks]` in `homma.toml`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Hooks(BTreeMap<String, Vec<HookEntry>>);

impl Hooks {
    /// The row every workspace has: the release gate on `pre-push`.
    pub fn gate() -> HookEntry {
        HookEntry {
            run:   GATE.to_string(),
            paths: Vec::new(),
        }
    }

    /// The declared entries under the default row, or the reason the
    /// declaration is refused: an event git has no hook for, or a glob that
    /// does not parse.
    pub fn new(declared: BTreeMap<String, Vec<HookEntry>>) -> Result<Self, InvalidHooks> {
        let mut map: BTreeMap<String, Vec<HookEntry>> = BTreeMap::new();
        map.insert(GATE_EVENT.to_string(), vec![Self::gate()]);
        for (event, entries) in declared {
            if !EVENTS.contains(&event.as_str()) {
                return Err(InvalidHooks::Event(event));
            }
            for e in &entries {
                for p in &e.paths {
                    if let Err(err) = glob::Pattern::new(p) {
                        return Err(InvalidHooks::Glob {
                            event:   event.clone(),
                            pattern: p.clone(),
                            reason:  err.msg.to_string(),
                        });
                    }
                }
                if e.run.trim().is_empty() {
                    return Err(InvalidHooks::Empty(event));
                }
            }
            // the gate written out, as a serialised table carries it, is the
            // row already in front and not a second one
            let gate = Self::gate();
            let on_gate_event = event == GATE_EVENT;
            let extra: Vec<HookEntry> = entries
                .into_iter()
                .filter(|e| !(on_gate_event && *e == gate))
                .collect();
            map.entry(event).or_default().extend(extra);
        }
        Ok(Self(map))
    }

    /// The default rows alone, for a caller with no manifest to read one from.
    pub fn defaults() -> &'static Hooks {
        static DEFAULTS: std::sync::LazyLock<Hooks> = std::sync::LazyLock::new(Hooks::default);
        &DEFAULTS
    }

    /// The events with at least one entry, in name order.
    pub fn events(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    /// The entries for one event, in the order they run; none for an event
    /// the table does not name.
    pub fn entries(&self, event: &str) -> &[HookEntry] {
        self.0.get(event).map(Vec::as_slice).unwrap_or(&[])
    }
}

impl Default for Hooks {
    fn default() -> Self {
        Self::new(BTreeMap::new()).expect("the default table names only the gate")
    }
}

impl<'de> serde::Deserialize<'de> for Hooks {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let declared = BTreeMap::<String, Vec<HookEntry>>::deserialize(d)?;
        Self::new(declared).map_err(serde::de::Error::custom)
    }
}

/// Why a `[hooks]` table is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidHooks {
    /// A key that is not the name of a git hook.
    Event(String),
    /// A glob under an event that does not parse.
    Glob {
        event:   String,
        pattern: String,
        reason:  String,
    },
    /// An entry with nothing to run.
    Empty(String),
}

impl fmt::Display for InvalidHooks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InvalidHooks::Event(e) => {
                write!(
                    f,
                    "`[hooks.{e}]` names no git hook; the events are {}",
                    EVENTS.join(", ")
                )
            },
            InvalidHooks::Glob {
                event,
                pattern,
                reason,
            } => {
                write!(
                    f,
                    "`[hooks.{event}]` has a glob that does not parse, `{pattern}`: {reason}"
                )
            },
            InvalidHooks::Empty(e) => write!(f, "`[hooks.{e}]` has an entry with nothing to run"),
        }
    }
}

impl std::error::Error for InvalidHooks {}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Hooks, String> {
        toml::from_str::<Hooks>(text).map_err(|e| e.to_string())
    }

    #[test]
    fn an_empty_table_is_the_gate_on_pre_push_and_nothing_else() {
        let h = Hooks::default();
        assert_eq!(h.events().collect::<Vec<_>>(), vec!["pre-push"]);
        assert_eq!(h.entries("pre-push"), &[Hooks::gate()]);
        assert!(h.entries("pre-commit").is_empty());
        assert_eq!(Hooks::defaults(), &h);
        assert_eq!(
            parse("").unwrap(),
            h,
            "a manifest writing no table has the default"
        );
    }

    #[test]
    fn declared_entries_follow_the_gate_in_the_order_written() {
        let h = parse(
            "[[pre-push]]\nrun = \"a\"\n[[pre-push]]\nrun = \"b\"\npaths = [\"*.md\"]\n\
             [[pre-commit]]\nrun = \"c {paths}\"\npaths = [\"*.md\", \"docs/*\"]\n",
        )
        .unwrap();
        assert_eq!(h.events().collect::<Vec<_>>(), vec![
            "pre-commit",
            "pre-push"
        ]);
        let push: Vec<&str> = h
            .entries("pre-push")
            .iter()
            .map(|e| e.run.as_str())
            .collect();
        assert_eq!(
            push,
            vec![GATE, "a", "b"],
            "the gate first, then as written"
        );
        assert_eq!(h.entries("pre-commit")[0].paths, vec!["*.md", "docs/*"]);
        // and it round-trips through serialisation with the gate in place
        let text = toml::to_string(&h).unwrap();
        assert_eq!(parse(&text).unwrap(), h);
    }

    #[test]
    fn the_gate_cannot_be_dropped_by_writing_a_table_without_it() {
        let h = parse("[[pre-commit]]\nrun = \"x\"\n").unwrap();
        assert_eq!(h.entries("pre-push"), &[Hooks::gate()]);
    }

    #[test]
    fn an_event_git_has_no_hook_for_is_refused_naming_the_row() {
        let err = parse("[[pre-comit]]\nrun = \"x\"\n").unwrap_err();
        assert!(
            err.contains("`[hooks.pre-comit]` names no git hook"),
            "{err}"
        );
        assert!(
            err.contains("pre-commit"),
            "the message lists the events: {err}"
        );
        assert_eq!(
            Hooks::new(
                [("push".to_string(), vec![Hooks::gate()])]
                    .into_iter()
                    .collect()
            ),
            Err(InvalidHooks::Event("push".into()))
        );
    }

    #[test]
    fn a_glob_that_does_not_parse_and_an_empty_run_are_refused() {
        let err = parse("[[pre-commit]]\nrun = \"x\"\npaths = [\"[\"]\n").unwrap_err();
        assert!(err.contains("glob that does not parse"), "{err}");
        assert!(err.contains("`[`"), "{err}");
        let err = parse("[[pre-commit]]\nrun = \"  \"\n").unwrap_err();
        assert!(err.contains("nothing to run"), "{err}");
        let err = parse("[[pre-commit]]\nrun = \"x\"\nwhen = \"always\"\n").unwrap_err();
        assert!(
            err.contains("unknown field"),
            "an entry takes run and paths only: {err}"
        );
    }

    #[test]
    fn an_entry_matches_what_the_event_touched_and_expands_the_paths() {
        let touched: Vec<String> = ["README.md", "src/lib.rs", "docs/a.md", "it's.md"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let md = HookEntry {
            run:   "vibecheck check {paths}".into(),
            paths: vec!["*.md".into()],
        };
        assert!(md.runs_for(&touched));
        assert_eq!(md.matching(&touched), vec![
            "README.md",
            "docs/a.md",
            "it's.md"
        ]);
        let git_args: Vec<String> = vec!["origin".into(), "git@x:y.git".into()];
        assert_eq!(
            md.command(&md.matching(&touched), &git_args),
            "vibecheck check 'README.md' 'docs/a.md' 'it'\\''s.md'",
            "git's arguments reach an entry only through their placeholder"
        );
        let rs = HookEntry {
            run:   "cargo fmt --check".into(),
            paths: vec!["*.toml".into()],
        };
        assert!(!rs.runs_for(&touched), "no toml was touched");
        assert_eq!(
            rs.command(&[], &git_args),
            "cargo fmt --check",
            "no placeholder, nothing expanded"
        );
        // an entry without paths runs on nothing touched at all
        let gate = Hooks::gate();
        assert!(gate.runs_for(&[]));
        assert_eq!(
            gate.matching(&touched).len(),
            4,
            "everything, for its placeholder"
        );
        // a placeholder with nothing matched expands to nothing
        assert_eq!(md.command(&[], &[]), "vibecheck check ");
        // and git's arguments, quoted, where an entry asks for them
        let msg = HookEntry {
            run:   "lint-message {args}".into(),
            paths: Vec::new(),
        };
        assert_eq!(
            msg.command(&[], &["it's.txt".to_string()]),
            "lint-message 'it'\\''s.txt'"
        );
        assert_eq!(
            msg.command(&[], &git_args),
            "lint-message 'origin' 'git@x:y.git'"
        );
        // a glob with a directory component holds the component
        let deep = HookEntry {
            run:   "x".into(),
            paths: vec!["docs/*".into()],
        };
        assert_eq!(deep.matching(&touched), vec!["docs/a.md"]);
    }
}
