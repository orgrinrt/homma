//! An unknown home stops homma rather than silently shortening the deny list.
//!
//! **Its own test binary, and that is the whole reason it exists as a file.**
//! `HOME` is process-global, so mutating it inside the unit-test binary races
//! every sibling that reads it, and this crate has one that does. An integration
//! test is its own process, which makes the race unexpressible rather than
//! merely avoided.
//!
//! What it pins: `Denied::from_env` returned an **empty list** when `HOME` was
//! absent or relative, which removed two of the three deny items outright, under
//! a doc comment asserting that was "not a licence". Both cases were reproduced
//! writing a workspace into a `.claude` at exit 0. The effect of a forbidden
//! write succeeding is a licence, whatever the comment above it says.

use homma_api::{Denied, NoHome};

/// Both cases in one test, because each needs the variable to itself and two
/// tests in one binary run in parallel by default.
#[test]
fn a_home_that_cannot_be_determined_is_refused() {
    // SAFETY: this binary contains exactly this one test, so nothing else in the
    // process reads or writes `HOME` while it runs. The claim is checkable: if a
    // second test is ever added to this file, it is wrong, which is why the file
    // says so at the top rather than only here.
    unsafe {
        std::env::remove_var("HOME");
    }
    assert_eq!(
        Denied::from_env().unwrap_err(),
        NoHome::Unset,
        "an absent HOME must stop homma, not shorten the list"
    );

    unsafe {
        std::env::set_var("HOME", "relative-home");
    }
    match Denied::from_env() {
        Err(NoHome::Relative(v)) => assert_eq!(v, "relative-home"),
        other => panic!("a relative HOME must be refused and named, got {other:?}"),
    }

    // And the message says what to do, because the operator seeing it has an
    // environment problem rather than a homma problem.
    let msg = NoHome::Unset.to_string();
    assert!(msg.contains("absolute"), "{msg}");
    assert!(
        msg.contains("permit exactly the writes they forbid"),
        "the message has to say what running without it would allow: {msg}"
    );

    unsafe {
        std::env::set_var("HOME", "/tmp");
    }
    assert!(
        Denied::from_env().is_ok(),
        "an absolute HOME is the ordinary case"
    );
}
