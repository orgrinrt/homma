//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The release vocabulary: a version and the level that moves it, what a gate
//! run is and what it measured, the finding a check produces, and the shape a
//! badge is served in. No I/O; `homma-core` does the measuring and the moving.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use crate::record::{Attr, AttrType, Kind, Mutability, Record};
use crate::reference::{Namespace, Reference};

/// How far a release moves the version. Never inferred from commits; the
/// level is given on every run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Patch,
    Minor,
    Major,
}

impl FromStr for Level {
    type Err = UnknownLevel;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "patch" => Ok(Level::Patch),
            "minor" => Ok(Level::Minor),
            "major" => Ok(Level::Major),
            other => Err(UnknownLevel(other.to_string())),
        }
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Level::Patch => "patch",
            Level::Minor => "minor",
            Level::Major => "major",
        })
    }
}

/// A level string that is none of the three.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownLevel(pub String);

impl fmt::Display for UnknownLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown level `{}`, expected patch, minor or major",
            self.0
        )
    }
}

impl std::error::Error for UnknownLevel {}

pub use crate::version::{NotAVersion, Version};
/// What a root marker file signals about the repository carrying it: which
/// toolchain the gate calls for it, or that there is none to call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Signal {
    Cargo,
    Deno,
    Content,
}

/// The marker files a workspace recognises at a repository root, each with
/// what it signals. `[markers]` in `homma.toml`; the two rows the binary knew
/// before the table existed are present whether or not the manifest writes
/// them, so a workspace declaring nothing keeps the behaviour it had, and a
/// manifest may override either by naming the same file.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Markers(std::collections::BTreeMap<String, Signal>);

impl Markers {
    pub const DEFAULT: [(&'static str, Signal); 2] =
        [("Cargo.toml", Signal::Cargo), ("deno.json", Signal::Deno)];

    /// The declared rows over the default ones.
    pub fn new(declared: impl IntoIterator<Item = (String, Signal)>) -> Self {
        let mut map: std::collections::BTreeMap<String, Signal> = Self::DEFAULT
            .iter()
            .map(|(f, s)| ((*f).to_string(), *s))
            .collect();
        map.extend(declared);
        Self(map)
    }

    /// The default rows alone, as a borrow that outlives any caller, for a
    /// setup that holds a reference and has no manifest to read one from.
    pub fn defaults() -> &'static Markers {
        static DEFAULTS: std::sync::LazyLock<Markers> = std::sync::LazyLock::new(Markers::default);
        &DEFAULTS
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, Signal)> {
        self.0.iter().map(|(f, s)| (f.as_str(), *s))
    }

    pub fn signal_of(&self, file: &str) -> Option<Signal> {
        self.0.get(file).copied()
    }
}

impl Default for Markers {
    fn default() -> Self {
        Self::new([])
    }
}

impl<'de> serde::Deserialize<'de> for Markers {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let declared = std::collections::BTreeMap::<String, Signal>::deserialize(d)?;
        Ok(Self::new(declared))
    }
}

/// What a repository is: the set of signals the marker files at its root
/// carry. `content` adds nothing to that set, so a root carrying `Cargo.toml`
/// beside a content marker is a crate, and only a root whose markers say
/// nothing but content is `Content`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepoKind {
    Crate,
    Deno,
    Both,
    Content,
}

impl RepoKind {
    /// The kind a set of signals adds up to, `None` for the empty set.
    pub fn from_signals(signals: impl IntoIterator<Item = Signal>) -> Option<Self> {
        let (mut cargo, mut deno, mut content) = (false, false, false);
        for s in signals {
            match s {
                Signal::Cargo => cargo = true,
                Signal::Deno => deno = true,
                Signal::Content => content = true,
            }
        }
        match (cargo, deno, content) {
            (true, true, _) => Some(RepoKind::Both),
            (true, false, _) => Some(RepoKind::Crate),
            (false, true, _) => Some(RepoKind::Deno),
            (false, false, true) => Some(RepoKind::Content),
            (false, false, false) => None,
        }
    }

    pub fn has_crate(self) -> bool {
        matches!(self, RepoKind::Crate | RepoKind::Both)
    }

    pub fn has_deno(self) -> bool {
        matches!(self, RepoKind::Deno | RepoKind::Both)
    }
}

#[cfg(test)]
mod summary_tests {
    use super::*;

    fn run(steps: Vec<StepOutcome>) -> GateRun {
        GateRun {
            repo: "x".into(),
            sha: "0".repeat(40),
            ran_at: "t".into(),
            verdict: GateRun::verdict_of(&steps),
            steps,
        }
    }

    fn ran(step: Step, passed: bool) -> StepOutcome {
        StepOutcome {
            step,
            passed,
            skipped: false,
            numbers: Default::default(),
            log: String::new(),
        }
    }

    #[test]
    fn every_step_skipped_says_nothing_ran_and_why() {
        let r = run(Step::ALL.iter().map(|s| StepOutcome::skipped(*s)).collect());
        assert_eq!(r.verdict, Verdict::Green);
        assert_eq!(
            r.summary(),
            "nothing to run: every step skipped, the repository's markers say only content"
        );
    }

    #[test]
    fn a_step_that_ran_keeps_the_summary_it_had_and_skipped_ones_stay_out() {
        let mut steps: Vec<StepOutcome> =
            Step::ALL.iter().map(|s| StepOutcome::skipped(*s)).collect();
        steps[5] = ran(Step::Notices, false);
        let r = run(steps);
        assert_eq!(r.verdict, Verdict::Red);
        assert_eq!(r.summary(), "notices failed");
        let r = run(vec![
            ran(Step::Format, true),
            StepOutcome::skipped(Step::Deny),
        ]);
        assert_eq!(r.summary(), "format");
        // the control: no steps at all is not "every step skipped"
        assert_eq!(run(vec![]).summary(), "");
    }
}

#[cfg(test)]
mod kind_tests {
    use super::*;

    #[test]
    fn the_defaults_are_always_present_and_a_declared_row_overrides_one() {
        let m = Markers::default();
        assert_eq!(m.signal_of("Cargo.toml"), Some(Signal::Cargo));
        assert_eq!(m.signal_of("deno.json"), Some(Signal::Deno));
        assert_eq!(m.signal_of("polka.toml"), None);
        let m = Markers::new([
            ("polka.toml".to_string(), Signal::Content),
            ("deno.json".to_string(), Signal::Content),
        ]);
        assert_eq!(m.signal_of("polka.toml"), Some(Signal::Content));
        assert_eq!(m.signal_of("deno.json"), Some(Signal::Content));
        assert_eq!(m.signal_of("Cargo.toml"), Some(Signal::Cargo));
        assert_eq!(m.iter().count(), 3);
    }

    #[test]
    fn a_table_deserialises_over_the_defaults_and_refuses_a_word_outside_the_three() {
        #[derive(Debug, serde::Deserialize)]
        struct Doc {
            markers: Markers,
        }
        let d: Doc = toml::from_str("[markers]\n\"polka.toml\" = \"content\"\n").unwrap();
        assert_eq!(d.markers.signal_of("polka.toml"), Some(Signal::Content));
        assert_eq!(d.markers.signal_of("Cargo.toml"), Some(Signal::Cargo));
        let err = toml::from_str::<Doc>("[markers]\n\"polka.toml\" = \"prose\"\n").unwrap_err();
        let text = err.to_string();
        assert!(text.contains("prose"), "{text}");
        assert!(text.contains("content"), "{text}");
        let d: Doc = toml::from_str("").unwrap_or_else(|_| {
            Doc {
                markers: Markers::default(),
            }
        });
        assert_eq!(d.markers, Markers::default());
    }

    #[test]
    fn signals_add_up_to_a_kind_and_content_adds_nothing() {
        use Signal::*;
        assert_eq!(RepoKind::from_signals([]), None);
        assert_eq!(RepoKind::from_signals([Content]), Some(RepoKind::Content));
        assert_eq!(
            RepoKind::from_signals([Content, Content]),
            Some(RepoKind::Content)
        );
        assert_eq!(RepoKind::from_signals([Cargo]), Some(RepoKind::Crate));
        assert_eq!(
            RepoKind::from_signals([Cargo, Content]),
            Some(RepoKind::Crate)
        );
        assert_eq!(
            RepoKind::from_signals([Deno, Content]),
            Some(RepoKind::Deno)
        );
        assert_eq!(RepoKind::from_signals([Deno, Cargo]), Some(RepoKind::Both));
        assert_eq!(
            RepoKind::from_signals([Content, Cargo, Deno]),
            Some(RepoKind::Both)
        );
        assert!(!RepoKind::Content.has_crate());
        assert!(!RepoKind::Content.has_deno());
        assert!(RepoKind::Both.has_crate() && RepoKind::Both.has_deno());
        assert!(RepoKind::Crate.has_crate() && !RepoKind::Crate.has_deno());
        assert!(!RepoKind::Deno.has_crate() && RepoKind::Deno.has_deno());
    }
}

/// The six gate steps, in the order they run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Format,
    Lint,
    Tests,
    Deny,
    Docs,
    Notices,
}

impl Step {
    pub const ALL: [Step; 6] =
        [Step::Format, Step::Lint, Step::Tests, Step::Deny, Step::Docs, Step::Notices];

    /// Whether a failure of this step turns the verdict red. Docs only
    /// reports its number.
    pub fn blocks(self) -> bool {
        !matches!(self, Step::Docs)
    }

    pub fn name(self) -> &'static str {
        match self {
            Step::Format => "format",
            Step::Lint => "lint",
            Step::Tests => "tests",
            Step::Deny => "deny",
            Step::Docs => "docs",
            Step::Notices => "notices",
        }
    }
}

/// What one step produced: whether it passed, whether it was skipped because
/// nothing asked for it, the numbers it measured, and everything it printed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StepOutcome {
    pub step:    Step,
    pub passed:  bool,
    #[serde(default)]
    pub skipped: bool,
    #[serde(default)]
    pub numbers: BTreeMap<String, String>,
    #[serde(default)]
    pub log:     String,
}

impl StepOutcome {
    pub fn skipped(step: Step) -> Self {
        Self {
            step,
            passed: true,
            skipped: true,
            numbers: BTreeMap::new(),
            log: String::new(),
        }
    }

    /// Whether this outcome, on its own, makes the run red.
    pub fn is_red(&self) -> bool {
        !self.passed && self.step.blocks()
    }
}

/// The verdict of a gate run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Green,
    Red,
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Verdict::Green => "green",
            Verdict::Red => "red",
        })
    }
}

/// One gate run against one commit of one repository.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GateRun {
    /// The repository's name in the workspace configuration.
    pub repo:    String,
    /// The full sha the run measured.
    pub sha:     String,
    /// RFC 3339.
    pub ran_at:  String,
    pub verdict: Verdict,
    pub steps:   Vec<StepOutcome>,
}

/// The name of the kind a gate run is stored under.
pub const GATE_RUN_KIND: &str = "gate-run";

/// The steps as they sit in the record's text attribute: toml, so a person
/// reading the store sees a table per step rather than one line of brackets.
#[derive(serde::Serialize, serde::Deserialize)]
struct StepsBlob {
    steps: Vec<StepOutcome>,
}

impl GateRun {
    /// The verdict the steps add up to: red on any blocking failure.
    pub fn verdict_of(steps: &[StepOutcome]) -> Verdict {
        if steps.iter().any(StepOutcome::is_red) {
            Verdict::Red
        } else {
            Verdict::Green
        }
    }

    /// The kind every run is stored under. Append-only, one text attribute
    /// per column and an instant for the time, the steps as json in text
    /// because the store has no number type and nothing sorts or sums these.
    pub fn kind() -> Kind {
        Kind::new(GATE_RUN_KIND, Mutability::AppendOnly)
            .with_attr("repo", AttrType::Text)
            .with_attr("sha", AttrType::Text)
            .with_attr("ran_at", AttrType::Instant)
            .with_attr("verdict", AttrType::Text)
            .with_attr("steps", AttrType::Text)
            .requiring("repo")
            .requiring("sha")
            .requiring("ran_at")
            .requiring("verdict")
            .requiring("steps")
    }

    /// The record this run is stored as. The id is the repo and the sha and
    /// the time, so two runs on one commit are two records.
    pub fn to_record(&self) -> Record {
        let provenance = Reference::new(Namespace::Forge, self.repo.clone());
        let steps = toml::to_string(&StepsBlob {
            steps: self.steps.clone(),
        })
        .expect("steps serialise");
        Record::new(
            format!("{}:{}:{}", self.repo, self.sha, self.ran_at),
            GATE_RUN_KIND,
            &provenance,
        )
        .with("repo", Attr::Text(self.repo.clone()))
        .with("sha", Attr::Text(self.sha.clone()))
        .with("ran_at", Attr::Instant(self.ran_at.clone()))
        .with("verdict", Attr::Text(self.verdict.to_string()))
        .with("steps", Attr::Text(steps))
    }

    /// The run a record holds, or why it does not.
    pub fn from_record(record: &Record) -> Result<GateRun, NotAGateRun> {
        record
            .check(&Self::kind())
            .map_err(|e| NotAGateRun(format!("{e:?}")))?;
        let text = |name: &str| {
            match record.attrs.get(name) {
                Some(Attr::Text(s)) | Some(Attr::Instant(s)) => Ok(s.clone()),
                _ => Err(NotAGateRun(format!("`{name}` missing"))),
            }
        };
        let verdict = match text("verdict")?.as_str() {
            "green" => Verdict::Green,
            "red" => Verdict::Red,
            other => return Err(NotAGateRun(format!("verdict `{other}`"))),
        };
        let steps: Vec<StepOutcome> = toml::from_str::<StepsBlob>(&text("steps")?)
            .map_err(|e| NotAGateRun(e.to_string()))?
            .steps;
        Ok(GateRun {
            repo: text("repo")?,
            sha: text("sha")?,
            ran_at: text("ran_at")?,
            verdict,
            steps,
        })
    }

    /// The numbers, in step order, the way the status description carries
    /// them: `tests 41/41, docs 97%, deny 0`.
    /// One line naming the steps that ran and how each went. A run in which
    /// every step was skipped, which is what a repository whose markers say
    /// only content produces, says so rather than saying nothing, so a green
    /// status on such a repository never reads as a check that happened.
    pub fn summary(&self) -> String {
        if !self.steps.is_empty() && self.steps.iter().all(|s| s.skipped) {
            return "nothing to run: every step skipped, the repository's markers say only content"
                .into();
        }
        let mut parts = Vec::new();
        for step in &self.steps {
            if step.skipped {
                continue;
            }
            let mut piece = step.step.name().to_string();
            if let Some(n) = step.numbers.get("summary") {
                piece.push(' ');
                piece.push_str(n);
            } else if !step.passed {
                piece.push_str(" failed");
            }
            parts.push(piece);
        }
        parts.join(", ")
    }
}

/// A record that is not a gate run, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotAGateRun(pub String);

impl fmt::Display for NotAGateRun {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not a gate run: {}", self.0)
    }
}

impl std::error::Error for NotAGateRun {}

/// How much a check finding weighs. `Warn` is reported, `Error` blocks a
/// release, `Fatal` stops the check itself.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum CheckSeverity {
    Warn,
    Error,
    Fatal,
}

impl CheckSeverity {
    pub fn blocks(self) -> bool {
        self >= CheckSeverity::Error
    }
}

/// One thing a check established was not so. The id is the catalogue's.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Finding {
    pub id:       String,
    pub severity: CheckSeverity,
    pub message:  String,
}

impl Finding {
    pub fn new(id: &str, severity: CheckSeverity, message: impl Into<String>) -> Self {
        Self {
            id: id.to_string(),
            severity,
            message: message.into(),
        }
    }
}

/// The shape shields' endpoint badge reads.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Badge {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u8,
    pub label:          String,
    pub message:        String,
    pub color:          String,
}

impl Badge {
    pub fn new(
        label: impl Into<String>,
        message: impl Into<String>,
        color: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: 1,
            label:          label.into(),
            message:        message.into(),
            color:          color.into(),
        }
    }
}

#[cfg(test)]
#[path = "release_tests.rs"]
mod tests;
