//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The paste, prove and store order, and what a store that refuses writes does to it.

use std::cell::Cell;

use super::*;
use crate::release::auth::flow::Ports;

/// A terminal that pastes one token and records what it was told.
struct Pasting {
    token: &'static str,
    said:  RefCell<Vec<String>>,
    /// How many times a token was asked for.
    asked: Cell<usize>,
}

impl Prompt for Pasting {
    fn say(&self, text: &str) {
        self.said.borrow_mut().push(text.into());
    }

    fn secret(&self, _: &str) -> String {
        self.asked.set(self.asked.get() + 1);
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
        asked: Cell::default(),
    };
    let e = entry("jsr").unwrap();
    let st = flow::fix_token(
        e,
        e.token().unwrap(),
        &Ports {
            store:  &store,
            probe:  &Canned(answer),
            prompt: &prompt,
        },
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

fn pasting(token: &'static str) -> Pasting {
    Pasting {
        token,
        said: RefCell::default(),
        asked: Cell::default(),
    }
}

const JSR_VAR: &str = "HOMMA_AUTH_TEST_JSR_UNSET_ELSEWHERE";

/// A machine with no keychain, whose registry token is read from a variable nobody set.
fn no_keychain() -> Environment {
    Environment {
        vars: vec![("jsr".into(), JSR_VAR.into())],
    }
}

fn jsr_accepts() -> Canned {
    Canned((200, Some(serde_json::json!({"name": "someone"}))))
}

/// A store that reads fine, and takes a write only to refuse it once a token is proven.
struct FullDisk;

impl Store for FullDisk {
    fn name(&self) -> String {
        "a full disk".into()
    }

    fn get(&self, _: &str) -> Result<Option<Stored>, Unavailable> {
        Ok(None)
    }

    fn put(&self, _: &str, _: &str, _: Option<Day>) -> Result<(), Unavailable> {
        Err(Unavailable("no space left".into()))
    }

    fn forget(&self, _: &str) -> Result<bool, Unavailable> {
        Ok(false)
    }
}

#[test]
fn a_store_that_refuses_writes_is_found_out_before_a_paste_is_asked_for() {
    let prompt = pasting("pasted-token");
    let e = entry("jsr").unwrap();
    let st = flow::fix_token(
        e,
        e.token().unwrap(),
        &Ports {
            store:  &no_keychain(),
            probe:  &jsr_accepts(),
            prompt: &prompt,
        },
        today(),
        false,
    );
    assert_eq!(
        prompt.asked.get(),
        0,
        "a token was asked for that could not be kept"
    );
    assert_eq!(st.state, State::Refused);
    assert!(st.state.needs_work());
    assert!(st.detail.contains(JSR_VAR), "{}", st.detail);
    let said = prompt.said.borrow().join("\n");
    assert!(said.contains(JSR_VAR), "{said}");
}

#[test]
fn a_walk_over_a_store_that_refuses_writes_does_not_end_all_good() {
    let prompt = pasting("pasted-token");
    let ok = flow::all(
        &ENTRIES[1 .. 2],
        &Ports {
            store:  &no_keychain(),
            probe:  &jsr_accepts(),
            prompt: &prompt,
        },
        today(),
        false,
    );
    assert!(
        !ok,
        "the walk ended good with the token unset and unwritable"
    );
    let said = prompt.said.borrow().join("\n");
    assert!(!said.contains("all are good"), "{said}");
    assert!(said.contains("still not right: jsr"), "{said}");
    assert_eq!(prompt.asked.get(), 0);
}

#[test]
fn fixing_one_over_a_store_that_refuses_writes_needs_work() {
    let prompt = pasting("pasted-token");
    let st = flow::fix(
        entry("jsr").unwrap(),
        &Ports {
            store:  &no_keychain(),
            probe:  &jsr_accepts(),
            prompt: &prompt,
        },
        today(),
        false,
    );
    assert!(st.state.needs_work(), "{:?}", st.state);
}

#[test]
fn a_proven_token_the_store_then_refuses_needs_work() {
    let prompt = pasting("pasted-token");
    let e = entry("jsr").unwrap();
    let st = flow::fix_token(
        e,
        e.token().unwrap(),
        &Ports {
            store:  &FullDisk,
            probe:  &jsr_accepts(),
            prompt: &prompt,
        },
        today(),
        false,
    );
    assert_eq!(
        prompt.asked.get(),
        1,
        "the paste was proven before the store said no"
    );
    assert_eq!(st.state, State::Refused);
    assert!(st.state.needs_work());
    assert!(st.detail.contains("no space left"), "{}", st.detail);
}

#[test]
fn a_store_that_takes_writes_is_asked_for_the_paste_as_before() {
    let prompt = pasting("pasted-token");
    let e = entry("jsr").unwrap();
    let st = flow::fix_token(
        e,
        e.token().unwrap(),
        &Ports {
            store:  &Memory::default(),
            probe:  &jsr_accepts(),
            prompt: &prompt,
        },
        today(),
        false,
    );
    assert_eq!(prompt.asked.get(), 1);
    assert_eq!(st.state, State::Ok);
}
