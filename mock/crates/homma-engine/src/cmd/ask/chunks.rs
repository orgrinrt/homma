//! A command's output cut into chunks by content, each with its line span and the digest of its
//! text, and a partial's lines moved to where its chunk sits.
//!
//! A chunk ends after a line followed by a blank line, after a boundary line once it holds
//! [`MIN`] bytes, or after the line that takes it to [`MAX`]. A boundary line is one whose digest
//! has its low four bits zero, so where chunks end depends on the lines and not on where they
//! sit: text inserted into the output moves only the chunks it lands in.

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
        // A line as long as a chunk may be is a chunk alone.
        if line.len() >= MAX {
            if let Some((first, text)) = open.take() {
                out.push(close(first, i, text));
            }
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

/// `partial` with every `line <n>` and `line <a>-<b>` in it moved on by `by` lines.
#[must_use]
pub fn moved(partial: &str, by: usize) -> String {
    let mut out = String::with_capacity(partial.len());
    let mut rest = partial;
    while let Some(i) = rest.find("line ") {
        let (before, after) = rest.split_at(i + "line ".len());
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
