use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::{GRACE, Stopped, run, spawn};

fn sh(script: &str) -> Command {
    let mut c = Command::new("sh");
    c.args(["-c", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    c
}

/// The pid a script wrote to `path`, waiting up to five seconds for it to be written whole.
#[cfg(target_os = "linux")]
fn pid_in(path: &std::path::Path) -> String {
    for _ in 0 .. 100 {
        if let Some(s) = std::fs::read_to_string(path)
            .ok()
            .filter(|s| s.ends_with('\n'))
        {
            return s.trim().to_string();
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("no pid was written to {}", path.display());
}

/// Whether process `pid` is gone, or dead and waiting only to be reaped, within a second, read off
/// `/proc`.
#[cfg(target_os = "linux")]
fn ended(pid: &str) -> bool {
    let stat = std::path::Path::new("/proc").join(pid).join("stat");
    for _ in 0 .. 20 {
        match std::fs::read_to_string(&stat) {
            Err(_) => return true,
            // The state follows the name, which is in parentheses and may hold spaces.
            Ok(s) if s.rsplit_once(") ").is_some_and(|(_, r)| r.starts_with('Z')) => return true,
            Ok(_) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    false
}

#[cfg(unix)]
#[test]
fn a_child_past_its_limit_is_ended_with_what_it_started() {
    let dir = tempfile::tempdir().unwrap();
    let pid = dir.path().join("pid");
    let script = format!("sleep 30 &\necho $! > {}\nwait\n", pid.display());
    let begun = Instant::now();
    let got = run(spawn(&mut sh(&script)).unwrap(), "", Duration::from_secs(1));
    assert!(matches!(got, Err(Stopped::RanPast)), "{got:?}");
    assert!(begun.elapsed() < Duration::from_secs(10));
    #[cfg(target_os = "linux")]
    {
        let pid = pid_in(&pid);
        assert!(ended(&pid), "the process it started, {pid}, still runs");
    }

    // control: a child within its limit runs to its end and is not cut short
    let got = run(
        spawn(&mut sh("sleep 0.2; echo done")).unwrap(),
        "",
        Duration::from_secs(10),
    )
    .unwrap();
    assert!(got.status.success());
    assert_eq!(got.stdout, b"done\n");
}

#[cfg(unix)]
#[test]
fn a_pipe_held_past_the_childs_exit_gets_the_grace_and_its_holder_is_ended() {
    let dir = tempfile::tempdir().unwrap();
    let pid = dir.path().join("pid");
    // The background sleep inherits stdout, so the pipe stays open after the shell exits.
    let script = format!(
        "sleep 30 &\necho $! > {}\necho out\necho err >&2\n",
        pid.display()
    );
    let begun = Instant::now();
    let got = run(
        spawn(&mut sh(&script)).unwrap(),
        "",
        Duration::from_secs(20),
    )
    .unwrap();
    let took = begun.elapsed();
    assert!(got.status.success());
    assert_eq!(got.stdout, b"out\n", "what arrived is kept");
    assert_eq!(got.stderr, b"err\n");
    assert!(took >= GRACE, "the pipe had its grace: {took:?}");
    assert!(took < Duration::from_secs(10), "and no more: {took:?}");
    #[cfg(target_os = "linux")]
    {
        let pid = pid_in(&pid);
        assert!(ended(&pid), "the holder, {pid}, still runs");
    }

    // control: a process left running with its pipes closed is not waited on
    let script = "sleep 5 > /dev/null 2>&1 &\necho out\n";
    let begun = Instant::now();
    let got = run(spawn(&mut sh(script)).unwrap(), "", Duration::from_secs(20)).unwrap();
    assert_eq!(got.stdout, b"out\n");
    assert!(
        begun.elapsed() < Duration::from_secs(3),
        "{:?}",
        begun.elapsed()
    );
}

#[cfg(unix)]
#[test]
fn a_child_exiting_just_under_its_limit_still_gets_the_whole_grace() {
    // The shell exits with about half a second of its two left; what it started writes six tenths
    // of a second later, past the limit and inside the grace.
    let script = "sleep 1.5\n(sleep 0.6; echo tail) &\n";
    let got = run(spawn(&mut sh(script)).unwrap(), "", Duration::from_secs(2)).unwrap();
    assert!(got.status.success());
    assert_eq!(got.stdout, b"tail\n", "the output past the limit is kept");

    // control: the same output written after the grace is lost
    let script = "sleep 1.5\n(sleep 2; echo tail) &\n";
    let got = run(spawn(&mut sh(script)).unwrap(), "", Duration::from_secs(2)).unwrap();
    assert!(
        got.stdout.is_empty(),
        "{:?}",
        String::from_utf8_lossy(&got.stdout)
    );
}

#[test]
fn the_reason_names_the_limit_as_given() {
    let says = |limit| Stopped::RanPast.reason(limit);
    assert_eq!(
        says(Duration::from_millis(600)),
        "it ran past 600 ms and was ended"
    );
    assert_eq!(
        says(Duration::from_secs(300)),
        "it ran past 300 s and was ended"
    );
    assert_eq!(
        says(Duration::from_millis(1500)),
        "it ran past 1.5 s and was ended"
    );
}

#[cfg(unix)]
#[test]
fn input_larger_than_a_pipe_holds_reaches_a_child_that_echoes_it() {
    let input = "0123456789abcdef\n".repeat(64 * 1024);
    let got = run(
        spawn(&mut sh("cat")).unwrap(),
        &input,
        Duration::from_secs(30),
    )
    .unwrap();
    assert!(got.status.success());
    assert_eq!(got.stdout.len(), input.len());
    assert_eq!(got.stdout, input.as_bytes());

    // control: a child that reads nothing exits on its own and the write does not hold the run
    let got = run(
        spawn(&mut sh("exit 3")).unwrap(),
        &input,
        Duration::from_secs(30),
    )
    .unwrap();
    assert_eq!(got.status.code(), Some(3));
}
