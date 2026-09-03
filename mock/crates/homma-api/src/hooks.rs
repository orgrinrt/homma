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

/// The events git hands input on stdin, per `githooks(5)`. A hook for any
/// other event reads none, since whatever wraps git may hand it an open pipe
/// and a read would wait on it.
pub const STDIN_EVENTS: &[&str] = &[
    "pre-push",
    "pre-receive",
    "post-receive",
    "post-rewrite",
    "reference-transaction",
    "proc-receive",
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
/// every invocation of the event. Built through [`HookEntry::new`] and
/// nowhere else, so an entry with a glob that does not parse, or nothing to
/// run, does not exist.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct HookEntry {
    run:      String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    paths:    Vec<String>,
    #[serde(skip)]
    patterns: Vec<glob::Pattern>,
}

impl HookEntry {
    /// An entry, or why it cannot be one: a glob that does not parse, or a
    /// command that is blank.
    pub fn new(run: impl Into<String>, paths: Vec<String>) -> Result<Self, InvalidHooks> {
        let run = run.into();
        if run.trim().is_empty() {
            return Err(InvalidHooks::Empty);
        }
        let mut patterns = Vec::with_capacity(paths.len());
        for p in &paths {
            patterns.push(glob::Pattern::new(p).map_err(|err| {
                InvalidHooks::Glob {
                    pattern: p.clone(),
                    reason:  err.msg.to_string(),
                }
            })?);
        }
        Ok(Self {
            run,
            paths,
            patterns,
        })
    }

    /// The command, with its placeholders still in it.
    pub fn run(&self) -> &str {
        &self.run
    }

    /// The globs as written.
    pub fn paths(&self) -> &[String] {
        &self.paths
    }

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
        touched
            .iter()
            .map(String::as_str)
            .filter(|path| self.patterns.iter().any(|g| g.matches_with(path, MATCH)))
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

/// The shape a manifest writes an entry in.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntry {
    run:   String,
    #[serde(default)]
    paths: Vec<String>,
}

impl<'de> serde::Deserialize<'de> for HookEntry {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = RawEntry::deserialize(d)?;
        Self::new(raw.run, raw.paths).map_err(serde::de::Error::custom)
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
        HookEntry::new(GATE, Vec::new()).expect("the gate is a command with no globs")
    }

    /// The declared entries under the default row, or the reason the
    /// declaration is refused: an event git has no hook for. The entries
    /// themselves were checked when they were made.
    pub fn new(declared: BTreeMap<String, Vec<HookEntry>>) -> Result<Self, InvalidHooks> {
        let mut map: BTreeMap<String, Vec<HookEntry>> = BTreeMap::new();
        map.insert(GATE_EVENT.to_string(), vec![Self::gate()]);
        for (event, entries) in declared {
            if !EVENTS.contains(&event.as_str()) {
                return Err(InvalidHooks::Event(event));
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

/// Why a `[hooks]` table, or one entry of it, is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidHooks {
    /// A key that is not the name of a git hook.
    Event(String),
    /// A glob that does not parse.
    Glob {
        pattern: String,
        reason:  String,
    },
    /// An entry with nothing to run.
    Empty,
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
                pattern,
                reason,
            } => {
                write!(
                    f,
                    "a hook entry has a glob that does not parse, `{pattern}`: {reason}"
                )
            },
            InvalidHooks::Empty => write!(f, "a hook entry has nothing to run"),
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

    fn entry(run: &str, paths: &[&str]) -> HookEntry {
        HookEntry::new(run, paths.iter().map(|s| s.to_string()).collect()).unwrap()
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
        let push: Vec<&str> = h.entries("pre-push").iter().map(|e| e.run()).collect();
        assert_eq!(
            push,
            vec![GATE, "a", "b"],
            "the gate first, then as written"
        );
        assert_eq!(h.entries("pre-commit")[0].paths(), &["*.md", "docs/*"]);
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
    fn a_glob_that_does_not_parse_and_an_empty_run_are_refused_at_the_entry() {
        // by the manifest, which names the row it happened in
        let err = parse("[[pre-commit]]\nrun = \"x\"\npaths = [\"[\"]\n").unwrap_err();
        assert!(err.contains("glob that does not parse"), "{err}");
        assert!(err.contains("`[`"), "{err}");
        assert!(err.contains("pre-commit"), "the row is named: {err}");
        let err = parse("[[pre-commit]]\nrun = \"  \"\n").unwrap_err();
        assert!(err.contains("nothing to run"), "{err}");
        let err = parse("[[pre-commit]]\nrun = \"x\"\nwhen = \"always\"\n").unwrap_err();
        assert!(
            err.contains("unknown field"),
            "an entry takes run and paths only: {err}"
        );
        // and by the constructor, so no entry exists that would match nothing
        assert!(matches!(
            HookEntry::new("x", vec!["[".into()]),
            Err(InvalidHooks::Glob { .. })
        ));
        assert_eq!(HookEntry::new(" ", Vec::new()), Err(InvalidHooks::Empty));
    }

    #[test]
    fn an_entry_matches_what_the_event_touched_and_expands_the_paths() {
        let touched: Vec<String> = ["README.md", "src/lib.rs", "docs/a.md", "it's.md"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let md = entry("vibecheck check {paths}", &["*.md"]);
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
        let rs = entry("cargo fmt --check", &["*.toml"]);
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
        let msg = entry("lint-message {args}", &[]);
        assert_eq!(
            msg.command(&[], &["it's.txt".to_string()]),
            "lint-message 'it'\\''s.txt'"
        );
        assert_eq!(
            msg.command(&[], &git_args),
            "lint-message 'origin' 'git@x:y.git'"
        );
        // a glob with a directory component holds the component
        let deep = entry("x", &["docs/*"]);
        assert_eq!(deep.matching(&touched), vec!["docs/a.md"]);
    }

    #[test]
    fn the_events_git_hands_stdin_to_are_the_documented_ones() {
        for e in STDIN_EVENTS {
            assert!(EVENTS.contains(e), "{e} is an event at all");
        }
        assert!(STDIN_EVENTS.contains(&"pre-push"));
        assert!(!STDIN_EVENTS.contains(&"pre-commit"));
        assert!(!STDIN_EVENTS.contains(&"commit-msg"));
    }
}
