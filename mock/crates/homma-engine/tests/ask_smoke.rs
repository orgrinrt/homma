//! `homma ask` through the binary: a stand-in model answers the partials call and the answering
//! call, the second ask over the same output is answered from the cache, and with `[summary]` off
//! the verb exits 1 saying why.

mod support;

use std::fs;

use predicates::prelude::*;

#[cfg(unix)]
#[test]
fn the_verb_answers_from_a_stand_in_model_and_again_from_the_cache_and_refuses_when_off() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    support::git_in(root, &["init", "-q"]);
    let model = root.join("model");
    let calls = root.join("calls");
    fs::write(
        &model,
        format!(
            "#!/bin/sh\nin=$(cat)\necho call >> {}\ncase \"$in\" in\n  *'=== chunk 1 ==='*) printf '%s\\n' '{{\"is_error\":false,\"result\":\"=== chunk 1 ===\\n- line 1: alpha\"}}' ;;\n  *) printf '%s\\n' '{{\"is_error\":false,\"result\":\"- line 1: alpha\\nAnswer: alpha, line 1\"}}' ;;\nesac\n",
            calls.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&model, fs::Permissions::from_mode(0o755)).unwrap();
    let on = root.join("homma.toml");
    fs::write(
        &on,
        format!(
            "{}\n[summary]\nenabled = true\ncommand = \"{}\"\n",
            support::minimal_config_toml(),
            model.display()
        ),
    )
    .unwrap();
    let ask = || {
        support::bin()
            .current_dir(root)
            .env("HOME", root)
            .args(["--config", on.to_str().unwrap()])
            .args(["ask", "what", "is", "printed"])
            .write_stdin("alpha\n")
            .assert()
    };
    ask()
        .success()
        .stdout(predicate::str::contains("Answer: alpha, line 1"))
        .stdout(predicate::str::contains(
            "1 chunk(s) of input, 0 of them from the cache",
        ));
    assert_eq!(fs::read_to_string(&calls).unwrap().lines().count(), 2);
    ask()
        .success()
        .stdout(predicate::str::contains("Answer: alpha, line 1"))
        .stdout(predicate::str::contains(
            "the whole answer came from the cache",
        ));
    assert_eq!(fs::read_to_string(&calls).unwrap().lines().count(), 2);
    assert!(root.join(".git/homma/ask/answers").is_dir());

    // THE CONTROL: the same ask with the pass off answers nothing and says why.
    let off = root.join("off.toml");
    fs::write(&off, support::minimal_config_toml()).unwrap();
    support::bin()
        .current_dir(root)
        .env("HOME", root)
        .args(["--config", off.to_str().unwrap()])
        .args(["ask", "what", "is", "printed"])
        .write_stdin("alpha\n")
        .assert()
        .failure()
        .stdout("")
        .stderr(predicate::str::contains("the summarising pass is off"));
}
