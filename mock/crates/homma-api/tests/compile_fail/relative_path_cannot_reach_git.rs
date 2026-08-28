//! A relative path must not be able to reach the git contract.
//!
//! Three consecutive rounds shipped a route that walked out of a runtime check
//! of this precondition. It is now the parameter type, and this file is what
//! keeps it there: loosening any `Git` method back to `&Path` makes this
//! compile, and the test fails.

use homma_api::Git;
use std::path::Path;

fn takes_any_git<G: Git>(git: &G) {
    // `Path` is not `AbsPath`, and there is no conversion that does not say
    // what the path is relative to.
    let _ = git.is_repo(Path::new("hands/rel"));
}

fn main() {}
