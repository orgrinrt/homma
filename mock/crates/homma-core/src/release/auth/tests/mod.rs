//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The auth tests, by concern: the expiry grading and each registry's answer, the paste, prove and
//! store order, the npmrc, the store, the verbs, and the live request. The doubles they share are here.

use std::cell::RefCell;

use super::flow::{self, Prompt};
use super::store::{Environment, Keychain, comment_date};
use super::*;

mod fix;
mod grade;
mod live;
mod npmrc;
mod store;
mod verbs;

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
