use std::cell::RefCell;

use homma_api::SummaryConfig;

use super::parts::cut;
use super::{Budget, OneShot, Pass, RULES, checked, pass, placeless, rules_version, summarise};

/// A model that records what it was handed and answers each call from a list, or fails.
struct Recorded {
    calls:   RefCell<Vec<(String, String)>>,
    answers: RefCell<Vec<Result<String, String>>>,
}

impl Recorded {
    fn answering(answers: Vec<Result<String, String>>) -> Self {
        Self {
            calls:   RefCell::new(Vec::new()),
            answers: RefCell::new(answers),
        }
    }
}

impl OneShot for Recorded {
    fn ask(&self, system: &str, content: &str) -> Result<String, String> {
        self.calls
            .borrow_mut()
            .push((system.to_string(), content.to_string()));
        let mut a = self.answers.borrow_mut();
        if a.is_empty() { Err("no answer left".into()) } else { a.remove(0) }
    }
}

const TINY: Budget = Budget {
    system:  1_000,
    request: 1_000,
    reserve: 0,
};

#[test]
fn the_pass_off_gives_the_output_whole_and_the_line_saying_so() {
    let off = pass(&SummaryConfig::default(), "brief", "a\nb\n");
    assert_eq!(off.summary, None);
    let note = off.note.unwrap();
    assert!(note.contains("off") && note.contains("[summary]"), "{note}");

    // control: turned on with a command that is not there, it is a failed call, not "off"
    let on = SummaryConfig {
        enabled: true,
        command: "/nonexistent/homma-summary-command".into(),
        ..SummaryConfig::default()
    };
    let failed = pass(&on, "brief", "a\n");
    assert_eq!(failed.summary, None);
    assert!(failed.note.unwrap().contains("did not start"));
}

#[test]
fn a_failed_call_gives_the_output_whole_and_its_reason() {
    let model = Recorded::answering(vec![Err("rate limited".into())]);
    let got = summarise(&model, &Budget::DEFAULT, "brief", "a\n");
    assert_eq!(got, Pass {
        summary: None,
        note:    Some(
            "summary: the call for part 1 of 1 failed: rate limited; the output is whole".into()
        ),
    });

    // control: the same call answering gives a summary
    let model = Recorded::answering(vec![Ok("- line 1: a".into())]);
    let got = summarise(&model, &Budget::DEFAULT, "brief", "a\n");
    assert_eq!(got.summary.as_deref(), Some("- line 1: a"));
    assert_eq!(got.note, None);
}

#[test]
fn content_over_one_request_is_cut_at_lines_into_parts_each_read_alone() {
    let brief = "b";
    let system = RULES.len() + brief.len() + 1;
    let budget = Budget {
        system:  10_000,
        // room for exactly two numbered lines of `xxxxxxxx` (`1\txxxxxxxx\n` is 11 bytes)
        request: (system + 22).div_ceil(super::BYTES_PER_TOKEN),
        reserve: 0,
    };
    let room = budget.part_bytes(system);
    assert!((22 .. 25).contains(&room), "{room}");
    let text = "xxxxxxxx\nxxxxxxxx\nxxxxxxxx\n";
    let model = Recorded::answering(vec![Ok("- line 1: x".into()), Ok("- line 3: x".into())]);
    let got = summarise(&model, &budget, brief, text);
    let calls = model.calls.borrow();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].1, "1\txxxxxxxx\n2\txxxxxxxx\n");
    assert_eq!(calls[1].1, "3\txxxxxxxx\n");
    assert_eq!(
        got.summary.as_deref(),
        Some("part 1 of 2, lines 1-2\n- line 1: x\n\npart 2 of 2, lines 3-3\n- line 3: x")
    );

    // control: the same text one call wide is one part
    let parts = cut(text, 33);
    assert_eq!(parts.len(), 1);
    // and a byte narrower than two lines is three
    assert_eq!(cut(text, 21).len(), 3);
    // a line longer than a part is cut, each piece keeping its number
    let long = cut("abcdefghij\n", 6);
    assert!(
        long.iter()
            .all(|p| p.body.starts_with("1\t") && p.body.len() <= 6)
    );
    assert_eq!(long.iter().map(|p| p.body.len() - 3).sum::<usize>(), 10);
}

#[test]
fn every_line_goes_with_its_number_and_a_line_with_no_place_is_counted() {
    let model = Recorded::answering(vec![Ok(
        "- src/a.rs:12: one\n- line 2: two\n- something with no place\n- line two: no number"
            .into(),
    )]);
    let got = summarise(&model, &Budget::DEFAULT, "what is where", "one\ntwo\n");
    let calls = model.calls.borrow();
    assert_eq!(calls[0].1, "1\tone\n2\ttwo\n");
    assert!(calls[0].0.starts_with(RULES) && calls[0].0.ends_with("what is where\n"));
    let summary = got.summary.unwrap();
    assert!(
        summary.ends_with("2 summary line(s) above name no place; open the output for those"),
        "{summary}"
    );

    // control: every line with a place adds nothing
    assert_eq!(
        placeless("- a.toml:key.sub: x\n- line 3-5: y\n- enabled: a key\nprose is not a line"),
        0
    );
}

#[test]
fn a_brief_over_the_budget_is_not_sent() {
    let model = Recorded::answering(vec![Ok("- line 1: a".into())]);
    let long = "w".repeat(TINY.system * super::BYTES_PER_TOKEN);
    let got = summarise(&model, &TINY, &long, "a\n");
    assert!(model.calls.borrow().is_empty());
    assert!(got.note.unwrap().contains("nothing was sent"));

    // control: a short brief under the same budget is sent
    let got = summarise(&model, &TINY, "short", "a\n");
    assert_eq!(model.calls.borrow().len(), 1);
    assert!(got.summary.is_some());
}

#[test]
fn an_answer_with_no_summary_line_is_a_failed_call() {
    for (said, why) in [
        ("   \n", "the answer was empty"),
        ("Here is a summary of the text.", "no summary line"),
    ] {
        let model = Recorded::answering(vec![Ok(said.into())]);
        let got = summarise(&model, &Budget::DEFAULT, "brief", "a\n");
        assert_eq!(got.summary, None, "{said:?}");
        let note = got.note.unwrap();
        assert!(
            note.contains("part 1 of 1 failed") && note.contains(why),
            "{note}"
        );
    }
    assert_eq!(checked("  - line 1: a  \n"), Ok("- line 1: a".into()));

    // control: an answer with one `- ` line among prose is a summary
    let model = Recorded::answering(vec![Ok("Summary:\n- line 1: a".into())]);
    let got = summarise(&model, &Budget::DEFAULT, "brief", "a\n");
    assert_eq!(got.summary.as_deref(), Some("Summary:\n- line 1: a"));
}

#[test]
fn empty_text_makes_no_call() {
    let model = Recorded::answering(vec![Ok("- line 1: a".into())]);
    let got = summarise(&model, &Budget::DEFAULT, "brief", " \n\n\t\n");
    assert!(model.calls.borrow().is_empty());
    assert_eq!(got.summary, None);
    assert!(got.note.unwrap().contains("nothing to summarise"));

    // control: one character of text is sent
    let got = summarise(&model, &Budget::DEFAULT, "brief", "a");
    assert_eq!(model.calls.borrow().len(), 1);
    assert!(got.summary.is_some());
}

#[test]
fn a_failing_later_part_fails_the_pass_after_the_earlier_ones_were_read() {
    let budget = Budget {
        system:  10_000,
        request: (RULES.len() + 2 + 11).div_ceil(super::BYTES_PER_TOKEN),
        reserve: 0,
    };
    let text = "xxxxxxxx\nxxxxxxxx\nxxxxxxxx\n";
    let model = Recorded::answering(vec![
        Ok("- line 1: x".into()),
        Err("overloaded".into()),
        Ok("- line 3: x".into()),
    ]);
    let got = summarise(&model, &budget, "b", text);
    assert_eq!(
        model.calls.borrow().len(),
        2,
        "it stops at the part that failed"
    );
    assert_eq!(got.summary, None);
    assert_eq!(
        got.note.as_deref(),
        Some("summary: the call for part 2 of 3 failed: overloaded; the output is whole")
    );

    // control: the same three parts all answering give a summary of three parts
    let model = Recorded::answering(vec![
        Ok("- line 1: x".into()),
        Ok("- line 2: x".into()),
        Ok("- line 3: x".into()),
    ]);
    let got = summarise(&model, &budget, "b", text).summary.unwrap();
    assert!(got.contains("part 3 of 3, lines 3-3"), "{got}");
}

#[test]
fn a_long_line_of_wide_characters_is_cut_at_character_boundaries() {
    // `ä` is two bytes: a part of 9 holds `1\t`, three of them and the newline.
    let parts = cut("ääääää\n", 9);
    let bodies: Vec<&str> = parts.iter().map(|p| p.body.as_str()).collect();
    assert_eq!(bodies, vec!["1\täää\n", "1\täää\n"]);
    // A part too narrow for even one character still carries one, rather than none or a split one.
    let narrow = cut("€€\n", 3);
    let bodies: Vec<&str> = narrow.iter().map(|p| p.body.as_str()).collect();
    assert_eq!(bodies, vec!["1\t€\n", "1\t€\n"]);

    // control: the same line in a part wide enough is whole
    assert_eq!(cut("ääääää\n", 64)[0].body, "1\tääääää\n");
}

#[test]
fn the_rules_version_is_pinned_to_the_rules_as_they_stand() {
    // An edit of the rules fails this until the pin moves, so no summary kept under the old rules
    // is taken for one under the new.
    assert_eq!(rules_version(), "d1a87ce1b71847dd");
}

#[cfg(unix)]
fn stand_in(dir: &std::path::Path, body: &str) -> super::call::Command {
    let script = dir.join("model");
    homma_fixture::script(&script, &format!("#!/bin/sh\n{body}"), 0o755);
    super::call::Command {
        program: script.display().to_string(),
        model:   "haiku".into(),
        limit:   super::call::LIMIT,
    }
}

#[cfg(unix)]
#[test]
fn the_command_is_run_isolated_with_the_brief_in_a_file_and_the_content_on_stdin() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("log");
    let cmd = stand_in(
        dir.path(),
        &format!(
            "pwd > {log}.dir\nls -A > {log}.ls\nprev=\nfor a in \"$@\"; do printf '%s\\n' \"$a\" >> {log}.args; if [ \"$prev\" = --system-prompt-file ]; then cat \"$a\" > {log}.prompt; printf '%s' \"$a\" > {log}.where; fi; prev=$a; done\ncat > {log}.stdin\nprintf '%s\\n' '{{\"type\":\"result\",\"is_error\":false,\"result\":\"- line 1: hello\"}}'\n",
            log = log.display()
        ),
    );
    let brief = "SYSTEM PROMPT TEXT";
    assert_eq!(cmd.ask(brief, "1\thello\n"), Ok("- line 1: hello".into()));
    let args = std::fs::read_to_string(log.with_extension("args")).unwrap();
    let args: Vec<&str> = args.lines().collect();
    for flag in [
        "-p",
        "--safe-mode",
        "--strict-mcp-config",
        "--disable-slash-commands",
        "--no-session-persistence",
    ] {
        assert!(args.contains(&flag), "{flag} missing from {args:?}");
    }
    let after = |f: &str| args[args.iter().position(|a| *a == f).unwrap() + 1];
    assert_eq!(after("--model"), "haiku");
    assert_eq!(after("--tools"), "");
    assert_eq!(after("--setting-sources"), "");
    // the brief is in the file the arguments name, and nowhere in the arguments
    assert!(!args.contains(&"--system-prompt"));
    assert!(args.iter().all(|a| !a.contains(brief)), "{args:?}");
    assert_eq!(
        std::fs::read_to_string(log.with_extension("prompt")).unwrap(),
        brief
    );
    assert_eq!(
        std::fs::read_to_string(log.with_extension("stdin")).unwrap(),
        "1\thello\n"
    );
    // a fresh empty directory, not the caller's, and the prompt's file outside it
    let ran_in = std::fs::read_to_string(log.with_extension("dir")).unwrap();
    let ran_in = std::path::Path::new(ran_in.trim());
    assert_ne!(ran_in, std::env::current_dir().unwrap());
    assert_eq!(
        std::fs::read_to_string(log.with_extension("ls")).unwrap(),
        ""
    );
    let file = std::fs::read_to_string(log.with_extension("where")).unwrap();
    assert!(!std::path::Path::new(&file).starts_with(ran_in), "{file}");
}

#[cfg(unix)]
#[test]
fn each_way_a_call_fails_gives_its_reason() {
    let dir = tempfile::tempdir().unwrap();
    let answers = |body: &str| stand_in(dir.path(), body).ask("S", "x");

    // an error result is a failure with its words
    assert_eq!(
        answers(
            "cat > /dev/null\nprintf '%s\\n' '{\"is_error\":true,\"result\":\"Not logged in\"}'\n"
        ),
        Err("Not logged in".into())
    );
    // a non-zero exit with no result gives the first line of its stderr
    assert_eq!(
        answers(
            "cat > /dev/null\necho 'out' \necho '' >&2\necho '  bad flag: --x  ' >&2\necho more >&2\nexit 3\n"
        ),
        Err("bad flag: --x".into())
    );
    // and with nothing on stderr, the first line of stdout
    assert_eq!(
        answers("cat > /dev/null\necho 'only out'\nexit 1\n"),
        Err("only out".into())
    );
    // an exit 0 with no JSON line
    assert_eq!(
        answers("cat > /dev/null\necho 'plain words'\n"),
        Err("the command printed no JSON result".into())
    );
    // a reason longer than the cap is cut to it
    let long = "w".repeat(super::call::REASON + 50);
    let got = answers(&format!("cat > /dev/null\necho '{long}' >&2\nexit 1\n")).unwrap_err();
    assert_eq!(got.chars().count(), super::call::REASON);
    // control: a short reason is whole, and a JSON result with exit 0 answers
    assert_eq!(
        answers("cat > /dev/null\necho 'short' >&2\nexit 1\n"),
        Err("short".into())
    );
    assert_eq!(
        answers(
            "cat > /dev/null\nprintf '%s\\n' '{\"is_error\":false,\"result\":\"- line 1: x\"}'\n"
        ),
        Ok("- line 1: x".into())
    );
    // a run past its limit is ended
    let slow = super::call::Command {
        limit: std::time::Duration::from_millis(200),
        ..stand_in(dir.path(), "sleep 5\n")
    };
    assert!(slow.ask("S", "x").unwrap_err().contains("ran past"));
}
