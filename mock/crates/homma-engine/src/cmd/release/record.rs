//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! A gate run in the store: appended after every run, read back by the
//! release, which wants the newest run on the exact sha it is about to move.

use homma_api::GateRun;
use homma_store::Store;

/// Append `run` under the `gate-run` kind.
pub fn append(store: &Store, run: &GateRun) -> Result<(), homma_store::Error> {
    store.append(&GateRun::kind(), &run.to_record())
}

/// The newest run recorded for `repo` at `sha`, or none. A run on another
/// sha, however recent, is not evidence about this one and is not returned.
pub fn newest_for(
    store: &Store,
    repo: &str,
    sha: &str,
) -> Result<Option<GateRun>, homma_store::Error> {
    let mut runs: Vec<GateRun> = store
        .read(homma_api::GATE_RUN_KIND)?
        .iter()
        .filter_map(|r| GateRun::from_record(r).ok())
        .filter(|r| r.repo == repo && r.sha == sha)
        .collect();
    runs.sort_by(|a, b| a.ran_at.cmp(&b.ran_at));
    Ok(runs.pop())
}

#[cfg(test)]
mod tests {
    use homma_api::{Step, StepOutcome, Verdict};

    use super::*;

    fn run(repo: &str, sha: &str, at: &str, verdict: Verdict) -> GateRun {
        let mut step = StepOutcome::skipped(Step::Tests);
        step.skipped = false;
        step.passed = verdict == Verdict::Green;
        GateRun {
            repo: repo.into(),
            sha: sha.into(),
            ran_at: at.into(),
            verdict,
            steps: vec![step],
        }
    }

    #[test]
    fn the_newest_run_on_the_exact_sha_comes_back_and_another_sha_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path());
        append(
            &store,
            &run("x", "aaa", "2026-09-02T20:00:00Z", Verdict::Red),
        )
        .unwrap();
        append(
            &store,
            &run("x", "aaa", "2026-09-02T21:00:00Z", Verdict::Green),
        )
        .unwrap();
        append(
            &store,
            &run("x", "bbb", "2026-09-02T22:00:00Z", Verdict::Green),
        )
        .unwrap();
        append(
            &store,
            &run("y", "aaa", "2026-09-02T23:00:00Z", Verdict::Green),
        )
        .unwrap();
        let got = newest_for(&store, "x", "aaa").unwrap().unwrap();
        assert_eq!(got.ran_at, "2026-09-02T21:00:00Z");
        assert_eq!(got.verdict, Verdict::Green);
        assert!(newest_for(&store, "x", "ccc").unwrap().is_none());
        assert!(newest_for(&store, "z", "aaa").unwrap().is_none());
    }

    #[test]
    fn an_empty_store_has_no_run() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path());
        assert!(newest_for(&store, "x", "aaa").unwrap().is_none());
    }
}
