//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The tools under `tools/`, read the way `homma tools` reads them.

use std::path::Path;

use super::super::doc::{Doc, Gap, Gathered, Kind, Verb};
use crate::cmd::tools::descriptor::{Entry, discover};

pub fn gather(root: &Path) -> Gathered {
    let mut docs = Vec::new();
    let mut broken = 0;
    for e in discover(root) {
        match e {
            Entry::Tool(t) => {
                let head = if t.promote {
                    format!("homma {}", t.name)
                } else {
                    format!("homma tools run {}", t.name)
                };
                let mut d = Doc::new(Kind::Tool, &t.name, &head, &t.summary);
                d.tags.clone_from(&t.tags);
                d.verbs = t
                    .commands
                    .iter()
                    .map(|c| {
                        Verb {
                            name:    c.name.clone(),
                            summary: c.summary.clone(),
                            command: if c.name == t.name {
                                head.clone()
                            } else {
                                format!("{head} {}", c.name)
                            },
                        }
                    })
                    .collect();
                docs.push(d);
            },
            Entry::Broken(_) => broken += 1,
        }
    }
    let mut gaps = Vec::new();
    if broken > 0 {
        gaps.push(Gap {
            source: "tools",
            count:  broken,
            what:   format!("{broken} tool.toml cannot be read (`homma tools check` names them)"),
        });
    }
    Gathered {
        docs,
        gaps,
    }
}
