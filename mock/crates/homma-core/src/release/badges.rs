//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The badge files a gate run and a version become, and the orphan `badges`
//! branch they are written to. The json is the one shape shields' endpoint
//! badge reads, which is the reason it is json and not toml.

use std::path::Path;

use homma_api::{Badge, GateRun, Step, Verdict, Version};

use super::git::{self, GitError};

/// The branch the files live on.
pub const BRANCH: &str = "badges";

/// The files for `run` and `version`: `gate.json`, `tests.json`,
/// `docs.json`, `deny.json` where deny ran, and `version.json`.
pub fn files(run: &GateRun, version: &Version) -> Vec<(String, Badge)> {
    let mut out = Vec::new();
    let (gate_text, gate_colour) = match run.verdict {
        Verdict::Green => ("passing", "brightgreen"),
        Verdict::Red => ("failing", "red"),
    };
    out.push(("gate.json".into(), Badge::new("gate", gate_text, gate_colour)));
    for step in &run.steps {
        match step.step {
            Step::Tests => {
                if let (Some(t), Some(p)) = (step.numbers.get("tests"), step.numbers.get("passed"))
                {
                    let colour = if t == p { "brightgreen" } else { "red" };
                    out.push((
                        "tests.json".into(),
                        Badge::new("tests", format!("{p} of {t}"), colour),
                    ));
                }
            },
            Step::Docs => {
                if let Some(pct) = step.numbers.get("documented_percent") {
                    let n: f64 = pct.parse().unwrap_or(0.0);
                    let colour = if n >= 90.0 {
                        "brightgreen"
                    } else if n >= 60.0 {
                        "yellow"
                    } else {
                        "orange"
                    };
                    out.push(("docs.json".into(), Badge::new("docs", format!("{pct}%"), colour)));
                }
            },
            Step::Deny if !step.skipped => {
                let n = step.numbers.get("advisories").cloned().unwrap_or_default();
                let colour = if n == "0" { "brightgreen" } else { "red" };
                out.push((
                    "deny.json".into(),
                    Badge::new("advisories", n, colour),
                ));
            },
            _ => {},
        }
    }
    out.push((
        "version.json".into(),
        Badge::new("version", version.to_string(), "blue"),
    ));
    out
}

/// Rewrite the `badges` branch at `root` with `files`, as one commit with no
/// parent, and return its sha.
pub fn write(root: &Path, files: &[(String, Badge)]) -> Result<String, GitError> {
    let rendered: Vec<(String, String)> = files
        .iter()
        .map(|(name, badge)| {
            let mut text = serde_json::to_string_pretty(badge).expect("a badge serialises");
            text.push('\n');
            (name.clone(), text)
        })
        .collect();
    let entries: Vec<(&str, &str)> = rendered
        .iter()
        .map(|(n, t)| (n.as_str(), t.as_str()))
        .collect();
    git::write_orphan_branch(root, BRANCH, &entries, "badges")
}

#[cfg(test)]
mod tests {
    use homma_api::StepOutcome;

    use super::*;

    fn outcome(step: Step, numbers: &[(&str, &str)]) -> StepOutcome {
        let mut o = StepOutcome::skipped(step);
        o.skipped = false;
        for (k, v) in numbers {
            o.numbers.insert(k.to_string(), v.to_string());
        }
        o
    }

    fn run(steps: Vec<StepOutcome>) -> GateRun {
        GateRun {
            repo: "x".into(),
            sha: "s".into(),
            ran_at: "t".into(),
            verdict: GateRun::verdict_of(&steps),
            steps,
        }
    }

    #[test]
    fn every_measured_thing_gets_a_file_and_deny_only_where_it_ran() {
        let r = run(vec![
            outcome(Step::Tests, &[("tests", "12"), ("passed", "12")]),
            outcome(Step::Docs, &[("documented_percent", "95.0")]),
            StepOutcome::skipped(Step::Deny),
        ]);
        let f = files(&r, &Version::new(1, 2, 3));
        let names: Vec<&str> = f.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["gate.json", "tests.json", "docs.json", "version.json"]);
        assert_eq!(f[0].1.message, "passing");
        assert_eq!(f[1].1.message, "12 of 12");
        assert_eq!(f[1].1.color, "brightgreen");
        assert_eq!(f[2].1.color, "brightgreen");
        assert_eq!(f[3].1.message, "1.2.3");
        assert_eq!(f[3].1.schema_version, 1);
    }

    #[test]
    fn a_failing_tests_step_and_advisories_colour_red() {
        let mut tests = outcome(Step::Tests, &[("tests", "3"), ("passed", "1")]);
        tests.passed = false;
        let r = run(vec![tests, outcome(Step::Deny, &[("advisories", "2")])]);
        let f = files(&r, &Version::new(0, 1, 0));
        assert_eq!(f[0].1.message, "failing");
        assert_eq!(f[0].1.color, "red");
        assert_eq!(f[1].1.color, "red");
        let deny = f.iter().find(|(n, _)| n == "deny.json").unwrap();
        assert_eq!((deny.1.message.as_str(), deny.1.color.as_str()), ("2", "red"));
    }

    #[test]
    fn docs_colour_bands_by_the_fraction() {
        for (pct, colour) in [("100.0", "brightgreen"), ("75.5", "yellow"), ("12.0", "orange")] {
            let r = run(vec![outcome(Step::Docs, &[("documented_percent", pct)])]);
            let f = files(&r, &Version::new(0, 1, 0));
            assert_eq!(f[1].1.color, colour, "{pct}");
        }
    }

    #[test]
    fn the_branch_holds_only_the_json_and_has_no_parent() {
        let d = tempfile::tempdir().unwrap();
        let g = |args: &[&str]| {
            let out = super::super::sh::run(d.path(), "git", args).unwrap();
            assert!(out.ok(), "{}", out.log());
        };
        g(&["init", "-q", "-b", "main"]);
        std::fs::write(d.path().join("a"), "a").unwrap();
        g(&["-c", "user.name=t", "-c", "user.email=t@t", "add", "."]);
        g(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "one"]);
        let r = run(vec![outcome(Step::Tests, &[("tests", "1"), ("passed", "1")])]);
        let f = files(&r, &Version::new(0, 1, 0));
        let sha = write(d.path(), &f).unwrap();
        assert_eq!(git::parent_count(d.path(), &sha).unwrap(), 0);
        let on = git::files_on(d.path(), BRANCH).unwrap();
        let names: Vec<&str> = on.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["gate.json", "tests.json", "version.json"]);
        let parsed: Badge = serde_json::from_str(&on[1].1).unwrap();
        assert_eq!(parsed.message, "1 of 1");
        assert!(on[1].1.contains("\"schemaVersion\": 1"));
        let again = write(d.path(), &f).unwrap();
        assert_eq!(git::parent_count(d.path(), &again).unwrap(), 0, "a rewrite is still an orphan");
        assert!(git::current_branch(d.path()).unwrap().as_deref() == Some("main"));
    }
}
