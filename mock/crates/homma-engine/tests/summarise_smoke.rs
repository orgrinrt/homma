//! `homma summarise` through the binary: with `[summary]` on, the text goes to the configured
//! command and its summary is printed; with it off, the text is printed whole and the reason goes
//! to stderr, and both exit 0.

mod support;

use std::fs;

use predicates::prelude::*;

#[cfg(unix)]
#[test]
fn the_verb_prints_the_summary_with_the_pass_on_and_the_text_whole_with_it_off() {
    let dir = tempfile::tempdir().unwrap();
    let model = dir.path().join("model");
    homma_fixture::script(
        &model,
        "#!/bin/sh\ncat > /dev/null\nprintf '%s\\n' '{\"is_error\":false,\"result\":\"- line 2: the second line\"}'\n",
        0o755,
    );
    let text = dir.path().join("text.txt");
    fs::write(&text, "one\ntwo\n").unwrap();

    let on = dir.path().join("on.toml");
    fs::write(
        &on,
        format!(
            "{}\n[summary]\nenabled = true\ncommand = \"{}\"\n",
            support::minimal_config_toml(),
            model.display()
        ),
    )
    .unwrap();
    support::bin()
        .args(["--config", on.to_str().unwrap()])
        .args(["summarise", "--brief", "what is on line two"])
        .arg(&text)
        .assert()
        .success()
        .stdout("- line 2: the second line\n");

    // THE CONTROL: the same manifest without the table prints the text whole and says why.
    let off = dir.path().join("off.toml");
    fs::write(&off, support::minimal_config_toml()).unwrap();
    support::bin()
        .args(["--config", off.to_str().unwrap()])
        .args(["summarise", "--brief", "what is on line two"])
        .arg(&text)
        .assert()
        .success()
        .stdout("one\ntwo\n")
        .stderr(predicate::str::contains("summary: off"));
}
