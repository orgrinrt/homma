//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Skills and rules, over the corpora homma already loads.

use std::path::Path;

use homma_org::rules::Corpus;
use homma_org::skills::Skills;

use super::super::doc::{Doc, Gap, Gathered, Kind};

/// Where the skills are authored, from the workspace root.
pub const SKILLS: &str = ".shared/skills";
/// Where the rules are authored, from the workspace root.
pub const RULES: &str = ".shared/rules";

fn absent(source: &'static str, dir: &str) -> Gathered {
    Gathered {
        docs: Vec::new(),
        gaps: vec![Gap {
            source,
            count: 0,
            what: format!("no {dir} here: {source} not searched"),
        }],
    }
}

pub fn skills(root: &Path) -> Gathered {
    let dir = root.join(SKILLS);
    if !dir.is_dir() {
        return absent("skills", SKILLS);
    }
    match Skills::load(&dir) {
        Ok(s) => {
            Gathered {
                docs: s
                    .skills
                    .iter()
                    .map(|k| {
                        Doc::new(
                            Kind::Skill,
                            &k.name,
                            &format!("the `{}` skill", k.name),
                            &k.meta.description,
                        )
                    })
                    .collect(),
                gaps: Vec::new(),
            }
        },
        Err(e) => {
            Gathered {
                docs: Vec::new(),
                gaps: vec![Gap {
                    source: "skills",
                    count:  1,
                    what:   format!("the skills under {SKILLS} cannot be read: {e}"),
                }],
            }
        },
    }
}

pub fn rules(root: &Path) -> Gathered {
    let dir = root.join(RULES);
    if !dir.is_dir() {
        return absent("rules", RULES);
    }
    match Corpus::load(&dir) {
        Ok(c) => {
            Gathered {
                docs: c
                    .rules
                    .iter()
                    .map(|r| {
                        let mut d = Doc::new(
                            Kind::Rule,
                            &r.name,
                            &format!("homma rules show {}", r.name),
                            &r.meta.fires,
                        );
                        d.tags.clone_from(&r.meta.topics);
                        d
                    })
                    .collect(),
                gaps: Vec::new(),
            }
        },
        Err(e) => {
            Gathered {
                docs: Vec::new(),
                gaps: vec![Gap {
                    source: "rules",
                    count:  1,
                    what:   format!("the rules under {RULES} cannot be read: {e}"),
                }],
            }
        },
    }
}
