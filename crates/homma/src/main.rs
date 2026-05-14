//! `homma` CLI entry point.
//!
//! Parses args, initialises logging, dispatches to a command body under
//! [`cmd`]. Errors are printed to stderr and propagate via process exit.

use std::process::ExitCode;

use clap::Parser;
use tracing_subscriber::{fmt, EnvFilter};

use crate::cli::Cli;

mod cli;
mod cmd;
mod output;

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_logging(cli.verbosity);
    match cmd::run(cli) {
        Ok(cmd::Outcome::Ok) => ExitCode::SUCCESS,
        Ok(cmd::Outcome::ReportedFailure) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

/// Configure `tracing` from the verbosity count.
///
/// `RUST_LOG` overrides when set, so users wanting per-module filters
/// (`RUST_LOG=homma=trace,gix=info`) keep that knob. Otherwise the count
/// flag picks a level: `0` → warn, `1` → info, `2` → debug, `3+` → trace.
fn init_logging(verbosity: u8) {
    let default_level = match verbosity {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_level));
    fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .with_target(verbosity >= 2)
        .compact()
        .init();
}
