//! Output helpers shared across command bodies.
//!
//! Every command produces a typed result struct that implements both
//! [`Serialize`] (for JSON mode) and a [`Display`]-style "render to a human
//! writer" method (for the default terminal mode). [`emit`] dispatches on
//! the chosen [`OutputFormat`] and writes one document to stdout.
//!
//! Errors are emitted on stderr by `main` regardless of format; the JSON
//! contract is "one success document per command, or none."

use std::io::Write;

use serde::Serialize;

use crate::cli::OutputFormat;

/// A command-result value that knows how to render itself in human form.
///
/// JSON form is provided automatically via the [`Serialize`] supertrait.
pub trait HumanRender {
    /// Write a human-readable representation to `out`. Implementors should
    /// not add a trailing newline; [`emit`] handles that.
    fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()>;
}

/// Emit a command result to stdout in the chosen format.
pub fn emit<T: Serialize + HumanRender>(value: &T, format: OutputFormat) -> std::io::Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    match format {
        OutputFormat::Human => {
            value.render_human(&mut out)?;
            // A trailing newline keeps shells from concatenating prompts.
            writeln!(out)?;
        }
        OutputFormat::Json => {
            let s = serde_json::to_string_pretty(value)
                .map_err(std::io::Error::other)?;
            writeln!(out, "{s}")?;
        }
    }
    Ok(())
}
