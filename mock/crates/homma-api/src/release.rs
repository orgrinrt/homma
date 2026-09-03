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

/// A semver triple with an optional prerelease, compared the way cargo and jsr
/// compare them: numerically per part, and a prerelease sorts before its own
/// release.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Version {
    pub major:      u64,
    pub minor:      u64,
    pub patch:      u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prerelease: Option<String>,
}

impl Version {
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
            prerelease: None,
        }
    }

    /// The version a level makes of this one. Before 1.0 a major is a minor,
    /// since that is what every resolver reads a `0.x` bump as, and a
    /// prerelease is dropped whatever the level.
    pub fn bumped(&self, level: Level) -> Version {
        let level = if self.major == 0 && level == Level::Major { Level::Minor } else { level };
        match level {
            Level::Patch => Version::new(self.major, self.minor, self.patch + 1),
            Level::Minor => Version::new(self.major, self.minor + 1, 0),
            Level::Major => Version::new(self.major + 1, 0, 0),
        }
    }

    /// Whether `next` is exactly one legal step above this version at the
    /// given level, which is what a release refuses to skip past.
    pub fn is_smallest_successor(&self, next: &Version, level: Level) -> bool {
        &self.bumped(level) == next
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| {
                match (&self.prerelease, &other.prerelease) {
                    (None, None) => std::cmp::Ordering::Equal,
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (Some(a), Some(b)) => a.cmp(b),
                }
            })
    }
}

impl FromStr for Version {
    type Err = NotAVersion;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.strip_prefix('v').unwrap_or(s);
        let (core, prerelease) = match s.split_once('-') {
            Some((c, p)) if !p.is_empty() => (c, Some(p.to_string())),
            Some(_) => return Err(NotAVersion(s.to_string())),
            None => (s, None),
        };
        let mut parts = core.split('.');
        let mut part = || {
            parts
                .next()
                .and_then(|p| p.parse::<u64>().ok())
                .ok_or_else(|| NotAVersion(s.to_string()))
        };
        let (major, minor, patch) = (part()?, part()?, part()?);
        if parts.next().is_some() {
            return Err(NotAVersion(s.to_string()));
        }
        Ok(Version {
            major,
            minor,
            patch,
            prerelease,
        })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(p) = &self.prerelease {
            write!(f, "-{p}")?;
        }
        Ok(())
    }
}

/// A string that is not `X.Y.Z` or `X.Y.Z-pre`, with or without a leading `v`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotAVersion(pub String);

impl fmt::Display for NotAVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` is not a version", self.0)
    }
}

impl std::error::Error for NotAVersion {}

/// What a repository is, read off its root: a `Cargo.toml` makes it a crate,
/// a `deno.json` a deno package, and one carrying both is gated as both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepoKind {
    Crate,
    Deno,
    Both,
}

impl RepoKind {
    pub fn has_crate(self) -> bool {
        matches!(self, RepoKind::Crate | RepoKind::Both)
    }

    pub fn has_deno(self) -> bool {
        matches!(self, RepoKind::Deno | RepoKind::Both)
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
    pub fn summary(&self) -> String {
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
mod tests {
    use super::*;

    #[test]
    fn a_major_before_one_point_zero_is_a_minor() {
        assert_eq!(
            Version::new(0, 4, 1).bumped(Level::Major),
            Version::new(0, 5, 0)
        );
        assert_eq!(
            Version::new(1, 4, 1).bumped(Level::Major),
            Version::new(2, 0, 0)
        );
    }

    #[test]
    fn a_patch_and_a_minor_move_the_part_they_name_and_reset_below() {
        assert_eq!(
            Version::new(0, 4, 1).bumped(Level::Patch),
            Version::new(0, 4, 2)
        );
        assert_eq!(
            Version::new(0, 4, 1).bumped(Level::Minor),
            Version::new(0, 5, 0)
        );
        assert_eq!(
            Version::new(2, 4, 1).bumped(Level::Minor),
            Version::new(2, 5, 0)
        );
    }

    #[test]
    fn a_prerelease_is_dropped_by_any_bump_and_sorts_before_its_release() {
        let pre: Version = "1.2.0-alpha.1".parse().unwrap();
        assert_eq!(pre.bumped(Level::Patch), Version::new(1, 2, 1));
        assert!(pre < Version::new(1, 2, 0));
        assert!(Version::new(1, 1, 9) < pre);
    }

    #[test]
    fn a_version_parses_with_or_without_the_v_and_refuses_the_rest() {
        assert_eq!("v0.2.2".parse::<Version>().unwrap(), Version::new(0, 2, 2));
        assert_eq!("0.2.2".parse::<Version>().unwrap(), Version::new(0, 2, 2));
        assert!("0.2".parse::<Version>().is_err());
        assert!("0.2.2.1".parse::<Version>().is_err());
        assert!("0.2.2-".parse::<Version>().is_err());
        assert!("archive/x".parse::<Version>().is_err());
        assert_eq!(Version::new(3, 0, 0).to_string(), "3.0.0");
    }

    #[test]
    fn the_smallest_successor_is_exactly_one_step() {
        let v = Version::new(0, 2, 2);
        assert!(v.is_smallest_successor(&Version::new(0, 2, 3), Level::Patch));
        assert!(!v.is_smallest_successor(&Version::new(0, 2, 4), Level::Patch));
        assert!(!v.is_smallest_successor(&Version::new(0, 3, 0), Level::Patch));
    }

    #[test]
    fn a_level_parses_its_three_words_and_nothing_else() {
        assert_eq!("patch".parse::<Level>().unwrap(), Level::Patch);
        assert_eq!("major".parse::<Level>().unwrap(), Level::Major);
        assert!("Patch".parse::<Level>().is_err());
        assert!("release".parse::<Level>().is_err());
    }

    fn run() -> GateRun {
        let mut tests = StepOutcome {
            step:    Step::Tests,
            passed:  true,
            skipped: false,
            numbers: BTreeMap::new(),
            log:     "ok".into(),
        };
        tests.numbers.insert("summary".into(), "41/41".into());
        let docs = StepOutcome {
            step:    Step::Docs,
            passed:  false,
            skipped: false,
            numbers: BTreeMap::from([("summary".to_string(), "97%".to_string())]),
            log:     String::new(),
        };
        let steps = vec![
            StepOutcome::skipped(Step::Format),
            tests,
            StepOutcome::skipped(Step::Deny),
            docs,
        ];
        GateRun {
            repo: "notko".into(),
            sha: "abc123".into(),
            ran_at: "2026-09-02T21:00:00Z".into(),
            verdict: GateRun::verdict_of(&steps),
            steps,
        }
    }

    #[test]
    fn docs_failing_does_not_redden_but_a_blocking_step_does() {
        assert_eq!(run().verdict, Verdict::Green);
        let mut red = run();
        red.steps[1].passed = false;
        assert_eq!(GateRun::verdict_of(&red.steps), Verdict::Red);
    }

    #[test]
    fn a_run_round_trips_through_its_record_and_the_kind_checks_it() {
        let r = run();
        let record = r.to_record();
        record.check(&GateRun::kind()).unwrap();
        assert_eq!(GateRun::from_record(&record).unwrap(), r);
    }

    #[test]
    fn a_record_of_another_kind_is_refused() {
        let mut record = run().to_record();
        record.kind = "message".into();
        assert!(GateRun::from_record(&record).is_err());
        let mut bad = run().to_record();
        bad.attrs
            .insert("verdict".into(), Attr::Text("amber".into()));
        assert!(GateRun::from_record(&bad).is_err());
    }

    #[test]
    fn the_summary_skips_skipped_steps_and_names_a_failure_without_a_number() {
        assert_eq!(run().summary(), "tests 41/41, docs 97%");
        let mut r = run();
        r.steps[1].numbers.clear();
        r.steps[1].passed = false;
        assert_eq!(r.summary(), "tests failed, docs 97%");
    }

    #[test]
    fn a_badge_serialises_in_the_endpoint_shape() {
        let json = serde_json::to_string(&Badge::new("tests", "41/41", "green")).unwrap();
        assert_eq!(
            json,
            r#"{"schemaVersion":1,"label":"tests","message":"41/41","color":"green"}"#
        );
    }

    #[test]
    fn only_error_and_fatal_block() {
        assert!(!CheckSeverity::Warn.blocks());
        assert!(CheckSeverity::Error.blocks());
        assert!(CheckSeverity::Fatal.blocks());
    }
}
