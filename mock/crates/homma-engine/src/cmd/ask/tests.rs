use std::cell::RefCell;

use super::cache::{Asker, Cache};
use super::{ANSWER_RULES, PARTIAL_RULES, answer, chunks};
use crate::cmd::summary::OneShot;

/// A model that answers a partials call with a section per chunk, naming the chunk's first word at
/// its line 1, and an answering call with its input's first line and `Answer: <that line>`, recording both.
#[derive(Default)]
struct Reader {
    partial_calls: RefCell<Vec<String>>,
    answer_calls:  RefCell<Vec<String>>,
    /// Chunk numbers whose section it leaves out.
    skip:          Vec<usize>,
    /// Chunk numbers whose section it gives with no line under the header.
    empty:         Vec<usize>,
    /// What the answering call says in place of its usual answer, where set.
    says:          Option<String>,
}

impl OneShot for Reader {
    fn ask(&self, system: &str, content: &str) -> Result<String, String> {
        if system == PARTIAL_RULES {
            self.partial_calls.borrow_mut().push(content.to_string());
            let mut out = String::new();
            let mut lines = content.lines().peekable();
            while let Some(line) = lines.next() {
                let Some(n) = line
                    .strip_prefix("=== chunk ")
                    .and_then(|r| r.strip_suffix(" ==="))
                    .and_then(|n| n.parse::<usize>().ok())
                else {
                    continue;
                };
                let first = lines
                    .peek()
                    .and_then(|l| l.split('\t').nth(1))
                    .and_then(|t| t.split_whitespace().next())
                    .unwrap_or("")
                    .to_string();
                if self.empty.contains(&n) {
                    out.push_str(&format!("=== chunk {n} ===\n"));
                } else if !self.skip.contains(&n) {
                    out.push_str(&format!("=== chunk {n} ===\n- line 1: {first}\n"));
                }
            }
            Ok(out)
        } else {
            assert!(system.starts_with(ANSWER_RULES), "{system}");
            self.answer_calls.borrow_mut().push(content.to_string());
            if let Some(s) = &self.says {
                return Ok(s.clone());
            }
            let first = content.lines().next().unwrap_or("");
            Ok(format!("{first}\nAnswer: {first}"))
        }
    }
}

/// The stand-in command asked for `model`.
const fn by(model: &str) -> Asker<'_> {
    Asker {
        command: "claude",
        model,
    }
}

fn cache() -> (tempfile::TempDir, Cache) {
    let t = tempfile::tempdir().unwrap();
    let c = Cache::at(t.path().join("ask"));
    (t, c)
}

/// How many of `before`'s chunks are not among `after`'s, by digest.
fn gone(before: &[chunks::Chunk], after: &[chunks::Chunk]) -> usize {
    before
        .iter()
        .filter(|c| !after.iter().any(|a| a.digest == c.digest))
        .count()
}

#[test]
fn chunks_end_at_blank_and_boundary_lines_and_stay_put_when_text_is_inserted() {
    let spans = |text: &str| -> Vec<(usize, usize)> {
        chunks::cut(text)
            .iter()
            .map(|c| (c.first, c.last))
            .collect()
    };
    assert_eq!(spans("a\nb\n\n\nc\n"), vec![(1, 2), (5, 5)]);
    assert!(chunks::cut("\n \n").is_empty());

    // A run with no blank line: boundary lines cut it, and where depends on the lines alone.
    let lines: Vec<String> = (0 .. 400)
        .map(|i| format!("record {i} of the listing"))
        .collect();
    let run = |at: Option<usize>| -> String {
        let mut l = lines.clone();
        if let Some(at) = at {
            l.insert(at, "an inserted line".to_string());
        }
        l.iter().map(|l| format!("{l}\n")).collect()
    };
    let cut = chunks::cut(&run(None));
    assert!(cut.len() > 2, "{}", cut.len());
    assert!(cut.iter().all(|c| c.text.len() <= chunks::MAX + 64));
    // At the top, in the middle and at the end, the line changes the one chunk it lands in.
    for at in [0, 200, 400] {
        let after = chunks::cut(&run(Some(at)));
        assert_eq!(gone(&cut, &after), 1, "inserted at line {}", at + 1);
    }
    // THE CONTROL: the same run again changes nothing.
    assert_eq!(gone(&cut, &chunks::cut(&run(None))), 0);
}

#[test]
fn a_line_too_long_for_a_chunk_is_cut_into_chunks_on_its_own_number() {
    // Wide characters, so a cut that ignored their boundaries would split one.
    let long = "日".repeat((chunks::MAX * 7 / 2) / 3);
    let text = format!("short\n{long}\nshort\n");
    let cut = chunks::cut(&text);
    let spans: Vec<(usize, usize)> = cut.iter().map(|c| (c.first, c.last)).collect();
    assert_eq!(spans, vec![(1, 1), (2, 2), (2, 2), (2, 2), (2, 2), (3, 3)]);
    assert!(
        cut.iter().all(|c| c.text.len() <= chunks::MAX),
        "no chunk outgrows the cap"
    );
    let rejoined: String = cut[1 .. 5].iter().map(|c| c.text.trim_end()).collect();
    assert_eq!(rejoined, long, "the pieces are the line whole, in order");
    // Every chunk fits a call beside the partial rules.
    let room = crate::cmd::summary::Budget::DEFAULT.part_bytes(PARTIAL_RULES.len());
    assert!(
        cut.iter()
            .enumerate()
            .all(|(i, c)| super::section(i, c).len() <= room)
    );

    // THE CONTROL: a line just under the cap is one chunk.
    let under = "x".repeat(chunks::MAX - 1);
    assert_eq!(chunks::cut(&format!("{under}\n")).len(), 1);
}

#[test]
fn a_chunk_keeps_its_partial_wherever_it_moves_in_the_output() {
    let (_t, cache) = cache();
    let model = Reader::default();
    let first = answer(
        &model,
        by("m"),
        &cache,
        "what is beta",
        "alpha one\n\nbeta two\n",
    )
    .unwrap();
    assert_eq!(first.chunks, 2);
    assert_eq!(first.cached, 0);
    assert_eq!(model.partial_calls.borrow().len(), 1);
    assert_eq!(
        model.answer_calls.borrow()[0],
        "- line 1: alpha\n\n- line 3: beta\n"
    );

    // The same two paragraphs after a new one: only the new one is read, and beta now sits at 5.
    let moved = answer(
        &model,
        by("m"),
        &cache,
        "what is beta",
        "gamma three\n\nalpha one\n\nbeta two\n",
    )
    .unwrap();
    assert_eq!(moved.cached, 2);
    let partials = model.partial_calls.borrow();
    assert_eq!(partials.len(), 2);
    assert_eq!(partials[1], "=== chunk 1 ===\n1\tgamma three\n");
    assert_eq!(
        model.answer_calls.borrow()[1],
        "- line 1: gamma\n\n- line 3: alpha\n\n- line 5: beta\n"
    );
    // Only the place a partial line begins with moves, ranges and `lines` whole; what the line says
    // is left alone, and so is a word that only ends in `line`.
    assert_eq!(
        chunks::moved("- line 2-4, lines 7-8: the pipeline 3 is at line 9\n", 10),
        "- line 12-14, lines 17-18: the pipeline 3 is at line 9\n"
    );
    assert_eq!(chunks::moved("- deadline 3: x\n", 10), "- deadline 3: x\n");
    assert_eq!(
        chunks::moved("not a summary line 3\n", 10),
        "not a summary line 3\n"
    );
    // THE CONTROL: moving a partial by nothing leaves it as it was.
    assert_eq!(chunks::moved("- line 2-4: x\n", 0), "- line 2-4: x\n");
}

#[test]
fn an_answer_is_not_reused_once_its_chunks_sit_on_other_lines() {
    let (_t, cache) = cache();
    let model = Reader::default();
    answer(&model, by("m"), &cache, "q", "alpha one\n\nbeta two\n").unwrap();
    // The same chunks, with one more blank line between them: beta is at line 4 now.
    let again = answer(&model, by("m"), &cache, "q", "alpha one\n\n\nbeta two\n").unwrap();
    assert!(!again.whole);
    assert_eq!(again.cached, 2);
    assert_eq!(
        model.answer_calls.borrow()[1],
        "- line 1: alpha\n\n- line 4: beta\n"
    );
    // THE CONTROL: the same output again is reused whole.
    let same = answer(&model, by("m"), &cache, "q", "alpha one\n\n\nbeta two\n").unwrap();
    assert!(same.whole);
}

#[test]
fn input_that_is_not_text_is_read_not_dropped() {
    let out = super::read_input(&b"a\xffb\nwarn\n"[..]).unwrap();
    assert_eq!(out, "a\u{FFFD}b\nwarn\n");
    // THE CONTROL: text that is UTF-8 comes through as it is.
    assert_eq!(super::read_input(&b"ab\n"[..]).unwrap(), "ab\n");
}

#[test]
fn another_command_for_the_same_model_reads_everything_again() {
    let (_t, cache) = cache();
    let model = Reader::default();
    let out = "alpha one\n\nbeta two\n";
    answer(&model, by("m"), &cache, "q", out).unwrap();
    let other = Asker {
        command: "another-claude",
        model:   "m",
    };
    let again = answer(&model, other, &cache, "q", out).unwrap();
    assert!(!again.whole);
    assert_eq!(again.cached, 0);
    assert_eq!(model.partial_calls.borrow().len(), 2);
    // THE CONTROL: the first command and model again reuse the answer whole.
    assert!(answer(&model, by("m"), &cache, "q", out).unwrap().whole);
}

#[test]
fn an_unchanged_output_and_question_reuse_the_answer_with_no_call() {
    let (_t, cache) = cache();
    let model = Reader::default();
    let out = "alpha one\n\nbeta two\n";
    let first = answer(&model, by("m"), &cache, "q", out).unwrap();
    let again = answer(&model, by("m"), &cache, "q", out).unwrap();
    assert!(again.whole);
    assert_eq!(again.answer, first.answer);
    assert_eq!(model.partial_calls.borrow().len(), 1);
    assert_eq!(model.answer_calls.borrow().len(), 1);

    // THE CONTROL: another question over the same output answers again, from the kept partials;
    // another model keys apart and reads everything again.
    let other = answer(&model, by("m"), &cache, "another q", out).unwrap();
    assert!(!other.whole);
    assert_eq!(other.cached, 2);
    assert_eq!(model.partial_calls.borrow().len(), 1);
    assert_eq!(model.answer_calls.borrow().len(), 2);
    answer(&model, by("other-model"), &cache, "q", out).unwrap();
    assert_eq!(model.partial_calls.borrow().len(), 2);
}

#[test]
fn a_changed_chunk_alone_is_read_again() {
    let (_t, cache) = cache();
    let model = Reader::default();
    answer(&model, by("m"), &cache, "q", "alpha one\n\nbeta two\n").unwrap();
    let changed = answer(&model, by("m"), &cache, "q", "alpha one\n\nbeta three\n").unwrap();
    assert_eq!(changed.cached, 1);
    assert!(!changed.whole);
    assert_eq!(
        model.partial_calls.borrow()[1],
        "=== chunk 2 ===\n1\tbeta three\n"
    );
}

#[test]
fn an_answer_missing_a_chunk_fails_the_ask() {
    let (_t, cache) = cache();
    let model = Reader {
        skip: vec![2],
        ..Reader::default()
    };
    let err = answer(&model, by("m"), &cache, "q", "alpha one\n\nbeta two\n").unwrap_err();
    assert!(err.contains("no section for chunk 2 (lines 3-3)"), "{err}");
    assert!(model.answer_calls.borrow().is_empty());

    // A section with a header and no line is as good as none.
    let t2 = tempfile::tempdir().unwrap();
    let fresh = Cache::at(t2.path().join("ask"));
    let hollow = Reader {
        empty: vec![1],
        ..Reader::default()
    };
    let err = answer(&hollow, by("m"), &fresh, "q", "alpha one\n\nbeta two\n").unwrap_err();
    assert!(err.contains("no section for chunk 1 (lines 1-1)"), "{err}");

    // THE CONTROL: the same model leaving nothing out answers; the kept partial of chunk 1 is used.
    let whole = Reader::default();
    let got = answer(&whole, by("m"), &cache, "q", "alpha one\n\nbeta two\n").unwrap();
    assert_eq!(got.cached, 1);
    assert_eq!(
        whole.partial_calls.borrow()[0],
        "=== chunk 2 ===\n1\tbeta two\n"
    );
}

#[test]
fn an_answer_with_no_summary_or_no_answer_line_fails_and_is_not_kept() {
    let out = "alpha one\n\nbeta two\n";
    for (says, why) in [
        ("  \n", "the answer was empty"),
        ("Answer: alpha", "no summary line"),
        ("- line 1: alpha", "no line starting `Answer:`"),
    ] {
        let (_t, cache) = cache();
        let model = Reader {
            says: Some(says.to_string()),
            ..Reader::default()
        };
        let err = answer(&model, by("m"), &cache, "q", out).unwrap_err();
        assert!(err.contains(why), "{says:?}: {err}");
        // Nothing was kept: asked again, it calls again.
        let _ = answer(&model, by("m"), &cache, "q", out);
        assert_eq!(model.answer_calls.borrow().len(), 2, "{says:?}");
    }

    // THE CONTROL: an answer with both is kept and served from the cache with no call.
    let (_t, cache) = cache();
    let model = Reader {
        says: Some("- line 1: alpha\nAnswer: alpha".to_string()),
        ..Reader::default()
    };
    answer(&model, by("m"), &cache, "q", out).unwrap();
    let again = answer(&model, by("m"), &cache, "q", out).unwrap();
    assert!(again.whole);
    assert_eq!(again.answer, "- line 1: alpha\nAnswer: alpha");
    assert_eq!(model.answer_calls.borrow().len(), 1);
}

#[test]
fn empty_output_makes_no_call() {
    let (_t, cache) = cache();
    let model = Reader::default();
    let err = answer(&model, by("m"), &cache, "q", " \n\n").unwrap_err();
    assert!(err.contains("nothing to answer from"), "{err}");
    assert!(model.partial_calls.borrow().is_empty());
    assert!(model.answer_calls.borrow().is_empty());

    // THE CONTROL: one line of output is read and answered.
    answer(&model, by("m"), &cache, "q", "alpha\n").unwrap();
    assert_eq!(model.partial_calls.borrow().len(), 1);
    assert_eq!(model.answer_calls.borrow().len(), 1);
}

/// A reader the test fails on if anything reads it.
struct Untouched;

impl std::io::Read for Untouched {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        panic!("a terminal was read");
    }
}

#[test]
fn a_terminal_on_stdin_is_refused_unread() {
    assert_eq!(super::read_piped(true, Untouched).unwrap(), None);
    // THE CONTROL: input that is not a terminal is read whole.
    assert_eq!(
        super::read_piped(false, &b"alpha\n"[..]).unwrap(),
        Some("alpha\n".to_string())
    );
}
