//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The built-in groups, and the verbs they hand to the tool of their name.
//!
//! A group is a built-in command that takes verbs (`rules`, `skills`, `release`). It answers its own
//! verbs, and a verb it does not have goes to the tool of the group's name, so `homma rules show` is the
//! tool `rules`'s `show` while `homma rules about` stays the built-in's. Which commands are groups is read
//! off the command line's own shape, so a command that gains verbs is a group from then on and cannot be
//! one that forgot to hand a word on.

use std::ffi::OsString;
use std::path::PathBuf;

use clap::{ArgMatches, CommandFactory, FromArgMatches};

use crate::cli::Cli;

/// The one command that takes verbs and is no group: `homma tools run <name>` is how any tool is named,
/// whatever built-in holds the name, so it hands nothing on.
const OWN: &str = "tools";

/// The names of the built-in groups.
pub fn groups() -> Vec<String> {
    Cli::command()
        .get_subcommands()
        .filter(|c| c.has_subcommands() && c.get_name() != OWN)
        .map(|c| c.get_name().to_string())
        .collect()
}

/// The command line of [`Cli`], with every group also taking a word it has no verb for.
pub fn command() -> clap::Command {
    groups().into_iter().fold(Cli::command(), |cmd, g| {
        cmd.mut_subcommand(g, |s| s.allow_external_subcommands(true))
    })
}

/// A group's word that is not one of its verbs, with the global options that came with it.
#[derive(Debug, PartialEq, Eq)]
pub struct Handed {
    /// The group.
    pub group:     String,
    /// The word and everything after it.
    pub words:     Vec<OsString>,
    /// `--config`.
    pub config:    Option<PathBuf>,
    /// `--dir`.
    pub dir:       Option<PathBuf>,
    /// `-v`, counted.
    pub verbosity: u8,
}

/// What the command line was.
#[derive(Debug)]
pub enum Parsed {
    /// A built-in command, as [`Cli`] has it.
    Cli(Box<Cli>),
    /// A word a group does not know.
    Handed(Handed),
}

/// Parse `args` the way [`Cli`] does, except that a group's unknown word is [`Parsed::Handed`].
///
/// # Errors
///
/// What clap refuses, with the usage it would have printed.
pub fn parse_from<I, T>(args: I) -> Result<Parsed, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let matches = command().try_get_matches_from(args)?;
    if let Some(h) = handed(&matches) {
        return Ok(Parsed::Handed(h));
    }
    Cli::from_arg_matches(&matches).map(|c| Parsed::Cli(Box::new(c)))
}

/// [`parse_from`] over the process's arguments, exiting with clap's own message when they do not parse.
pub fn parse() -> Parsed {
    parse_from(std::env::args_os()).unwrap_or_else(|e| e.exit())
}

fn handed(m: &ArgMatches) -> Option<Handed> {
    let (group, sub) = m.subcommand()?;
    let (word, rest) = sub.subcommand()?;
    let cmd = command();
    let g = cmd.find_subcommand(group)?;
    if !g.is_allow_external_subcommands_set() || g.find_subcommand(word).is_some() {
        return None;
    }
    let mut words = vec![OsString::from(word)];
    words.extend(rest.get_many::<OsString>("").into_iter().flatten().cloned());
    Some(Handed {
        group: group.to_string(),
        words,
        config: m.get_one::<PathBuf>("config").cloned(),
        dir: m.get_one::<PathBuf>("dir").cloned(),
        verbosity: m.get_count("verbosity"),
    })
}
