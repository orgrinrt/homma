//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The commit status a gate run becomes: `homma/gate`, green or red, with the
//! numbers in the description in the order the step table lists them.

use homma_api::{GateRun, Step, Verdict};

use crate::forge::{CommitStatus, Forge, ForgeError, StatusState};

/// The context every ruleset names.
pub const CONTEXT: &str = "homma/gate";

/// The status for `run`, without posting it.
pub fn status_for(run: &GateRun) -> CommitStatus {
    CommitStatus {
        context:     CONTEXT.into(),
        state:       match run.verdict {
            Verdict::Green => StatusState::Success,
            Verdict::Red => StatusState::Failure,
        },
        description: description(run),
        target_url:  None,
    }
}

/// `green, 12 of 12 tests, 83.3% documented, 0 advisories, 41.0s`, or the
/// red form naming the steps that failed, and both forges cap a description
/// at 140 characters so the tail is dropped rather than refused.
pub fn description(run: &GateRun) -> String {
    let mut parts = vec![run.verdict.to_string()];
    if run.verdict == Verdict::Red {
        let failed: Vec<&str> = run
            .steps
            .iter()
            .filter(|s| s.is_red())
            .map(|s| s.step.name())
            .collect();
        if !failed.is_empty() {
            parts.push(format!("failed {}", failed.join(", ")));
        }
    }
    for step in &run.steps {
        match step.step {
            Step::Tests => {
                if let (Some(t), Some(p)) = (step.numbers.get("tests"), step.numbers.get("passed"))
                {
                    parts.push(format!("{p} of {t} tests"));
                }
            },
            Step::Deny => {
                if let Some(n) = step.numbers.get("advisories") {
                    parts.push(format!("{n} advisories"));
                }
            },
            Step::Docs => {
                if let Some(pct) = step.numbers.get("documented_percent") {
                    parts.push(format!("{pct}% documented"));
                }
            },
            _ => {},
        }
        if let Some(w) = step.numbers.get("wall_seconds") {
            parts.push(format!("{w}s"));
        }
    }
    let mut text = parts.join(", ");
    if text.len() > 140 {
        text.truncate(137);
        text.push_str("...");
    }
    text
}

/// Post the status for `run` on the sha it measured.
pub fn post(forge: &dyn Forge, owner: &str, name: &str, run: &GateRun) -> Result<(), ForgeError> {
    forge.set_commit_status(owner, name, &run.sha, &status_for(run))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use homma_api::StepOutcome;

    use super::*;
    use crate::forge::{CreateRepoSpec, RepoMetadata};

    fn outcome(step: Step, passed: bool, numbers: &[(&str, &str)]) -> StepOutcome {
        let mut o = StepOutcome::skipped(step);
        o.skipped = false;
        o.passed = passed;
        for (k, v) in numbers {
            o.numbers.insert(k.to_string(), v.to_string());
        }
        o
    }

    fn run(steps: Vec<StepOutcome>) -> GateRun {
        GateRun {
            repo: "x".into(),
            sha: "abc".into(),
            ran_at: "t".into(),
            verdict: GateRun::verdict_of(&steps),
            steps,
        }
    }

    #[test]
    fn a_green_run_lists_its_numbers_in_table_order() {
        let r = run(vec![
            outcome(Step::Format, true, &[]),
            outcome(Step::Tests, true, &[("tests", "12"), ("passed", "12")]),
            outcome(Step::Deny, true, &[("advisories", "0")]),
            outcome(Step::Docs, true, &[("documented_percent", "83.3")]),
            outcome(Step::Notices, true, &[("wall_seconds", "41.0")]),
        ]);
        assert_eq!(
            description(&r),
            "green, 12 of 12 tests, 0 advisories, 83.3% documented, 41.0s"
        );
        let s = status_for(&r);
        assert_eq!(s.state, StatusState::Success);
        assert_eq!(s.context, "homma/gate");
    }

    #[test]
    fn a_red_run_names_the_steps_that_failed_and_a_failing_docs_step_is_not_one() {
        let r = run(vec![
            outcome(Step::Lint, false, &[]),
            outcome(Step::Tests, false, &[("tests", "3"), ("passed", "1")]),
            outcome(Step::Docs, false, &[]),
        ]);
        assert_eq!(description(&r), "red, failed lint, tests, 1 of 3 tests");
        assert_eq!(status_for(&r).state, StatusState::Failure);
    }

    #[test]
    fn a_description_never_exceeds_the_forges_cap() {
        let steps = (0 .. 40)
            .map(|_| {
                outcome(Step::Tests, true, &[
                    ("tests", "1000000"),
                    ("passed", "1000000"),
                ])
            })
            .collect();
        let d = description(&run(steps));
        assert!(d.len() <= 140, "{}", d.len());
        assert!(d.ends_with("..."));
    }

    struct Recorder(RefCell<Vec<(String, String, String, CommitStatus)>>);

    impl Forge for Recorder {
        fn fetch_repo(&self, _: &str, _: &str) -> Result<RepoMetadata, ForgeError> {
            unreachable!()
        }

        fn repo_exists(&self, _: &str, _: &str) -> Result<bool, ForgeError> {
            unreachable!()
        }

        fn create_repo(&self, _: &str, _: &CreateRepoSpec) -> Result<RepoMetadata, ForgeError> {
            unreachable!()
        }

        fn archive_repo(&self, _: &str, _: &str) -> Result<(), ForgeError> {
            unreachable!()
        }

        fn delete_repo(&self, _: &str, _: &str) -> Result<(), ForgeError> {
            unreachable!()
        }

        fn credential_works(&self) -> Result<bool, ForgeError> {
            Ok(true)
        }

        fn set_commit_status(
            &self,
            owner: &str,
            name: &str,
            sha: &str,
            status: &CommitStatus,
        ) -> Result<(), ForgeError> {
            self.0
                .borrow_mut()
                .push((owner.into(), name.into(), sha.into(), status.clone()));
            Ok(())
        }

        fn create_release(&self, _: &str, _: &str, _: &str, _: &str) -> Result<(), ForgeError> {
            unreachable!()
        }
    }

    #[test]
    fn posting_goes_to_the_sha_the_run_measured() {
        let forge = Recorder(RefCell::new(Vec::new()));
        let r = run(vec![outcome(Step::Format, true, &[])]);
        post(&forge, "o", "r", &r).unwrap();
        let seen = forge.0.borrow();
        assert_eq!(seen.len(), 1);
        assert_eq!(
            (seen[0].0.as_str(), seen[0].1.as_str(), seen[0].2.as_str()),
            ("o", "r", "abc")
        );
        assert_eq!(seen[0].3.state, StatusState::Success);
    }
}
