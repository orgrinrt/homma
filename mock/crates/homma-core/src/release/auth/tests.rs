//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The expiry grading, how each registry's answer reads, the store's
//! refusals, and the one rule that matters most: a token the registry has
//! not accepted is never stored.

use std::cell::RefCell;
use std::io::{Read, Write};
use std::path::Path;

use super::flow::{self, Prompt};
use super::store::{Environment, Keychain, comment_date};
use super::*;

fn today() -> Day {
    Day::parse("2026-11-23").unwrap()
}

/// A store in memory that records every write.
#[derive(Default)]
struct Memory {
    held: RefCell<Vec<(String, Stored)>>,
    puts: RefCell<Vec<String>>,
}

impl Memory {
    fn with(key: &str, secret: &str, expires: Option<Day>) -> Self {
        let m = Memory::default();
        m.held.borrow_mut().push((key.into(), Stored {
            secret: secret.into(),
            expires,
        }));
        m
    }
}

impl Store for Memory {
    fn name(&self) -> String {
        "memory".into()
    }

    fn get(&self, key: &str) -> Result<Option<Stored>, Unavailable> {
        Ok(self
            .held
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, s)| s.clone()))
    }

    fn put(&self, key: &str, secret: &str, expires: Option<Day>) -> Result<(), Unavailable> {
        self.puts.borrow_mut().push(secret.into());
        let mut held = self.held.borrow_mut();
        held.retain(|(k, _)| k != key);
        held.push((key.into(), Stored {
            secret: secret.into(),
            expires,
        }));
        Ok(())
    }

    fn forget(&self, key: &str) -> Result<bool, Unavailable> {
        let mut held = self.held.borrow_mut();
        let before = held.len();
        held.retain(|(k, _)| k != key);
        Ok(held.len() != before)
    }
}

/// A registry that answers one way, and a cli that must not be asked.
struct Canned(Answer);

impl Probe for Canned {
    fn whoami(&self, _: &str, _: &str, _: bool) -> Answer {
        self.0.clone()
    }

    fn cli(&self, argv: &[&str]) -> Option<(bool, String)> {
        panic!("shelled out: {argv:?}")
    }
}

fn token_of(key: &str) -> &'static Token {
    entry(key).unwrap().token().unwrap()
}

fn grade_of(expires: Option<Day>) -> State {
    grade(
        &Stored {
            secret: "t".into(),
            expires,
        },
        today(),
        "as someone",
    )
    .state
}

#[test]
fn no_expiry_is_ok() {
    assert_eq!(grade_of(None), State::Ok);
}

#[test]
fn far_future_is_ok() {
    assert_eq!(grade_of(Some(today().plus(365))), State::Ok);
}

#[test]
fn inside_two_weeks_is_expiring() {
    assert_eq!(grade_of(Some(today().plus(3))), State::Expiring);
}

#[test]
fn already_past_is_invalid() {
    assert_eq!(grade_of(Some(today().plus(-1))), State::Invalid);
}

#[test]
fn days_left_counts_to_the_expiry() {
    let s = Stored {
        secret:  "hunter2".into(),
        expires: Day::parse("2027-01-01"),
    };
    assert_eq!(s.days_left(Day::parse("2026-12-25").unwrap()), Some(7));
}

fn npm_with(answer: Answer, stored: bool) -> Status {
    let store = if stored { Memory::with("npm", "npm-token", None) } else { Memory::default() };
    check(entry("npm").unwrap(), &store, &Canned(answer), today())
}

#[test]
fn nothing_stored_is_missing() {
    assert_eq!(npm_with((200, None), false).state, State::Missing);
}

#[test]
fn a_401_is_a_rejection_of_the_token_we_hold() {
    assert_eq!(npm_with((401, None), true).state, State::Invalid);
}

#[test]
fn a_200_names_the_holder() {
    let st = npm_with(
        (200, Some(serde_json::json!({"username": "orgrinrt"}))),
        true,
    );
    assert_eq!(st.state, State::Ok);
    assert!(st.detail.contains("orgrinrt"), "{}", st.detail);
}

#[test]
fn no_answer_is_unreachable_rather_than_invalid() {
    assert_eq!(npm_with((0, None), true).state, State::Unreachable);
}

#[test]
fn it_asks_the_registry_rather_than_the_npm_cli() {
    // `Canned::cli` panics, so reaching it fails the test
    assert_eq!(
        npm_with((200, Some(serde_json::json!({"username": "x"}))), true).state,
        State::Ok
    );
}

#[test]
fn each_token_registry_names_its_own_endpoint() {
    for e in ENTRIES {
        let Some(t) = e.token() else { continue };
        assert!(!t.whoami_url.is_empty(), "{}", e.key);
        assert!(
            t.whoami_url.contains(e.key.split('-').next().unwrap()),
            "{}",
            e.key
        );
    }
}

#[test]
fn only_crates_io_sends_the_token_bare() {
    for e in ENTRIES {
        let Some(t) = e.token() else { continue };
        assert_eq!(t.bearer, e.key != "crates-io", "{}", e.key);
    }
}

fn crates_with(answer: Answer) -> Status {
    judge(
        token_of("crates-io"),
        Some(&Stored {
            secret:  "a-token".into(),
            expires: None,
        }),
        &Canned(answer),
        today(),
    )
}

#[test]
fn a_401_from_crates_io_is_a_real_rejection() {
    assert_eq!(crates_with((401, None)).state, State::Invalid);
}

#[test]
fn a_403_is_a_scoped_token_and_must_not_be_rejected() {
    let st = crates_with((403, None));
    assert_ne!(
        st.state,
        State::Invalid,
        "a publish-scoped token was rejected"
    );
    assert!(
        !st.state.needs_work(),
        "a valid scoped token was queued for re-entry"
    );
}

#[test]
fn a_200_names_the_crates_io_user() {
    let st = crates_with((
        200,
        Some(serde_json::json!({"user": {"login": "orgrinrt"}})),
    ));
    assert_eq!(st.state, State::Ok);
    assert!(st.detail.contains("orgrinrt"));
}

#[test]
fn no_answer_from_crates_io_is_unreachable_not_invalid() {
    assert_eq!(crates_with((0, None)).state, State::Unreachable);
}

#[test]
fn a_403_from_jsr_is_no_scoped_token() {
    let st = judge(
        token_of("jsr"),
        Some(&Stored {
            secret:  "t".into(),
            expires: None,
        }),
        &Canned((403, None)),
        today(),
    );
    assert_eq!(st.state, State::Unreachable);
}

#[test]
fn needs_work_covers_the_three_actionable_states() {
    for s in [State::Missing, State::Invalid, State::Expiring] {
        assert!(s.needs_work(), "{s}");
    }
}

#[test]
fn ok_and_unreachable_do_not_prompt() {
    for s in [State::Ok, State::Unreachable] {
        assert!(!s.needs_work(), "{s}");
    }
}

/// A terminal that pastes one token and records what it was told.
struct Pasting {
    token: &'static str,
    said:  RefCell<Vec<String>>,
}

impl Prompt for Pasting {
    fn say(&self, text: &str) {
        self.said.borrow_mut().push(text.into());
    }

    fn secret(&self, _: &str) -> String {
        self.token.into()
    }

    fn line(&self, _: &str) -> String {
        String::new()
    }

    fn open(&self, _: &str) -> bool {
        panic!("a browser was opened with the browser off")
    }

    fn hand_over(&self, _: &[&str]) -> Result<(), String> {
        Ok(())
    }
}

fn drive(answer: Answer) -> (Status, Memory) {
    let store = Memory::default();
    let prompt = Pasting {
        token: "pasted-token",
        said:  RefCell::default(),
    };
    let e = entry("jsr").unwrap();
    let st = flow::fix_token(
        e,
        e.token().unwrap(),
        &store,
        &Canned(answer),
        &prompt,
        today(),
        false,
    );
    (st, store)
}

#[test]
fn a_rejected_token_is_not_left_in_the_store() {
    let (st, store) = drive((401, None));
    assert_eq!(st.state, State::Invalid);
    assert_eq!(
        store.get("jsr").unwrap(),
        None,
        "a rejected token was left stored"
    );
    assert!(
        store.puts.borrow().is_empty(),
        "a rejected token reached the store at all"
    );
}

#[test]
fn an_accepted_token_is_kept() {
    let (st, store) = drive((200, Some(serde_json::json!({"name": "someone"}))));
    assert_eq!(st.state, State::Ok);
    assert_eq!(store.get("jsr").unwrap().unwrap().secret, "pasted-token");
}

fn npmrc_written(dir: &Path, secret: &str) -> std::path::PathBuf {
    let where_ = dir.join("npmrc");
    flow::write_npmrc(&where_, &Memory::with("npm", secret, None)).unwrap();
    where_
}

#[test]
fn the_line_is_the_one_npm_reads() {
    let d = tempfile::tempdir().unwrap();
    let at = npmrc_written(d.path(), "npm-token");
    assert_eq!(
        std::fs::read_to_string(at).unwrap(),
        "//registry.npmjs.org/:_authToken=npm-token\n"
    );
}

fn read_by_npm(path: &Path) -> String {
    let out = std::process::Command::new("npm")
        .args(["config", "list", "--userconfig"])
        .arg(path)
        .output()
        .expect("npm runs");
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    s
}

#[test]
fn npm_itself_recognises_the_key_we_wrote() {
    let d = tempfile::tempdir().unwrap();
    let said = read_by_npm(&npmrc_written(d.path(), "npm-token"));
    assert!(
        said.contains("//registry.npmjs.org/:_authToken = (protected)"),
        "{said}"
    );
    assert!(!said.contains("Unknown user config"), "{said}");
}

#[test]
fn a_misspelled_key_is_something_npm_complains_about() {
    let d = tempfile::tempdir().unwrap();
    let wrong = d.path().join("wrong");
    std::fs::write(&wrong, "//registry.npmjs.org/:_authTokenX=abc\n").unwrap();
    // npm 10 lists an unknown key as it does a known one, so the control is
    // that the misspelling is not read back as the key npm takes a token from
    let said = read_by_npm(&wrong);
    assert!(said.contains(":_authTokenX = (protected)"), "{said}");
    assert!(!said.contains(":_authToken = (protected)"), "{said}");
}

#[cfg(unix)]
#[test]
fn nobody_else_may_read_it() {
    use std::os::unix::fs::PermissionsExt;
    let d = tempfile::tempdir().unwrap();
    let at = npmrc_written(d.path(), "npm-token");
    assert_eq!(
        std::fs::metadata(at).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn a_rotated_token_does_not_leave_the_old_one_underneath() {
    let d = tempfile::tempdir().unwrap();
    npmrc_written(d.path(), "first");
    let at = npmrc_written(d.path(), "second");
    let text = std::fs::read_to_string(at).unwrap();
    assert!(text.contains("second") && !text.contains("first"), "{text}");
}

#[test]
fn nothing_stored_writes_nothing_and_says_so() {
    let d = tempfile::tempdir().unwrap();
    let at = d.path().join("npmrc");
    let err = flow::write_npmrc(&at, &Memory::default()).unwrap_err();
    assert!(!at.exists());
    assert!(err.contains("npm"), "{err}");
}

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

#[test]
fn the_export_lines_read_each_token_through_the_verbs() {
    let store = Memory::with("crates-io", "t", None);
    store.put("npm", "n", None).unwrap();
    let lines = flow::env_lines(ENTRIES, &store, "homma release auth");
    assert_eq!(lines, [
        "export CARGO_REGISTRY_TOKEN=\"$(homma release auth token crates-io)\"",
        "# JSR_AUTH_TOKEN unset: nothing stored for jsr",
        "export NPM_CONFIG_USERCONFIG=\"$(homma release auth npmrc)\"",
    ]);
}

#[test]
fn a_token_is_handed_out_only_for_a_token_registry() {
    let store = Memory::with("crates-io", "secret", None);
    assert_eq!(flow::token(ENTRIES, &store, "crates-io").unwrap(), "secret");
    assert!(flow::token(ENTRIES, &store, "github").is_err());
    assert!(
        flow::token(ENTRIES, &store, "jsr")
            .unwrap_err()
            .contains("nothing stored")
    );
}

#[test]
fn an_expiry_is_corrected_without_the_token_changing() {
    let store = Memory::with("jsr", "keep-me", None);
    let said = flow::expiry(ENTRIES, &store, "jsr", "2026-12-03", today()).unwrap();
    assert_eq!(said, "  jsr: expiry 2026-12-03, 10 days left");
    assert_eq!(store.get("jsr").unwrap().unwrap().secret, "keep-me");
    assert_eq!(
        flow::expiry(ENTRIES, &store, "jsr", "never", today()).unwrap(),
        "  jsr: expiry none"
    );
    assert!(flow::expiry(ENTRIES, &store, "jsr", "01/01/2027", today()).is_err());
}

#[test]
fn the_diagnosis_shows_the_token_masked_and_what_the_registry_said() {
    let store = Memory::with("jsr", "abcdefghijklmnop", None);
    let out = flow::diagnose(ENTRIES, &store, &Canned((401, None)), "jsr", today())
        .unwrap()
        .join("\n");
    assert!(out.contains("16 chars, abcd...mnop"), "{out}");
    assert!(!out.contains("efghijkl"), "{out}");
    assert!(out.contains("http: 401"), "{out}");
    assert!(out.contains("verdict: invalid"), "{out}");
}

/// Serve one request on a local port and hand back what arrived.
fn one_request(respond: &'static str) -> (String, std::thread::JoinHandle<String>) {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/me", l.local_addr().unwrap());
    let h = std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut buf = [0u8; 4096];
        let mut got = Vec::new();
        while !got.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = s.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            got.extend_from_slice(&buf[.. n]);
        }
        s.write_all(respond.as_bytes()).unwrap();
        String::from_utf8_lossy(&got).into_owned()
    });
    (url, h)
}

const OK_EMPTY: &str = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}";

#[test]
fn the_request_names_itself_and_carries_the_token_bare_where_asked() {
    let (url, h) = one_request(OK_EMPTY);
    assert_eq!(Live.whoami(&url, "tok", false).0, 200);
    let req = h.join().unwrap().to_ascii_lowercase();
    assert!(req.contains("\r\nuser-agent: homma/"), "{req}");
    assert!(req.contains("\r\nauthorization: tok\r\n"), "{req}");
}

#[test]
fn the_request_carries_a_bearer_where_asked() {
    let (url, h) = one_request(OK_EMPTY);
    Live.whoami(&url, "tok", true);
    let req = h.join().unwrap().to_ascii_lowercase();
    assert!(req.contains("\r\nauthorization: bearer tok\r\n"), "{req}");
    assert!(req.contains("\r\nuser-agent: homma/"), "{req}");
}

#[test]
fn a_status_the_registry_sends_is_the_answer_and_no_server_is_zero() {
    let (url, h) =
        one_request("HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    assert_eq!(Live.whoami(&url, "tok", true).0, 401);
    h.join().unwrap();
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let gone = format!("http://{}/me", l.local_addr().unwrap());
    drop(l);
    assert_eq!(Live.whoami(&gone, "tok", true).0, 0);
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
