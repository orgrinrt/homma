//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A registry request trusts the roots `SSL_CERT_FILE` names, and says so when it names none that can be read.
//!
//! A cloud container sends every request through a proxy whose authority is a bundle at that path, and a
//! request that trusts only a compiled-in store is refused as an unknown issuer. The test serves a local TLS
//! endpoint with a certificate made for it, and asks it three times from a child process: with the file
//! naming that certificate, without the variable, and with the variable naming a file that is not there. The
//! roots are read when a connection is first made and the environment is the process's, so each ask is a
//! process of its own rather than a variable set in a test thread.
//!
//! The refusal without the file is asserted as `invalid peer certificate`, which is how the library reports a
//! self-signed leaf no root vouches for, and not as the unknown-issuer variant the topic of the round that
//! added this asked for. The first ask and the second use the same certificate and the same address, so a name
//! mismatch or an expired certificate cannot make one pass and the other fail.

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Set in the child: the url it asks.
const URL_VAR: &str = "HOMMA_TLS_TEST_URL";
/// What the child prints before its answer.
const ANSWER: &str = "HOMMA_TLS_ANSWER:";
/// What the child writes on standard error when no root could be read.
const NO_ROOTS: &str = "no trusted root certificate could be read";

/// The test's own path as the test harness names it, for the child to run only this test.
fn this_test() -> String {
    let module = module_path!().split_once("::").map_or("", |(_, rest)| rest);
    format!("{module}::{TEST}")
}

/// This test's own name, which the child is told to run.
const TEST: &str = "ssl_cert_file_sets_the_roots_and_a_missing_file_is_said_on_stderr";

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn openssl(dir: &Path, args: &[&str]) {
    let out = Command::new("openssl")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("this test needs the `openssl` command line tool");
    assert!(
        out.status.success(),
        "openssl {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A free local port, found by asking for one and letting it go.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// What the child answers for one ask of `url` and what it wrote on standard error, with `SSL_CERT_FILE` set to
/// `roots` or removed.
fn asked(url: &str, roots: Option<&Path>) -> (String, String) {
    let mut child = Command::new(std::env::current_exe().unwrap());
    child
        .args(["--exact", &this_test(), "--nocapture", "--test-threads=1"])
        .env(URL_VAR, url)
        .env_remove("SSL_CERT_DIR")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match roots {
        Some(file) => child.env("SSL_CERT_FILE", file),
        None => child.env_remove("SSL_CERT_FILE"),
    };
    let out = child.output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    // libtest has already written `test <name> ... ` on the line the child answers on
    let answer = text
        .lines()
        .find_map(|l| l.split_once(ANSWER).map(|(_, answer)| answer))
        .unwrap_or_else(|| {
            panic!("the child printed no answer, so it did not run this test: {text}\n{stderr}")
        })
        .to_string();
    (answer, stderr)
}

#[test]
fn ssl_cert_file_sets_the_roots_and_a_missing_file_is_said_on_stderr() {
    if let Ok(url) = std::env::var(URL_VAR) {
        let answer = match super::agent().get(&url).call() {
            Ok(resp) => format!("ok {}", resp.status()),
            Err(e) => format!("err {e}"),
        };
        println!("{ANSWER}{answer}");
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    openssl(dir.path(), &[
        "req",
        "-x509",
        "-newkey",
        "rsa:2048",
        "-nodes",
        "-keyout",
        "key.pem",
        "-out",
        "cert.pem",
        "-days",
        "1",
        "-subj",
        "/CN=localhost",
        "-addext",
        "subjectAltName=IP:127.0.0.1",
        "-addext",
        "basicConstraints=critical,CA:FALSE",
    ]);
    std::fs::write(dir.path().join("index.txt"), "served").unwrap();
    let port = free_port();
    let _server = Server(
        Command::new("openssl")
            .current_dir(dir.path())
            .args(["s_server", "-quiet", "-accept"])
            .arg(format!("127.0.0.1:{port}"))
            .args(["-cert", "cert.pem", "-key", "key.pem", "-WWW"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("this test needs the `openssl` command line tool"),
    );
    let started = Instant::now();
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the local TLS endpoint never came up"
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    let url = format!("https://127.0.0.1:{port}/index.txt");
    let (answer, stderr) = asked(&url, Some(&dir.path().join("cert.pem")));
    assert_eq!(
        answer, "ok 200",
        "a request did not trust the certificate `SSL_CERT_FILE` names"
    );
    // Said only when a store ends up empty: a store that read its file has nothing to tell.
    assert!(
        !stderr.contains(NO_ROOTS),
        "a store that read its roots still said it had none: {stderr}"
    );
    // The control, and the case this exists for: the same endpoint with no file naming its certificate is
    // refused. Without it the request above is equally consistent with a client that trusts anything.
    let (refused, _) = asked(&url, None);
    assert!(
        refused.starts_with("err") && refused.contains("invalid peer certificate"),
        "a certificate no root vouches for was not refused as an invalid peer certificate: {refused}"
    );
    // A file that is not there leaves no root at all, and every request is refused as the one above is, which
    // says nothing of why. The cause is on standard error, with the path the variable named.
    let missing = dir.path().join("not-there.pem");
    let (refused, stderr) = asked(&url, Some(&missing));
    assert!(
        refused.starts_with("err"),
        "a request was trusted with no root to trust: {refused}"
    );
    assert!(
        stderr.contains(NO_ROOTS) && stderr.contains(&*missing.to_string_lossy()),
        "a store with no roots did not say so and name the file `SSL_CERT_FILE` named: {stderr}"
    );
}
