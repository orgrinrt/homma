//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Reading the descriptors of the workspace's own tools.
//!
//! A tool is a directory `tools/<name>/` holding a `tool.toml` in the format renki defines for a host to
//! read, and homma is that host. The reading and the checking of a descriptor are renki's
//! ([`Descriptor::parse`]), so what renki refuses is broken here for the same reason and in its words.
//! Two checks are added that renki does not make, since only a host with a directory per tool has them: the
//! descriptor's name is its directory's, and a verb is named once. Nothing is fetched and nothing runs:
//! reading a descriptor is the whole of listing a tool.

use std::path::PathBuf;

use renki::extension::Descriptor;
use serde::Serialize;

/// Where the tools sit, relative to the workspace root.
pub const TOOLS_DIR: &str = "tools";
/// The descriptor's file name.
pub const DESCRIPTOR: &str = "tool.toml";

/// A directory with a descriptor that cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Broken {
    /// The directory's name, which is the name the tool would have.
    pub name: String,
    /// The descriptor.
    pub file: PathBuf,
    /// Why it was refused.
    pub why:  String,
}

/// What `discover` found in one directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// A usable tool, as renki read it.
    Tool(Descriptor),
    /// A descriptor that is not.
    Broken(Broken),
}

impl Entry {
    /// The name, which is the directory's.
    pub fn name(&self) -> &str {
        match self {
            Self::Tool(d) => &d.name,
            Self::Broken(b) => &b.name,
        }
    }
}

/// Read one descriptor. `dir_name` is the name of the directory it sat in.
///
/// # Errors
///
/// With the reason, when renki refuses the text, or the descriptor names a tool other than the
/// directory's, or a verb twice.
pub fn parse(dir_name: &str, text: &str) -> Result<Descriptor, String> {
    let d = Descriptor::parse(text)?;
    if d.name != dir_name {
        return Err(format!(
            "its name is `{}` and the directory's is `{dir_name}`; they are the same name",
            d.name
        ));
    }
    for (i, c) in d.commands.iter().enumerate() {
        if d.commands[.. i].iter().any(|seen| seen.name == c.name) {
            return Err(format!("the command `{}` is given twice", c.name));
        }
    }
    Ok(d)
}

/// A reason on one line, since it is printed after a name and a path.
fn one_line(why: &str) -> String {
    why.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Every directory under `<root>/tools` that holds a `tool.toml`, as a tool or as the reason it is not,
/// sorted by name. A directory without a descriptor is not a tool and is not listed. No tools directory
/// is no tools.
pub fn discover(root: &std::path::Path) -> Vec<Entry> {
    let base = root.join(TOOLS_DIR);
    let Ok(read) = std::fs::read_dir(&base) else {
        return Vec::new();
    };
    let mut out: Vec<Entry> = Vec::new();
    for e in read.flatten() {
        let dir = e.path();
        let file = dir.join(DESCRIPTOR);
        if !dir.is_dir() || !file.is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        let broken = |why: String| {
            Entry::Broken(Broken {
                name: name.clone(),
                file: file.clone(),
                why:  one_line(&why),
            })
        };
        out.push(match std::fs::read_to_string(&file) {
            Err(e) => broken(format!("could not be read: {e}")),
            Ok(text) => {
                match parse(&name, &text) {
                    Ok(d) => Entry::Tool(d),
                    Err(why) => broken(why),
                }
            },
        });
    }
    out.sort_by(|a, b| a.name().cmp(b.name()));
    out
}
