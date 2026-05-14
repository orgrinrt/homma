//! `homma migrate`: stub. Substantive behaviour lands in #452.
//!
//! The shape is reserved here so the CLI surface is stable between #451
//! and #452: `homma migrate <repo> --to <forge>`. The body fails fast
//! with a "not yet implemented" diagnostic.

use anyhow::{bail, Result};

use crate::cli::OutputFormat;

pub fn run(repo: &str, to: &str, _format: OutputFormat) -> Result<()> {
    bail!(
        "migrate is not yet implemented (task #452); would migrate `{repo}` to forge `{to}`"
    )
}
