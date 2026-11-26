//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital

//! `alone` from a test at the root of an integration test binary, whose module
//! path is the binary's name alone.

const CHILD: &str = "HOMMA_FIXTURE_ROOT_TEST_CHILD";

/// Failing control: with the bare-name branch naming `<module>::<name>`, the
/// child runs no test.
#[test]
fn a_test_at_the_root_of_its_binary_runs_alone_by_its_bare_name() {
    if std::env::var_os(CHILD).is_some() {
        return;
    }
    assert!(
        !module_path!().contains("::"),
        "this test sits at the binary's root"
    );
    let out = homma_fixture::alone(
        module_path!(),
        "a_test_at_the_root_of_its_binary_runs_alone_by_its_bare_name",
    )
    .env(CHILD, "1")
    .output()
    .unwrap();
    homma_fixture::passed(&out);
}
