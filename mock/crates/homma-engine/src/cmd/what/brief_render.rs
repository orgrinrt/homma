//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A brief as it is printed, and its fitting within [`BYTES`].

use std::fmt::Write as _;

use super::brief::{BYTES, Brief, QUOTES};

/// What a cut text ends with.
const CUT: &str = " (cut)";

/// `s` shortened by at least `by` bytes at a character boundary, marked as cut, or emptied.
fn shorten(s: &mut String, by: usize) {
    let want = s.len().saturating_sub(by + CUT.len());
    if want == 0 {
        s.clear();
        return;
    }
    let mut at = want;
    while !s.is_char_boundary(at) {
        at -= 1;
    }
    s.truncate(at);
    s.push_str(CUT);
}

/// The brief within [`BYTES`] printed: the last things left out first, then the one thing's
/// quotes from its last, then its reason and the note cut.
pub(super) fn fit(mut b: Brief) -> Brief {
    let over = |b: &Brief| b.render().len().saturating_sub(BYTES);
    while over(&b) > 0 && b.things.len() > 1 {
        b.things.pop();
        b.left += 1;
    }
    while over(&b) > 0 && b.things.first().is_some_and(|t| !t.quotes.is_empty()) {
        b.things[0].quotes.pop();
        b.dropped += 1;
    }
    let by = over(&b);
    if by > 0 {
        if let Some(why) = b.things.first_mut().and_then(|t| t.why.as_mut()) {
            shorten(why, by);
        }
    }
    let by = over(&b);
    if by > 0 {
        if let Some(note) = b.note.as_mut() {
            shorten(note, by);
        }
    }
    b
}

impl Brief {
    /// The brief as a reader reads it: each thing with its command, why, and its quoted lines, then
    /// a line for each thing left out, cut or given from the cache.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        for t in &self.things {
            let _ = writeln!(out, "{} {}: {}", t.kind.label(), t.name, t.command);
            if let Some(w) = &t.why {
                let _ = writeln!(out, "  {w}");
            }
            for q in &t.quotes {
                let _ = writeln!(out, "  \"{q}\"");
            }
        }
        if self.left > 0 {
            let _ = writeln!(
                out,
                "{} more thing(s) bear on the work and were left out to keep the brief within {BYTES} \
                 bytes; `homma what <words>` lists them, and a smaller `--limit` gives the rest more room",
                self.left
            );
        }
        if self.dropped > 0 {
            let _ = writeln!(
                out,
                "{} line(s) of the answer were left out: a quote not a whole line of its source, a \
                 line with no place, no quote or a second reason, or a quote past a thing's {QUOTES}",
                self.dropped
            );
        }
        if let Some(n) = self.cut {
            let _ = writeln!(
                out,
                "the work's text was cut after its line {n} to fit the pass"
            );
        }
        if let Some(n) = &self.note {
            let _ = writeln!(out, "no extract: {n}");
        }
        if self.cached {
            let _ = writeln!(out, "the brief came from the cache");
        }
        out.trim_end().to_string()
    }
}

impl crate::output::HumanRender for Brief {
    fn render_human(&self, out: &mut dyn std::io::Write) -> std::io::Result<()> {
        if self.things.is_empty() {
            return write!(
                out,
                "nothing the workspace holds bears on this work by its words"
            );
        }
        write!(out, "{}", self.render())
    }
}
