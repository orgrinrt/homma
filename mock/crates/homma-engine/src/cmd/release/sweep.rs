//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! What a workspace-wide release does with each repo's plan: takes it, passes
//! it over with why, or refuses the sweep where passing it over would leave it
//! unreleased without a word.

use anyhow::{Result, anyhow};
use homma_core::release::plan::{Plan, PlanError};

/// What the sweep does with one repo.
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Take,
    /// Passed over, with the line that says why.
    PassOver(String),
}

/// The verdict on `name`'s plan. A manifest off the level, or a next
/// version no single step above the last tag, refuses a single run, so it
/// refuses the sweep too rather than being passed over.
pub fn verdict(name: &str, plan: Result<Plan, PlanError>) -> Result<Verdict> {
    match plan {
        Ok(p) if p.commits.is_empty() => {
            Ok(Verdict::PassOver(format!(
                "{name}: nothing unreleased, passed over"
            )))
        },
        Ok(_) => Ok(Verdict::Take),
        Err(
            e @ (PlanError::OffLevel(_)
            | PlanError::NotOneStep {
                ..
            }),
        ) => Err(anyhow!("{name}: {e}")),
        Err(e) => Ok(Verdict::PassOver(format!("{name}: passed over, {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use homma_api::Version;

    use super::*;

    #[test]
    fn a_next_version_off_by_a_step_refuses_the_sweep_and_names_the_repo() {
        let err = verdict(
            "renki",
            Err(PlanError::NotOneStep {
                what: "the last tag".into(),
                last: Box::new("0.0.5-rc.1".parse().unwrap()),
                next: Box::new(Version::new(0, 0, 6)),
            }),
        )
        .unwrap_err();
        let said = err.to_string();
        assert!(said.starts_with("renki: "), "{said}");
        assert!(
            said.contains("0.0.6") && said.contains("0.0.5-rc.1"),
            "{said}"
        );
        let other = verdict("x", Err(PlanError::Cycle("a, b".into()))).unwrap();
        assert!(
            matches!(&other, Verdict::PassOver(l) if l.starts_with("x: passed over, ") && l.contains("a, b")),
            "{other:?}"
        );
    }
}
