use std::cell::RefCell;

use homma_api::SummaryConfig;

use super::parts::cut;
use super::{Budget, OneShot, Pass, RULES, pass, placeless, rules_version, summarise};

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
        placeless("- a.toml:key.sub: x\n- line 3-5: y\nprose is not a line"),
        0
    );
    assert_eq!(rules_version().len(), 16);
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

#[cfg(unix)]
#[test]
fn the_command_is_run_isolated_with_the_content_on_stdin() {
    use std::os::unix::fs::PermissionsExt;

    use super::call::Command;

    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("log");
    let script = dir.path().join("model");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\npwd > {log}.dir\nls -A > {log}.ls\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done > {log}.args\ncat > {log}.stdin\nprintf '%s\\n' '{{\"type\":\"result\",\"is_error\":false,\"result\":\"- line 1: hello\"}}'\n",
            log = log.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let cmd = Command {
        program: script.display().to_string(),
        model:   "haiku".into(),
        limit:   super::call::LIMIT,
    };
    assert_eq!(
        cmd.ask("SYSTEM", "1\thello\n"),
        Ok("- line 1: hello".into())
    );
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
    assert_eq!(after("--system-prompt"), "SYSTEM");
    assert_eq!(
        std::fs::read_to_string(log.with_extension("stdin")).unwrap(),
        "1\thello\n"
    );
    // a fresh empty directory, not the caller's
    let ran_in = std::fs::read_to_string(log.with_extension("dir")).unwrap();
    assert_ne!(
        ran_in.trim(),
        std::env::current_dir().unwrap().display().to_string()
    );
    assert_eq!(
        std::fs::read_to_string(log.with_extension("ls")).unwrap(),
        ""
    );

    // control: an error result is a failure with its words
    std::fs::write(
        &script,
        "#!/bin/sh\ncat > /dev/null\nprintf '%s\\n' '{\"is_error\":true,\"result\":\"Not logged in\"}'\n",
    )
    .unwrap();
    assert_eq!(cmd.ask("S", "x"), Err("Not logged in".into()));
    // and a run past its limit is ended
    std::fs::write(&script, "#!/bin/sh\nsleep 5\n").unwrap();
    let slow = Command {
        limit: std::time::Duration::from_millis(200),
        ..cmd
    };
    assert!(slow.ask("S", "x").unwrap_err().contains("ran past"));
}
