//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The stores: the keychain program that is not there, the environment variable, the comment date, and
//! what a debug rendering shows.

use super::*;

#[test]
fn the_keychain_comment_carries_the_expiry() {
    let shown = "keychain: \"/x\"\nattributes:\n    \"icmt\"<blob>=\"2027-01-01\"\n";
    assert_eq!(comment_date(shown), Day::parse("2027-01-01"));
    assert_eq!(comment_date("    \"icmt\"<blob>=<NULL>\n"), None);
    assert_eq!(comment_date("nothing here"), None);
}

/// What crashed the python on Linux: its first line printed, then the
/// keychain program was not there and nothing caught it. Every line still
/// renders, each token entry naming why, and the status is not fine.
#[test]
fn a_keychain_program_that_is_not_there_is_a_reason_on_each_line_not_a_crash() {
    let store = Keychain {
        program: "/nonexistent/security".into(),
        account: "publish".into(),
    };
    struct Quiet;
    impl Probe for Quiet {
        fn whoami(&self, _: &str, _: &str, _: bool) -> Answer {
            panic!("a registry was asked with no token to ask about")
        }

        fn cli(&self, _: &[&str]) -> Option<(bool, String)> {
            None
        }
    }
    let (lines, ok) = flow::status(ENTRIES, &store, &Quiet, today());
    assert!(!ok);
    assert_eq!(lines[0], "publish credentials\n");
    assert_eq!(lines.len(), 1 + ENTRIES.len());
    for (e, l) in ENTRIES.iter().zip(&lines[1 ..]) {
        assert!(l.contains(e.key), "{l}");
        if e.token().is_some() {
            assert!(l.contains("   ??  "), "{l}");
            assert!(l.contains("/nonexistent/security"), "{l}");
        }
    }
}

#[test]
fn the_environment_store_reads_its_variable_and_refuses_a_write_by_name() {
    let var = "HOMMA_AUTH_TEST_TOKEN_UNSET_ELSEWHERE";
    let store = Environment {
        vars: vec![("crates-io".into(), var.into())],
    };
    assert_eq!(store.get("crates-io").unwrap(), None);
    let no_var = store.get("jsr").unwrap_err().to_string();
    assert!(no_var.contains("[registries.jsr]"), "{no_var}");
    let refused = store.put("crates-io", "x", None).unwrap_err().to_string();
    assert!(refused.contains(var), "{refused}");
    assert!(store.forget("crates-io").is_err());
}

#[cfg(target_os = "macos")]
mod keychain_round_trip {
    //! The real keychain, under a name nothing else uses.
    use super::*;

    const KEY: &str = "selftest-auth-suite";

    fn kc() -> Keychain {
        Keychain::new()
    }

    struct Clean;
    impl Drop for Clean {
        fn drop(&mut self) {
            let _ = kc().forget(KEY);
        }
    }

    #[test]
    fn absent_reads_as_none() {
        let _c = Clean;
        let _ = kc().forget(KEY);
        assert_eq!(kc().get(KEY).unwrap(), None);
    }

    #[test]
    fn secret_and_expiry_survive() {
        let _c = Clean;
        kc().put(KEY, "hunter2", Day::parse("2027-01-01")).unwrap();
        let got = kc().get(KEY).unwrap().unwrap();
        assert_eq!(got.secret, "hunter2");
        assert_eq!(got.expires, Day::parse("2027-01-01"));
    }

    #[test]
    fn no_expiry_is_representable() {
        let _c = Clean;
        kc().put(KEY, "hunter2", None).unwrap();
        assert_eq!(kc().get(KEY).unwrap().unwrap().expires, None);
    }

    #[test]
    fn put_replaces_rather_than_duplicating() {
        let _c = Clean;
        kc().put(KEY, "first", None).unwrap();
        kc().put(KEY, "second", None).unwrap();
        assert_eq!(kc().get(KEY).unwrap().unwrap().secret, "second");
    }

    #[test]
    fn forget_reports_whether_there_was_anything() {
        let _c = Clean;
        kc().put(KEY, "x", None).unwrap();
        assert!(kc().forget(KEY).unwrap());
        assert!(!kc().forget(KEY).unwrap());
    }
}

#[test]
fn a_stored_secret_is_not_in_its_debug_rendering() {
    let s = Stored {
        secret:  "hunter2-the-secret".into(),
        expires: Day::parse("2027-01-01"),
    };
    let shown = format!("{s:?}");
    assert!(!shown.contains("hunter2"), "{shown}");
    assert!(shown.contains("redacted"), "{shown}");
    assert!(
        shown.contains("2027-01-01"),
        "the expiry is no secret: {shown}"
    );
    let boxed = format!("{:#?}", Some(&s));
    assert!(!boxed.contains("hunter2"), "{boxed}");
}

#[test]
fn only_the_environment_store_says_up_front_that_it_cannot_write() {
    let var = "HOMMA_AUTH_TEST_TOKEN_UNSET_ELSEWHERE";
    let env = Environment {
        vars: vec![("crates-io".into(), var.into())],
    };
    let refused = env.writable("crates-io").unwrap_err().to_string();
    assert!(refused.contains(var), "{refused}");
    assert!(
        env.writable("jsr")
            .unwrap_err()
            .to_string()
            .contains("[registries.jsr]")
    );
    assert!(Keychain::new().writable("crates-io").is_ok());
}
