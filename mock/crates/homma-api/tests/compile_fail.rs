//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Cases that must not compile.
//!
//! `harness-the-type-system.md`: when the right fix makes a case
//! unrepresentable, the case does not vanish from the suite, it becomes a
//! compile-fail test asserting that it stays unrepresentable. Otherwise a later
//! loosening of a bound silently restores the illegal state and every remaining
//! test still passes, because none of them names it.

#[test]
fn a_relative_path_cannot_reach_the_git_contract() {
    trybuild::TestCases::new().compile_fail("tests/compile_fail/*.rs");
}
