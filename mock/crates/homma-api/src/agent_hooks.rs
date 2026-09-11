//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The agent hooks table: what a workspace declares for the agent host, one
//! array per host event, under `[agent.hooks]` in `homma.toml`. No I/O;
//! `homma-engine` writes a wrapper per row and registers it.
//!
//! The git hooks table in [`crate::hooks`] is the other kind of hook and
//! shares nothing with this one but the manifest.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Component, Path};

/// The events the agent host runs a hook for, by the names it gives them. A
/// table naming anything else is refused when it loads.
pub const AGENT_EVENTS: &[&str] = &[
    "SessionStart",
    "Setup",
    "UserPromptSubmit",
    "UserPromptExpansion",
    "PreToolUse",
    "PermissionRequest",
    "PermissionDenied",
    "PostToolUse",
    "PostToolUseFailure",
    "PostToolBatch",
    "Notification",
    "MessageDisplay",
    "SubagentStart",
    "SubagentStop",
    "TaskCreated",
    "TaskCompleted",
    "Stop",
    "StopFailure",
    "TeammateIdle",
    "InstructionsLoaded",
    "ConfigChange",
    "CwdChanged",
    "DirectoryAdded",
    "FileChanged",
    "WorktreeCreate",
    "WorktreeRemove",
    "PreCompact",
    "PostCompact",
    "PreModelSwitch",
    "PostModelSwitch",
    "Elicitation",
    "ElicitationResult",
    "SessionEnd",
];

/// The matcher a row gets when it names none, which the host reads as every
/// tool, and ignores on an event that has no tools to match.
pub const EVERY_TOOL: &str = "*";

/// One row: which tools it fires for, the script it runs, and the
/// repositories it is narrowed to. Built through [`AgentHook::new`] and
/// nowhere else, so a row running nothing, or running something outside the
/// workspace, does not exist.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AgentHook {
    matcher: String,
    run:     String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    repos:   Vec<String>,
}

impl AgentHook {
    /// A row, or why it cannot be one.
    ///
    /// `run` is a path under the workspace and not a command line: the host
    /// hands a hook its input on stdin, so a hook wanting arguments ships a
    /// script that supplies them, and a path is all a row needs to say. An
    /// absolute path, one climbing out with `..`, or one carrying whitespace is
    /// refused rather than read as something it might have meant.
    pub fn new(
        matcher: impl Into<String>,
        run: impl Into<String>,
        repos: Vec<String>,
    ) -> Result<Self, InvalidAgentHooks> {
        let run = run.into();
        if run.trim().is_empty() {
            return Err(InvalidAgentHooks::Empty);
        }
        if run.chars().any(char::is_whitespace) {
            return Err(InvalidAgentHooks::NotAPath(run));
        }
        let p = Path::new(&run);
        if p.is_absolute() || p.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(InvalidAgentHooks::Escapes(run));
        }
        for r in &repos {
            if r.trim().is_empty() || r.contains('/') || r.chars().any(char::is_whitespace) {
                return Err(InvalidAgentHooks::Repo(r.clone()));
            }
        }
        let matcher = matcher.into();
        let matcher = if matcher.trim().is_empty() { EVERY_TOOL.to_string() } else { matcher };
        Ok(Self {
            matcher,
            run,
            repos,
        })
    }

    /// Which tools the row fires for.
    pub fn matcher(&self) -> &str {
        &self.matcher
    }

    /// The script, as a path under the workspace.
    pub fn run(&self) -> &str {
        &self.run
    }

    /// The repositories the row is narrowed to, by name; none means every call.
    pub fn repos(&self) -> &[String] {
        &self.repos
    }
}

/// A row as written, before it is checked.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    #[serde(default)]
    matcher: String,
    run:     String,
    #[serde(default)]
    repos:   Vec<String>,
}

impl<'de> serde::Deserialize<'de> for AgentHook {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let row = Row::deserialize(d)?;
        Self::new(row.matcher, row.run, row.repos).map_err(serde::de::Error::custom)
    }
}

/// The rows per event, in the order written. `[agent.hooks]` in `homma.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct AgentHooks(BTreeMap<String, Vec<AgentHook>>);

impl AgentHooks {
    /// The declared rows, or the reason they are refused: an event the host
    /// has no hook for. The rows themselves were checked when they were made.
    pub fn new(declared: BTreeMap<String, Vec<AgentHook>>) -> Result<Self, InvalidAgentHooks> {
        for event in declared.keys() {
            if !AGENT_EVENTS.contains(&event.as_str()) {
                return Err(InvalidAgentHooks::Event(event.clone()));
            }
        }
        Ok(Self(declared))
    }

    /// Every row with its event, events in name order and rows in the order
    /// written under each.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &AgentHook)> {
        self.0
            .iter()
            .flat_map(|(e, rows)| rows.iter().map(move |r| (e.as_str(), r)))
    }

    /// Whether the table declares nothing.
    pub fn is_empty(&self) -> bool {
        self.0.values().all(Vec::is_empty)
    }
}

impl<'de> serde::Deserialize<'de> for AgentHooks {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let declared = BTreeMap::<String, Vec<AgentHook>>::deserialize(d)?;
        Self::new(declared).map_err(serde::de::Error::custom)
    }
}

/// `[agent]` in `homma.toml`, which holds the hooks table and nothing else.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentTable {
    #[serde(default)]
    pub hooks: AgentHooks,
}

/// Why an `[agent.hooks]` table, or one row of it, is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidAgentHooks {
    /// A key that is not the name of a host event.
    Event(String),
    /// A row with nothing to run.
    Empty,
    /// A `run` carrying whitespace, which is a command line rather than a path.
    NotAPath(String),
    /// A `run` that is absolute or climbs out of the workspace.
    Escapes(String),
    /// A `repos` name that is blank or is a path rather than a name.
    Repo(String),
}

impl fmt::Display for InvalidAgentHooks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InvalidAgentHooks::Event(e) => {
                write!(
                    f,
                    "`[agent.hooks.{e}]` names no event the agent host has; the events are {}",
                    AGENT_EVENTS.join(", ")
                )
            },
            InvalidAgentHooks::Empty => write!(f, "an agent hook row has nothing to run"),
            InvalidAgentHooks::NotAPath(r) => {
                write!(
                    f,
                    "an agent hook row runs `{r}`, which is a command line; `run` is a path to a \
                     script under the workspace, and a hook wanting arguments ships one"
                )
            },
            InvalidAgentHooks::Escapes(r) => {
                write!(
                    f,
                    "an agent hook row runs `{r}`, which is not under the workspace; `run` is a \
                     relative path that stays inside it"
                )
            },
            InvalidAgentHooks::Repo(r) => {
                write!(
                    f,
                    "an agent hook row is narrowed to `{r}`, which is not a repository name"
                )
            },
        }
    }
}

impl std::error::Error for InvalidAgentHooks {}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<AgentTable, String> {
        toml::from_str::<AgentTable>(text).map_err(|e| e.to_string())
    }

    #[test]
    fn an_empty_table_declares_nothing() {
        let t = parse("").unwrap();
        assert!(t.hooks.is_empty());
        assert_eq!(t.hooks.iter().count(), 0);
    }

    #[test]
    fn a_row_carries_its_event_matcher_run_and_repos() {
        let t = parse(
            "[[hooks.PreToolUse]]\nmatcher = \"Bash\"\nrun = \"scripts/guard\"\nrepos = \
             [\"engine\"]\n",
        )
        .unwrap();
        let rows: Vec<_> = t.hooks.iter().collect();
        assert_eq!(rows.len(), 1);
        let (event, row) = rows[0];
        assert_eq!(event, "PreToolUse");
        assert_eq!(row.matcher(), "Bash");
        assert_eq!(row.run(), "scripts/guard");
        assert_eq!(row.repos(), &["engine".to_string()]);
    }

    #[test]
    fn a_row_naming_no_matcher_fires_for_every_tool() {
        let t = parse("[[hooks.Stop]]\nrun = \"scripts/at-the-end\"\n").unwrap();
        let (_, row) = t.hooks.iter().next().unwrap();
        assert_eq!(row.matcher(), EVERY_TOOL);
        // the control: a named matcher is kept as written
        let t = parse("[[hooks.Stop]]\nmatcher = \"Edit\"\nrun = \"x\"\n").unwrap();
        assert_eq!(t.hooks.iter().next().unwrap().1.matcher(), "Edit");
    }

    #[test]
    fn rows_keep_the_order_they_were_written_in_under_one_event() {
        let t = parse(
            "[[hooks.PreToolUse]]\nrun = \"b\"\n[[hooks.PreToolUse]]\nrun = \"a\"\n\
             [[hooks.PostToolUse]]\nrun = \"c\"\n",
        )
        .unwrap();
        let runs: Vec<(&str, &str)> = t.hooks.iter().map(|(e, r)| (e, r.run())).collect();
        assert_eq!(runs, vec![
            ("PostToolUse", "c"),
            ("PreToolUse", "b"),
            ("PreToolUse", "a")
        ]);
    }

    #[test]
    fn an_event_the_host_does_not_have_is_refused_and_named() {
        let err = parse("[[hooks.pre-commit]]\nrun = \"x\"\n").unwrap_err();
        assert!(err.contains("pre-commit"), "the event is named: {err}");
        assert!(err.contains("PreToolUse"), "the events are listed: {err}");
        // every event the host documents loads
        for e in AGENT_EVENTS {
            assert!(
                parse(&format!("[[hooks.{e}]]\nrun = \"x\"\n")).is_ok(),
                "{e}"
            );
        }
    }

    #[test]
    fn a_run_that_is_not_a_path_under_the_workspace_is_refused() {
        for (run, why) in [
            ("/usr/local/bin/guard", "not under the workspace"),
            ("../elsewhere/guard", "not under the workspace"),
            ("scripts/../../guard", "not under the workspace"),
            ("guard --strict", "command line"),
            ("", "nothing to run"),
            ("   ", "nothing to run"),
        ] {
            let err = parse(&format!("[[hooks.PreToolUse]]\nrun = \"{run}\"\n")).unwrap_err();
            assert!(
                err.contains(why),
                "`{run}` refused for the wrong reason: {err}"
            );
        }
        // the control: a relative path inside the workspace, however deep
        assert!(parse("[[hooks.PreToolUse]]\nrun = \".shared/scripts/x/guard\"\n").is_ok());
        assert!(parse("[[hooks.PreToolUse]]\nrun = \"./guard\"\n").is_ok());
    }

    #[test]
    fn a_repos_entry_that_is_not_a_name_is_refused() {
        for bad in ["", "a/b", "two words"] {
            let err = parse(&format!(
                "[[hooks.PreToolUse]]\nrun = \"x\"\nrepos = [\"{bad}\"]\n"
            ))
            .unwrap_err();
            assert!(err.contains("not a repository name"), "`{bad}`: {err}");
        }
        assert!(parse("[[hooks.PreToolUse]]\nrun = \"x\"\nrepos = [\"a-b_c\"]\n").is_ok());
    }

    #[test]
    fn a_field_the_row_does_not_have_is_refused() {
        let err = parse("[[hooks.PreToolUse]]\nrun = \"x\"\nwhen = \"always\"\n").unwrap_err();
        assert!(err.contains("when"), "{err}");
        // and so is one on the table above it
        let err = parse("rules = []\n").unwrap_err();
        assert!(err.contains("rules"), "{err}");
    }

    #[test]
    fn a_row_is_built_through_new_or_not_at_all() {
        assert_eq!(
            AgentHook::new("Bash", "", vec![]).unwrap_err(),
            InvalidAgentHooks::Empty
        );
        let row = AgentHook::new("", "x", vec![]).unwrap();
        assert_eq!(row.matcher(), EVERY_TOOL);
    }
}
