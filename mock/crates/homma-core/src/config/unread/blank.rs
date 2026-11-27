//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital

//! The manifest's text with the lines of its unread entries blanked, so a
//! strict parse of what is left names the lines of the text as written.

/// Each line belonging to one of `unread` made empty, every other line kept.
///
/// A line belongs to an entry where it is the header of a table under the
/// entry, or a line inside such a table, or a key whose path starts with the
/// entry, with the lines of its value where that value runs on: an array or an
/// inline table until its brackets close, a string in triple quotes until it
/// does. A shape this does not follow is the caller's to fall back from.
pub(super) fn blank(text: &str, unread: &[String]) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut table: Vec<String> = Vec::new();
    let mut in_unread = false;
    let mut open: Option<Open> = None;
    for line in text.lines() {
        if let Some(o) = open.as_mut() {
            if o.closes_on(line) {
                open = None;
            }
            out.push("");
            continue;
        }
        let t = line.trim_start();
        if t.starts_with('[') {
            table = header(t);
            in_unread = entry(&table, &[]).is_some_and(|e| unread.contains(&e));
            out.push(if in_unread { "" } else { line });
            continue;
        }
        if in_unread {
            out.push("");
            continue;
        }
        if let Some((key, value)) = key_line(t) {
            if entry(&table, &key).is_some_and(|e| unread.contains(&e)) {
                open = Open::after(value);
                out.push("");
                continue;
            }
        }
        out.push(line);
    }
    let mut joined = out.join("\n");
    joined.push('\n');
    joined
}

/// The entry a key path under a table header belongs to: `agent.<name>` under
/// `[agent]`, the first step anywhere else.
fn entry(table: &[String], key: &[String]) -> Option<String> {
    let full: Vec<&String> = table.iter().chain(key).collect();
    match full.as_slice() {
        [] => None,
        [agent, name, ..] if agent.as_str() == "agent" => Some(format!("agent.{name}")),
        [first, ..] => Some((*first).clone()),
    }
}

/// The dotted path of a key, its steps unquoted.
fn steps(text: &str) -> Vec<String> {
    text.split('.')
        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// The path a `[table]` or `[[array]]` header names.
fn header(t: &str) -> Vec<String> {
    let inner = t.trim_start_matches('[');
    steps(inner.split(']').next().unwrap_or(""))
}

/// A `key = value` line's key path and value, where it is one.
fn key_line(t: &str) -> Option<(Vec<String>, &str)> {
    if t.is_empty() || t.starts_with('#') {
        return None;
    }
    let (key, value) = t.split_once('=')?;
    let key = steps(key);
    (!key.is_empty()).then_some((key, value))
}

/// A value still open at the end of its key's line.
enum Open {
    /// Brackets of an array or an inline table, this many deep.
    Depth(i64),
    /// A string in this triple quote.
    Quote(&'static str),
}

impl Open {
    /// What is still open after a key's `value`, or nothing where it closes on
    /// the same line.
    fn after(value: &str) -> Option<Self> {
        let v = value.trim_start();
        for quote in ["\"\"\"", "'''"] {
            if let Some(rest) = v.strip_prefix(quote) {
                return (!rest.contains(quote)).then_some(Open::Quote(quote));
            }
        }
        let depth = depth_of(v);
        (depth > 0).then_some(Open::Depth(depth))
    }

    /// Take in one more line, and whether the value closes on it.
    fn closes_on(&mut self, line: &str) -> bool {
        match self {
            Open::Quote(quote) => line.contains(*quote),
            Open::Depth(depth) => {
                *depth += depth_of(line);
                *depth <= 0
            },
        }
    }
}

/// Brackets opened minus brackets closed on a line, outside of strings.
fn depth_of(line: &str) -> i64 {
    let mut depth = 0;
    let mut quote: Option<char> = None;
    for c in line.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {},
            (None, '"' | '\'') => quote = Some(c),
            (None, '#') => break,
            (None, '[' | '{') => depth += 1,
            (None, ']' | '}') => depth -= 1,
            _ => {},
        }
    }
    depth
}

#[cfg(test)]
mod tests {
    use super::blank;

    #[test]
    fn the_lines_of_an_unread_entry_are_blanked_and_every_other_line_kept() {
        let text = "a = 1\nlater = [\n  1,\n]\n[later.x]\nk = 1\n[[agent.later]]\nk = 2\n[defaults]\nb = 2\n";
        let unread = ["agent.later".to_string(), "later".to_string()];
        assert_eq!(
            blank(text, &unread),
            "a = 1\n\n\n\n\n\n\n\n[defaults]\nb = 2\n",
        );
        // Failing control: nothing unread, nothing blanked.
        assert_eq!(blank(text, &[]), text);
    }

    #[test]
    fn a_key_under_agent_belongs_to_its_entry_and_a_string_in_triple_quotes_runs_on() {
        let text = "[agent]\nlater = \"\"\"\nx [\n\"\"\"\nhooks = 1\n";
        let unread = ["agent.later".to_string()];
        assert_eq!(blank(text, &unread), "[agent]\n\n\n\nhooks = 1\n");
    }
}
