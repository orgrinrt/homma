use std::cell::RefCell;
use std::path::Path;

use super::cache::Cache;
use super::guard::{self, Guards, Place, Verdict};
use super::{ANSWER_RULES, PARTIAL_RULES, answer, chunks};
use crate::cmd::summary::OneShot;

/// A model that answers a partials call with a section per chunk, naming the chunk's first word at
/// its line 1, and an answering call with `Answer: <its input's first line>`, recording both.
#[derive(Default)]
struct Reader {
    partial_calls: RefCell<Vec<String>>,
    answer_calls:  RefCell<Vec<String>>,
    /// Chunk numbers whose section it leaves out.
    skip:          Vec<usize>,
    /// Chunk numbers whose section it gives with no line under the header.
    empty:         Vec<usize>,
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
            Ok(format!("Answer: {}", content.lines().next().unwrap_or("")))
        }
    }
}

fn cache() -> (tempfile::TempDir, Cache) {
    let t = tempfile::tempdir().unwrap();
    let c = Cache::at(t.path().join("ask"));
    (t, c)
}

#[test]
fn a_deny_rule_refuses_the_command_whole_or_in_part() {
    let deny = vec![Some("rm:*".to_string()), Some("git push*main".to_string())];
    assert_eq!(
        guard::denied(&deny, "ls && rm -rf x"),
        Some("Bash(rm:*)".into())
    );
    assert_eq!(
        guard::denied(&deny, "cat a | git push origin main"),
        Some("Bash(git push*main)".into())
    );
    assert_eq!(
        guard::denied(&deny, "echo ok\nrm -rf x"),
        Some("Bash(rm:*)".into())
    );
    assert_eq!(
        guard::denied(&deny, "sleep 1 & rm -rf x"),
        Some("Bash(rm:*)".into())
    );
    // THE CONTROL: commands no rule matches are not refused, a bare `Bash` refuses every one.
    assert_eq!(guard::denied(&deny, "ls; git push origin dev"), None);
    assert_eq!(guard::denied(&deny, "echo rm"), None);
    assert_eq!(guard::denied(&[None], "ls"), Some("Bash".into()));
    assert_eq!(guard::commands("a && b || c; d | e"), vec![
        "a && b || c; d | e",
        "a",
        "b",
        "c",
        "d",
        "e"
    ]);

    // The rules are read off every settings file given, past one that is not there.
    let t = tempfile::tempdir().unwrap();
    let f = t.path().join("settings.json");
    std::fs::write(
        &f,
        r#"{"permissions":{"deny":["Bash(curl:*)","Read(x)","Bash"],"allow":["Bash(rm:*)"]}}"#,
    )
    .unwrap();
    let g = guard::read(&[t.path().join("absent.json"), f]);
    assert_eq!(g.deny, vec![Some("curl:*".to_string()), None]);
}

fn settings_with_hook(dir: &Path, matcher: &str, hook: &str) -> Guards {
    let f = dir.join("settings.local.json");
    let v = serde_json::json!({
        "hooks": { "PreToolUse": [ { "matcher": matcher, "hooks": [ { "type": "command", "command": hook } ] } ] }
    });
    std::fs::write(&f, v.to_string()).unwrap();
    guard::read(&[f])
}

#[cfg(unix)]
#[test]
fn a_bash_hook_that_blocks_refuses_the_command_and_one_that_allows_does_not() {
    let t = tempfile::tempdir().unwrap();
    let at = Place {
        cwd:     t.path(),
        root:    t.path(),
        session: Some("s1"),
    };
    let seen = t.path().join("payload");
    let blocks = settings_with_hook(
        t.path(),
        "Edit|Bash",
        &format!(
            "cat > {}; echo 'not on this branch' >&2; exit 2",
            seen.display()
        ),
    );
    assert_eq!(blocks.hooks.len(), 1);
    assert_eq!(
        guard::check(&blocks, "git push", &at),
        Verdict::Refused("a hook refused it: not on this branch".into())
    );
    let payload: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&seen).unwrap()).unwrap();
    assert_eq!(payload["tool_name"], "Bash");
    assert_eq!(payload["tool_input"]["command"], "git push");
    assert_eq!(payload["session_id"], "s1");

    let json_deny = settings_with_hook(
        t.path(),
        "",
        r#"cat > /dev/null; echo '{"hookSpecificOutput":{"permissionDecision":"deny","permissionDecisionReason":"read only"}}'"#,
    );
    assert_eq!(
        guard::check(&json_deny, "git push", &at),
        Verdict::Refused("a hook refused it: read only".into())
    );

    // Pretty-printed JSON is read whole, and `ask` refuses, since nobody is there to be asked.
    let pretty = settings_with_hook(
        t.path(),
        ".*",
        "cat > /dev/null; printf '{\\n  \"hookSpecificOutput\": {\\n    \"permissionDecision\": \"ask\",\\n    \"permissionDecisionReason\": \"needs a person\"\\n  }\\n}\\n'",
    );
    assert_eq!(pretty.hooks.len(), 1, "`.*` sees a Bash call");
    assert_eq!(
        guard::check(&pretty, "git push", &at),
        Verdict::Refused("a hook refused it: needs a person".into())
    );
    assert_eq!(
        settings_with_hook(t.path(), "^Bash$", "true").hooks.len(),
        1
    );

    // THE CONTROL: a hook that lets it through, and a blocking one registered for another tool.
    let allows = settings_with_hook(t.path(), "Bash", "cat > /dev/null; exit 0");
    assert!(matches!(
        guard::check(&allows, "git push", &at),
        Verdict::Allowed { .. }
    ));
    let other = settings_with_hook(t.path(), "Edit", "exit 2");
    assert!(other.hooks.is_empty());
    assert!(
        settings_with_hook(t.path(), "Bas", "exit 2")
            .hooks
            .is_empty()
    );
    assert!(matches!(
        guard::check(&other, "git push", &at),
        Verdict::Allowed { .. }
    ));
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
    let run: String = (0 .. 400)
        .map(|i| format!("record {i} of the listing\n"))
        .collect();
    let cut = chunks::cut(&run);
    assert!(cut.len() > 2, "{}", cut.len());
    assert!(cut.iter().all(|c| c.text.len() <= chunks::MAX + 64));
    let shifted = format!("an inserted line\n{run}");
    let after = chunks::cut(&shifted);
    // Only the chunks the insertion landed in change: every other chunk is the same as before.
    let now: Vec<&str> = after.iter().map(|c| c.digest.as_str()).collect();
    let kept = cut
        .iter()
        .filter(|c| now.contains(&c.digest.as_str()))
        .count();
    assert!(kept + 2 >= cut.len(), "{kept} of {} kept", cut.len());
    // THE CONTROL: the insertion did change what it landed in, so the run is not one chunk the
    // check above trivially keeps.
    assert!(kept < cut.len());
    // A line as long as a chunk may be is a chunk of its own.
    let long = format!("short\n{}\nshort\n", "x".repeat(chunks::MAX));
    assert_eq!(spans(&long), vec![(1, 1), (2, 2), (3, 3)]);
}

#[test]
fn a_chunk_keeps_its_partial_wherever_it_moves_in_the_output() {
    let (_t, cache) = cache();
    let model = Reader::default();
    let first = answer(
        &model,
        "m",
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
        "m",
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
    answer(&model, "m", &cache, "q", "alpha one\n\nbeta two\n").unwrap();
    // The same chunks, with one more blank line between them: beta is at line 4 now.
    let again = answer(&model, "m", &cache, "q", "alpha one\n\n\nbeta two\n").unwrap();
    assert!(!again.whole);
    assert_eq!(again.cached, 2);
    assert_eq!(
        model.answer_calls.borrow()[1],
        "- line 1: alpha\n\n- line 4: beta\n"
    );
    // THE CONTROL: the same output again is reused whole.
    let same = answer(&model, "m", &cache, "q", "alpha one\n\n\nbeta two\n").unwrap();
    assert!(same.whole);
}

#[cfg(unix)]
#[test]
fn output_that_is_not_text_is_read_not_dropped() {
    let t = tempfile::tempdir().unwrap();
    let (code, out) =
        super::run_command("printf 'a\\377b\\n'; printf 'warn\\n' >&2", t.path()).unwrap();
    assert_eq!(code, "exit 0");
    assert_eq!(out, "a\u{FFFD}b\n[stderr]\nwarn\n");
    // THE CONTROL: text that is UTF-8 comes through as it is.
    let (_, plain) = super::run_command("printf 'ab\\n'", t.path()).unwrap();
    assert_eq!(plain, "ab\n");
}

#[test]
fn an_unchanged_output_and_question_reuse_the_answer_with_no_call() {
    let (_t, cache) = cache();
    let model = Reader::default();
    let out = "alpha one\n\nbeta two\n";
    let first = answer(&model, "m", &cache, "q", out).unwrap();
    let again = answer(&model, "m", &cache, "q", out).unwrap();
    assert!(again.whole);
    assert_eq!(again.answer, first.answer);
    assert_eq!(model.partial_calls.borrow().len(), 1);
    assert_eq!(model.answer_calls.borrow().len(), 1);

    // THE CONTROL: another question over the same output answers again, from the kept partials;
    // another model keys apart and reads everything again.
    let other = answer(&model, "m", &cache, "another q", out).unwrap();
    assert!(!other.whole);
    assert_eq!(other.cached, 2);
    assert_eq!(model.partial_calls.borrow().len(), 1);
    assert_eq!(model.answer_calls.borrow().len(), 2);
    answer(&model, "other-model", &cache, "q", out).unwrap();
    assert_eq!(model.partial_calls.borrow().len(), 2);
}

#[test]
fn a_changed_chunk_alone_is_read_again() {
    let (_t, cache) = cache();
    let model = Reader::default();
    answer(&model, "m", &cache, "q", "alpha one\n\nbeta two\n").unwrap();
    let changed = answer(&model, "m", &cache, "q", "alpha one\n\nbeta three\n").unwrap();
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
    let err = answer(&model, "m", &cache, "q", "alpha one\n\nbeta two\n").unwrap_err();
    assert!(err.contains("no section for chunk 2 (lines 3-3)"), "{err}");
    assert!(model.answer_calls.borrow().is_empty());

    // A section with a header and no line is as good as none.
    let t2 = tempfile::tempdir().unwrap();
    let fresh = Cache::at(t2.path().join("ask"));
    let hollow = Reader {
        empty: vec![1],
        ..Reader::default()
    };
    let err = answer(&hollow, "m", &fresh, "q", "alpha one\n\nbeta two\n").unwrap_err();
    assert!(err.contains("no section for chunk 1 (lines 1-1)"), "{err}");

    // THE CONTROL: the same model leaving nothing out answers; the kept partial of chunk 1 is used.
    let whole = Reader::default();
    let got = answer(&whole, "m", &cache, "q", "alpha one\n\nbeta two\n").unwrap();
    assert_eq!(got.cached, 1);
    assert_eq!(
        whole.partial_calls.borrow()[0],
        "=== chunk 2 ===\n1\tbeta two\n"
    );
}
