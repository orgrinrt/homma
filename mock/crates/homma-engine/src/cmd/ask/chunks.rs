//! A command's output cut into chunks by content, each with its line span and the digest of its
//! text, and a partial's lines moved to where its chunk sits.
//!
//! A chunk ends after a line followed by a blank line, after a boundary line once it holds
//! [`MIN`] bytes, or after the line that takes it to [`MAX`]. A boundary line is one whose digest
//! has its low four bits zero, so where chunks end depends on the lines and not on where they
//! sit: text inserted into the output moves only the chunks it lands in.
//!
//! A line of [`MAX`] bytes or more is cut into pieces of fewer than [`MAX`] bytes on character
//! boundaries, by the summarising pass's own cut, each a chunk alone on the line's number.

use crate::cmd::summary::parts::cut_at_chars;

/// The size a chunk holds before a boundary line may end it.
pub const MIN: usize = 512;
/// The size past which a chunk ends whatever its lines, the one cut by position.
pub const MAX: usize = 8_192;

/// One chunk of the output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub text:   String,
    /// Its first and last line in the output, one-based.
    pub first:  usize,
    pub last:   usize,
    /// The first sixteen hex digits of the BLAKE3 digest of its text.
    pub digest: String,
}

/// `output` cut into chunks by content. Blank lines belong to no chunk.
#[must_use]
pub fn cut(output: &str) -> Vec<Chunk> {
    let lines: Vec<&str> = output.lines().collect();
    let mut out = Vec::new();
    let mut open: Option<(usize, String)> = None;
    for (i, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        // A line as long as a chunk may be is cut into pieces, each a chunk alone on its number, so
        // no chunk outgrows a call.
        if line.len() >= MAX {
            if let Some((first, text)) = open.take() {
                out.push(close(first, i, text));
            }
            for piece in cut_at_chars(line, MAX - 1) {
                out.push(close(i + 1, i + 1, format!("{piece}\n")));
            }
            continue;
        }
        let (_, text) = open.get_or_insert_with(|| (i + 1, String::new()));
        text.push_str(line);
        text.push('\n');
        let blank_next = lines.get(i + 1).is_none_or(|l| l.trim().is_empty());
        let boundary = text.len() >= MIN && blake3::hash(line.as_bytes()).as_bytes()[0] & 0x0F == 0;
        if blank_next || boundary || text.len() >= MAX {
            if let Some((first, text)) = open.take() {
                out.push(close(first, i + 1, text));
            }
        }
    }
    out
}

/// The chunk holding `text`, from line `first` to line `last`.
fn close(first: usize, last: usize, text: String) -> Chunk {
    Chunk {
        digest: blake3::hash(text.as_bytes()).to_hex()[.. 16].to_string(),
        text,
        first,
        last,
    }
}

/// The chunk's lines numbered from 1, each `<n>\t<line>`, as a partial is made from.
#[must_use]
pub fn numbered(text: &str) -> String {
    text.lines()
        .enumerate()
        .map(|(i, l)| format!("{}\t{l}\n", i + 1))
        .collect()
}

/// `partial` with every `line <n>`, `line <a>-<b>`, `lines <n>` and `lines <a>-<b>` in the place
/// each of its lines begins with, after the `- ` and before the first `: `, moved on by `by`
/// lines. What a line says after its place is left as it is.
#[must_use]
pub fn moved(partial: &str, by: usize) -> String {
    partial
        .split_inclusive('\n')
        .map(|line| {
            let Some(at) = line.find("- ").filter(|at| line[.. *at].trim().is_empty()) else {
                return line.to_string();
            };
            let start = at + 2;
            let end = line[start ..].find(": ").map_or(line.len(), |i| start + i);
            format!(
                "{}{}{}",
                &line[.. start],
                moved_place(&line[start .. end], by),
                &line[end ..]
            )
        })
        .collect()
}

/// One place with its line numbers moved: a `line` or `lines` that is a whole word, then a space,
/// then a number or a range.
fn moved_place(place: &str, by: usize) -> String {
    let mut out = String::with_capacity(place.len());
    let mut rest = place;
    while let Some((i, word)) = next_line_word(rest) {
        let (before, after) = rest.split_at(i + word.len());
        out.push_str(before);
        rest = after;
        let mut first = true;
        loop {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            if digits == 0 {
                break;
            }
            let (n, after) = rest.split_at(digits);
            match n.parse::<usize>() {
                Ok(n) => out.push_str(&(n + by).to_string()),
                Err(_) => out.push_str(n),
            }
            rest = after;
            match rest.strip_prefix('-') {
                Some(after) if first && after.starts_with(|c: char| c.is_ascii_digit()) => {
                    out.push('-');
                    rest = after;
                    first = false;
                },
                _ => break,
            }
        }
    }
    out.push_str(rest);
    out
}

/// Where the next `line ` or `lines ` that starts a word is, and which of the two it is.
fn next_line_word(text: &str) -> Option<(usize, &'static str)> {
    let mut from = 0;
    while let Some(i) = text[from ..].find("line") {
        let at = from + i;
        let starts_word = text[.. at]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric());
        let word = if text[at ..].starts_with("lines ") {
            Some("lines ")
        } else if text[at ..].starts_with("line ") {
            Some("line ")
        } else {
            None
        };
        if let (true, Some(w)) = (starts_word, word) {
            return Some((at, w));
        }
        from = at + "line".len();
    }
    None
}
