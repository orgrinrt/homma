//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use std::path::Path;

use super::*;

fn parse(text: &str) -> StatusConfig {
    toml::from_str(text).expect("this manifest should parse")
}

fn one(tool: &[&str]) -> Inject {
    Inject {
        tool:   Argv::Many(tool.iter().map(|s| s.to_string()).collect()),
        title:  None,
        format: None,
    }
}

// ---------------------------------------------------------------------------
// the schema
// ---------------------------------------------------------------------------

#[test]
fn a_bare_string_tool_is_a_one_word_command() {
    let cfg = parse(
        r#"
        [[inject]]
        tool = "context"
    "#,
    );
    assert_eq!(cfg.inject[0].tool.words(), ["context"]);
}

#[test]
fn a_list_tool_keeps_its_arguments() {
    let cfg = parse(
        r#"
        [[inject]]
        tool = ["tools/rules/rules", "size"]
    "#,
    );
    assert_eq!(cfg.inject[0].tool.words(), ["tools/rules/rules", "size"]);
}

#[test]
fn the_optional_keys_are_optional() {
    let cfg = parse(
        r#"
        [[inject]]
        tool = "context"
    "#,
    );
    assert!(cfg.inject[0].title.is_none());
    assert!(cfg.inject[0].format.is_none());
}

#[test]
fn the_optional_keys_are_read_when_given() {
    let cfg = parse(
        r#"
        [[inject]]
        tool = "context"
        title = "window"
        format = "head -1"
    "#,
    );
    assert_eq!(cfg.inject[0].title.as_deref(), Some("window"));
    assert_eq!(cfg.inject[0].format.as_deref(), Some("head -1"));
}

#[test]
fn an_unknown_key_is_refused() {
    // The control on the schema being tight at all. Without it every test above
    // would pass against a struct that silently swallowed anything, and a typo
    // in the manifest would read as an entry that simply did nothing. `process`
    // in particular: it was in the shape this came from and is not a key here,
    // so a manifest still carrying one has to say so rather than drop it.
    let err = toml::from_str::<StatusConfig>(
        r#"
        [[inject]]
        tool = "context"
        process = "head -1"
    "#,
    );
    assert!(err.is_err(), "an unknown key must not be swallowed");
}

#[test]
fn no_status_table_at_all_is_no_injections() {
    let cfg: StatusConfig = toml::from_str("").expect("an empty table should parse");
    assert!(cfg.inject.is_empty());
}

#[test]
fn declaration_order_is_kept() {
    // The whole of what the operator asked for: blocks appear in the order the
    // manifest declares them.
    let cfg = parse(
        r#"
        [[inject]]
        tool = "first"
        [[inject]]
        tool = "second"
        [[inject]]
        tool = "third"
    "#,
    );
    let names: Vec<&str> = cfg
        .inject
        .iter()
        .map(|i| i.tool.words()[0].as_str())
        .collect();
    assert_eq!(names, ["first", "second", "third"]);
}

// ---------------------------------------------------------------------------
// anchoring
// ---------------------------------------------------------------------------

#[test]
fn a_relative_path_anchors_at_the_workspace_root() {
    let mut cfg = StatusConfig {
        inject: vec![one(&["tools/rules/rules", "size"])],
    };
    settle(&mut cfg, Path::new("/srv/ws"));
    assert_eq!(cfg.inject[0].tool.words()[0], "/srv/ws/tools/rules/rules");
    assert_eq!(
        cfg.inject[0].tool.words()[1],
        "size",
        "arguments are not touched"
    );
}

#[test]
fn a_bare_program_name_is_left_for_the_path() {
    // The control on the rule above being a rule rather than "prefix
    // everything". A bare name is what PATH is for, and anchoring it would make
    // `git` mean `<root>/git`, which does not exist.
    let mut cfg = StatusConfig {
        inject: vec![one(&["agenda"])],
    };
    settle(&mut cfg, Path::new("/srv/ws"));
    assert_eq!(cfg.inject[0].tool.words()[0], "agenda");
}

#[test]
fn an_absolute_program_path_is_left_alone() {
    let mut cfg = StatusConfig {
        inject: vec![one(&["/usr/local/bin/thing"])],
    };
    settle(&mut cfg, Path::new("/srv/ws"));
    assert_eq!(cfg.inject[0].tool.words()[0], "/usr/local/bin/thing");
}

#[test]
fn anchoring_a_bare_string_tool_works_too() {
    // The `One` variant reaches a different arm of `program_mut`, and a rule
    // that held for lists and not for strings would be invisible: the manifest
    // spelling that needs it most is the short one.
    let mut cfg = StatusConfig {
        inject: vec![Inject {
            tool:   Argv::One("tools/context".into()),
            title:  None,
            format: None,
        }],
    };
    settle(&mut cfg, Path::new("/srv/ws"));
    assert_eq!(cfg.inject[0].tool.words()[0], "/srv/ws/tools/context");
}

#[test]
fn anchoring_twice_lands_in_the_same_place() {
    let mut cfg = StatusConfig {
        inject: vec![one(&["tools/rules/rules"])],
    };
    settle(&mut cfg, Path::new("/srv/ws"));
    settle(&mut cfg, Path::new("/srv/ws"));
    assert_eq!(cfg.inject[0].tool.words()[0], "/srv/ws/tools/rules/rules");
}

#[test]
fn anchoring_an_empty_command_does_nothing_and_does_not_panic() {
    let mut cfg = StatusConfig {
        inject: vec![Inject {
            tool:   Argv::Many(vec![]),
            title:  None,
            format: None,
        }],
    };
    settle(&mut cfg, Path::new("/srv/ws"));
    assert!(cfg.inject[0].tool.words().is_empty());
}

// ---------------------------------------------------------------------------
// running
// ---------------------------------------------------------------------------

fn run(entry: Inject) -> Injected {
    let cfg = StatusConfig {
        inject: vec![entry],
    };
    run_all(&cfg, Path::new("/"))
        .pop()
        .expect("one entry, one block")
}

fn sh(script: &str) -> Inject {
    Inject {
        tool:   Argv::Many(vec!["sh".into(), "-c".into(), script.into()]),
        title:  Some("t".into()),
        format: None,
    }
}

#[test]
fn a_tools_stdout_is_the_block() {
    let got = run(sh("printf 'hello\n'"));
    assert_eq!(got.text, "hello");
    assert!(
        got.failed.is_none(),
        "a tool that worked has no failure: {got:?}"
    );
}

#[test]
fn trailing_blank_lines_come_off() {
    // Homma owns the spacing between blocks, so a tool ending in two newlines
    // must not push the next block down the page.
    let got = run(sh("printf 'hello\n\n\n'"));
    assert_eq!(got.text, "hello");
}

#[test]
fn interior_lines_are_kept_exactly() {
    // The control on the trim above. Trimming the end must not become trimming
    // or reflowing the middle, which is where a tool's own alignment lives.
    let got = run(sh("printf '  a\n\n  b\n'"));
    assert_eq!(got.text, "  a\n\n  b");
}

#[test]
fn a_tool_that_prints_nothing_is_empty_rather_than_broken() {
    let got = run(sh("true"));
    assert_eq!(got.text, "");
    assert!(got.failed.is_none(), "printing nothing is not a failure");
}

#[test]
fn the_format_pipeline_transforms_the_output() {
    let mut entry = sh("printf 'a\nb\nc\n'");
    entry.format = Some("tr a-z A-Z".into());
    assert_eq!(run(entry).text, "A\nB\nC");
}

#[test]
fn a_format_that_reads_none_of_its_input_neither_hangs_nor_fails() {
    // `head -1` closes the pipe after one line and the write into it returns
    // EPIPE. That is the ordinary case, not a fault, and treating the write's
    // error as one would make the most obvious format anybody writes report a
    // failure. The input is long enough that the write cannot fit in the pipe
    // buffer, which is what makes the error actually happen.
    let mut entry = sh("seq 1 200000");
    entry.format = Some("head -1".into());
    let got = run(entry);
    assert_eq!(got.text, "1");
    assert!(
        got.failed.is_none(),
        "EPIPE from a short reader is not a failure: {got:?}"
    );
}

#[test]
fn a_missing_program_is_a_failure_that_names_it() {
    let got = run(sh("true").tool_replaced(&["definitely-not-a-real-program-xyz"]));
    let why = got.failed.expect("a missing program is a failure");
    assert!(
        why.contains("definitely-not-a-real-program-xyz"),
        "the message must name what could not run: {why}"
    );
    assert_eq!(got.text, "");
}

#[test]
fn a_non_zero_exit_is_a_failure_carrying_the_code() {
    let got = run(sh("exit 3"));
    let why = got.failed.expect("a non-zero exit is a failure");
    assert!(
        why.contains('3'),
        "the message must carry the exit code: {why}"
    );
}

#[test]
fn the_tools_own_stderr_says_why() {
    let got = run(sh("printf 'the registry is missing\n' >&2; exit 1"));
    let why = got.failed.expect("a non-zero exit is a failure");
    assert!(
        why.contains("the registry is missing"),
        "the tool's own account is what explains it: {why}"
    );
}

#[test]
fn only_the_first_stderr_line_is_carried() {
    // A status block is not the place for a backtrace.
    let got = run(sh("printf 'first\nsecond\nthird\n' >&2; exit 1"));
    let why = got.failed.expect("a non-zero exit is a failure");
    assert!(why.contains("first"), "{why}");
    assert!(
        !why.contains("second"),
        "the rest stays where the operator can run it: {why}"
    );
}

#[test]
fn a_signal_is_reported_as_one_rather_than_as_a_code() {
    // `status.code()` is `None` there, and formatting `None` as a number is how
    // a killed tool comes out claiming to have exited 0.
    let got = run(sh("kill -9 $$"));
    let why = got.failed.expect("being killed is a failure");
    assert!(why.contains("signal"), "{why}");
}

#[test]
fn a_failing_format_is_reported_as_the_format_failing() {
    // Not as the tool failing. The tool worked, and pointing at it sends
    // whoever reads this to debug a program that did its job.
    let mut entry = sh("printf 'fine\n'");
    entry.format = Some("exit 4".into());
    let got = run(entry);
    let why = got.failed.expect("a failing format is a failure");
    assert!(
        why.contains("format"),
        "the message must say which half broke: {why}"
    );
    assert!(why.contains('4'), "{why}");
}

#[test]
fn an_empty_command_says_so() {
    let got = run(Inject {
        tool:   Argv::Many(vec![]),
        title:  None,
        format: None,
    });
    let why = got
        .failed
        .expect("declaring an injection that runs nothing is a failure");
    assert!(why.contains("no command"), "{why}");
}

#[test]
fn a_tool_reading_stdin_gets_end_of_input_rather_than_the_terminal() {
    // stdin is null. Without that, a tool that reads stdin blocks on whatever
    // the operator's terminal is attached to and `homma status` hangs with no
    // sign of why.
    let got = run(sh("cat"));
    assert_eq!(got.text, "");
    assert!(got.failed.is_none(), "{got:?}");
}

#[test]
fn every_block_comes_back_in_declaration_order() {
    let cfg = StatusConfig {
        inject: vec![sh("printf 'one\n'"), sh("printf 'two\n'"), sh("printf 'three\n'")],
    };
    let got = run_all(&cfg, Path::new("/"));
    let texts: Vec<&str> = got.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(texts, ["one", "two", "three"]);
}

#[test]
fn one_failing_block_does_not_stop_the_others() {
    // `homma status` is the cheapest sanity check in the workspace. A foreign
    // script exiting non-zero is one of the things worth finding out from it,
    // and it is not a reason to refuse to print the rest.
    let cfg = StatusConfig {
        inject: vec![sh("exit 1"), sh("printf 'still here\n'")],
    };
    let got = run_all(&cfg, Path::new("/"));
    assert!(got[0].failed.is_some());
    assert_eq!(got[1].text, "still here");
    assert!(got[1].failed.is_none());
}

#[test]
fn the_tool_runs_in_the_directory_it_was_given() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let cfg = StatusConfig {
        inject: vec![sh("pwd")],
    };
    let got = run_all(&cfg, dir.path());
    // Through `canonicalize` because the temp root is a symlink on macOS and
    // `pwd` prints the resolved side of it.
    let want = dir.path().canonicalize().expect("the temp dir resolves");
    assert_eq!(got[0].text, want.display().to_string());
}

#[test]
fn the_format_runs_in_that_directory_too() {
    // Otherwise a format naming a file beside the tool works when run by hand
    // and not through homma.
    let dir = tempfile::tempdir().expect("a temp dir");
    std::fs::write(dir.path().join("marker"), "found\n").expect("write the marker");
    let mut entry = sh("printf 'ignored\n'");
    entry.format = Some("cat marker".into());
    let cfg = StatusConfig {
        inject: vec![entry],
    };
    assert_eq!(run_all(&cfg, dir.path())[0].text, "found");
}

// ---------------------------------------------------------------------------
// titles
// ---------------------------------------------------------------------------

#[test]
fn a_block_with_no_title_takes_the_programs_file_name() {
    let got = run(Inject {
        tool:   Argv::One("/srv/ws/tools/self-compact/commands/context".into()),
        title:  None,
        format: None,
    });
    assert_eq!(got.title, "context");
}

#[test]
fn a_bare_program_name_is_its_own_title() {
    let got = run(Inject {
        tool:   Argv::One("agenda".into()),
        title:  None,
        format: None,
    });
    assert_eq!(got.title, "agenda");
}

#[test]
fn a_given_title_wins_over_the_derived_one() {
    let got = run(Inject {
        tool:   Argv::One("/srv/ws/tools/rules/rules".into()),
        title:  Some("the rule corpus".into()),
        format: None,
    });
    assert_eq!(got.title, "the rule corpus");
}

#[test]
fn a_block_that_could_not_run_is_still_titled() {
    // The title is what says which entry this was, so it is exactly the case
    // where a failing block needs one.
    let got = run(Inject {
        tool:   Argv::One("definitely-not-a-real-program-xyz".into()),
        title:  None,
        format: None,
    });
    assert_eq!(got.title, "definitely-not-a-real-program-xyz");
    assert!(got.failed.is_some());
}

// ---------------------------------------------------------------------------

impl Inject {
    /// Same entry with a different command, for the cases that want one.
    fn tool_replaced(mut self, words: &[&str]) -> Self {
        self.tool = Argv::Many(words.iter().map(|s| s.to_string()).collect());
        self
    }
}

#[test]
fn a_format_that_reads_all_of_a_large_input_does_not_deadlock() {
    // The pipes are finite in both directions, and this is the case where that
    // matters. Writing the input from the calling thread blocks until the child
    // accepts all of it, and nothing drains the child's stdout until
    // `wait_with_output` runs, which is afterwards. So once the child has
    // written a pipe buffer's worth it stops reading, this side is still
    // writing, and neither moves again.
    //
    // It held for small inputs and only for small inputs, which is why the
    // other forty tests here never saw it: none of them feeds more than a few
    // hundred bytes. A kilobyte came back fine and a megabyte sat until it was
    // killed.
    //
    // `cat` is the whole point of the arm: it reads everything and writes
    // everything, so both buffers fill. `head -1` above cannot reach this,
    // because it closes the pipe rather than filling it.
    let mut entry = sh("seq 1 200000");
    entry.format = Some("cat".into());
    let got = run(entry);

    // Well past any pipe buffer, so a passing run means the two directions
    // really were concurrent.
    assert!(
        got.text.len() > 1_000_000,
        "expected the whole of it back, got {} bytes",
        got.text.len()
    );
    assert!(
        got.text.starts_with('1'),
        "the start is missing: {:?}",
        &got.text[.. 20.min(got.text.len())]
    );
    assert!(got.text.ends_with("200000"), "the end is missing");
    assert!(
        got.failed.is_none(),
        "a large input is not a failure: {got:?}"
    );
}

#[test]
fn a_large_input_survives_a_format_that_transforms_it() {
    // The control for the test above. A passing size assertion could be a
    // pipeline that echoed something long back without reading its input, so
    // this one makes the output depend on the input having arrived whole.
    let mut entry = sh("seq 1 200000");
    entry.format = Some("wc -l".into());
    let got = run(entry);
    assert_eq!(
        got.text.trim(),
        "200000",
        "the reader did not see every line: {got:?}"
    );
    assert!(got.failed.is_none(), "{got:?}");
}
