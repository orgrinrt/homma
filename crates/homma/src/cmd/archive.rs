//! `homma archive`: stub. Substantive behaviour lands in #452.

use anyhow::{bail, Result};

use crate::cli::OutputFormat;

pub fn run(repo: &str, from: Option<&str>, _format: OutputFormat) -> Result<()> {
    let from = from.unwrap_or("<repo's configured forge>");
    bail!(
        "archive is not yet implemented (task #452); would archive `{repo}` on forge `{from}`"
    )
}
