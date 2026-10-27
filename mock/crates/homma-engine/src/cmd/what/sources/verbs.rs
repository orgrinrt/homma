//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! homma's own subcommands, read from clap's description of the command line.

use clap::{Command, CommandFactory};

use super::super::doc::{Doc, Gathered, Kind, Verb};
use crate::cli::Cli;

fn about(c: &Command) -> String {
    c.get_about()
        .map(|s| s.to_string().lines().next().unwrap_or("").to_string())
        .unwrap_or_default()
}

pub fn gather() -> Gathered {
    let cli = Cli::command();
    let mut docs = Vec::new();
    for c in cli.get_subcommands() {
        let name = c.get_name();
        if name == "help" || c.is_hide_set() {
            continue;
        }
        let mut d = Doc::new(Kind::Verb, name, &format!("homma {name}"), &about(c));
        for s in c.get_subcommands() {
            if s.get_name() == "help" || s.is_hide_set() {
                continue;
            }
            d.verbs.push(Verb {
                name:    s.get_name().to_string(),
                summary: about(s),
                command: format!("homma {name} {}", s.get_name()),
            });
        }
        docs.push(d);
    }
    Gathered {
        docs,
        gaps: Vec::new(),
    }
}
