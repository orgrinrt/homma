//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Writing one text as one YAML scalar.
//!
//! A description is prose that other people edit, and the harness reads it back
//! through a YAML reader of its own that this crate cannot ask. So the rule is
//! conservative and does not depend on which YAML version reads it: a text is
//! written plain only when it is plain words, and double quoted otherwise.

/// What a plain scalar may hold besides letters, digits and a space.
const PLAIN_PUNCTUATION: &str = ",.-_/'()";
/// Words a YAML reader of either version takes for a value rather than text.
const RESERVED: [&str; 12] =
    ["true", "false", "null", "yes", "no", "on", "off", "y", "n", "~", "nan", "inf"];

/// Whether `value` reads back as the same text when written without quotes.
///
/// It starts with an ASCII letter, holds only letters, digits, spaces and
/// [`PLAIN_PUNCTUATION`], does not end in a space, and is not a reserved word or
/// a number. Everything the check does not name is quoted, so a character the
/// check never thought of is safe by default.
fn is_plain(value: &str) -> bool {
    value.starts_with(|c: char| c.is_ascii_alphabetic())
        && !value.ends_with(' ')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == ' ' || PLAIN_PUNCTUATION.contains(c))
        && !RESERVED.contains(&value.to_ascii_lowercase().as_str())
        && value.parse::<f64>().is_err()
}

/// `value` as one YAML scalar: plain when [`is_plain`], double quoted otherwise.
pub fn scalar(value: &str) -> String {
    if is_plain(value) {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prose_that_a_plain_scalar_would_misread_is_quoted() {
        for (given, written) in [
            (
                "the coordinator: lands branches",
                "\"the coordinator: lands branches\"",
            ),
            (
                "lands branches # and reads",
                "\"lands branches # and reads\"",
            ),
            ("[bracketed] role", "\"[bracketed] role\""),
            ("\"quoted\" at the start", "\"\\\"quoted\\\" at the start\""),
            ("a: \"b\" \\ c", "\"a: \\\"b\\\" \\\\ c\""),
            ("two\nlines", "\"two\\nlines\""),
            ("ends with a colon:", "\"ends with a colon:\""),
            (" padded ", "\" padded \""),
            ("ends with a space ", "\"ends with a space \""),
            ("true", "\"true\""),
            ("12", "\"12\""),
            ("", "\"\""),
            // Each of these reads back as another type under at least one YAML
            // version, or as nothing, and the old check let them through plain.
            ("0x1F", "\"0x1F\""),
            (".inf", "\".inf\""),
            ("2026-10-30", "\"2026-10-30\""),
            ("Y", "\"Y\""),
            ("n", "\"n\""),
            ("NaN", "\"NaN\""),
            ("with a bell \u{7}", "\"with a bell \\x07\""),
            ("a colon:without a space", "\"a colon:without a space\""),
            ("issue#12 is open", "\"issue#12 is open\""),
            (
                "says \"hi\" and \\ leaves",
                "\"says \\\"hi\\\" and \\\\ leaves\"",
            ),
            ("1 starts with a digit", "\"1 starts with a digit\""),
            ("kääntäjä", "\"kääntäjä\""),
        ] {
            assert_eq!(scalar(given), written, "{given:?}");
        }
    }

    #[test]
    fn prose_that_reads_back_the_same_stays_plain() {
        for plain in [
            "the kaski coordinator, who dispatches helpers",
            "Paja, who works on tooling.",
            "keeps a file-size limit of 500 lines",
            "reads rules/skills (and the canon)",
            "Op's right hand",
            "yes and no",
            "ends with a digit 1",
        ] {
            assert_eq!(scalar(plain), plain);
        }
    }

    /// The law the whitelist exists for: nothing it writes plain contains a
    /// character with a meaning of its own to a YAML reader.
    #[test]
    fn nothing_written_plain_holds_a_character_that_means_something() {
        for c in (0u32 .. 0x250).filter_map(char::from_u32) {
            let given = format!("word{c}word");
            let written = scalar(&given);
            if written == given {
                assert!(
                    c.is_ascii_alphanumeric() || c == ' ' || PLAIN_PUNCTUATION.contains(c),
                    "{c:?} came out plain: {written}"
                );
            } else {
                assert!(
                    written.starts_with('"') && written.ends_with('"'),
                    "{written}"
                );
            }
        }
    }
}
