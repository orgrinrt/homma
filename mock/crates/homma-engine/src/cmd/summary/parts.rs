//! The content of a call: every line with its number, cut at line boundaries into parts that each
//! fit one request.

/// One part: the numbered lines it holds and the first and last of their numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub first: usize,
    pub last:  usize,
    /// The lines, each `<n>\t<line>\n`.
    pub body:  String,
}

/// `text` numbered from 1 and cut into parts of at most `bytes` each, at line boundaries. A line
/// that does not fit a part alone is cut itself, each piece carrying the line's number. Text with no
/// lines is one empty part, so a call still says there is nothing.
#[must_use]
pub fn cut(text: &str, bytes: usize) -> Vec<Part> {
    let bytes = bytes.max(1);
    let mut out: Vec<Part> = Vec::new();
    let mut open: Option<Part> = None;
    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        for piece in pieces(n, line, bytes) {
            let fits = open
                .as_ref()
                .is_some_and(|p| p.body.len() + piece.len() <= bytes);
            if !fits {
                out.extend(open.take());
            }
            let part = open.get_or_insert_with(|| {
                Part {
                    first: n,
                    last:  n,
                    body:  String::new(),
                }
            });
            part.last = n;
            part.body.push_str(&piece);
        }
    }
    out.extend(open);
    if out.is_empty() {
        out.push(Part {
            first: 0,
            last:  0,
            body:  String::new(),
        });
    }
    out
}

/// Line `n` as numbered lines of at most `bytes` each, cut at character boundaries where it is
/// longer, every piece keeping at least one character.
fn pieces(n: usize, line: &str, bytes: usize) -> Vec<String> {
    let head = format!("{n}\t");
    let whole = format!("{head}{line}\n");
    if whole.len() <= bytes {
        return vec![whole];
    }
    let room = bytes.saturating_sub(head.len() + 1).max(1);
    cut_at_chars(line, room)
        .into_iter()
        .map(|piece| format!("{head}{piece}\n"))
        .collect()
}

/// `text` in pieces of at most `bytes` each, every one ending on a character boundary and keeping
/// at least one character, so a character wider than `bytes` is a piece alone. Empty text is no
/// piece.
#[must_use]
pub fn cut_at_chars(text: &str, bytes: usize) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let mut at = bytes.min(rest.len());
        while !rest.is_char_boundary(at) {
            at -= 1;
        }
        if at == 0 {
            at = rest.chars().next().map_or(rest.len(), char::len_utf8);
        }
        let (piece, after) = rest.split_at(at);
        out.push(piece);
        rest = after;
    }
    out
}
