//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital

use std::os::unix::fs::PermissionsExt;

use super::*;

fn mode(p: &Path) -> u32 {
    std::fs::metadata(p).unwrap().permissions().mode() & 0o777
}

#[test]
fn a_planted_script_holds_its_text_and_mode_in_a_directory_made_for_it() {
    let dir = tempfile::tempdir().unwrap();
    let run = dir.path().join("a/b/run");
    script(&run, "#!/bin/sh\necho 'it ran'\n", 0o755);
    assert_eq!(
        std::fs::read_to_string(&run).unwrap(),
        "#!/bin/sh\necho 'it ran'\n"
    );
    assert_eq!(mode(&run), 0o755);
    let out = Command::new(&run).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout), "it ran\n");
}

#[test]
fn a_script_planted_to_stay_unrun_keeps_the_mode_it_was_given() {
    let dir = tempfile::tempdir().unwrap();
    let kept = dir.path().join("kept");
    script(&kept, "x", 0o644);
    assert_eq!(mode(&kept), 0o644);
}

#[test]
fn a_script_that_cannot_be_planted_fails_the_test() {
    // Failing control of the check above: a path under a file is no place to write.
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("file");
    std::fs::write(&file, "").unwrap();
    let res = std::panic::catch_unwind(|| script(&Path::new(&file).join("under"), "x", 0o755));
    assert!(res.is_err());
}

const CHILD: &str = "HOMMA_FIXTURE_TEST_CHILD";

#[test]
fn a_test_run_alone_sees_what_only_its_child_was_given() {
    if let Some(v) = std::env::var_os(CHILD) {
        assert_eq!(v, "given");
        return;
    }
    assert!(std::env::var_os(CHILD).is_none());
    let out = alone(
        module_path!(),
        "a_test_run_alone_sees_what_only_its_child_was_given",
    )
    .env(CHILD, "given")
    .output()
    .unwrap();
    passed(&out);
}

#[test]
fn a_child_whose_test_fails_is_not_passed() {
    if std::env::var_os(CHILD).is_some() {
        panic!("the child fails");
    }
    let out = alone(module_path!(), "a_child_whose_test_fails_is_not_passed")
        .env(CHILD, "1")
        .output()
        .unwrap();
    assert!(std::panic::catch_unwind(|| passed(&out)).is_err());
}

#[test]
fn a_child_that_ran_no_test_is_not_passed() {
    let out = alone(module_path!(), "no_test_has_this_name")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "libtest exits zero when it runs nothing"
    );
    assert!(std::panic::catch_unwind(|| passed(&out)).is_err());
}

#[test]
fn a_child_that_ran_eleven_tests_is_not_one_that_ran_its_one() {
    // Failing control: a check for the substring "1 passed" takes this summary.
    use std::os::unix::process::ExitStatusExt;
    let out = Output {
        status: std::process::ExitStatus::from_raw(0),
        stdout: b"running 11 tests\n\ntest result: ok. 11 passed; 0 failed; 0 ignored\n".to_vec(),
        stderr: Vec::new(),
    };
    assert!(std::panic::catch_unwind(|| passed(&out)).is_err());
}
