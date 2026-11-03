//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A registry request trusts the roots `SSL_CERT_FILE` names, and only those.
//!
//! A cloud container sends every request through a proxy whose authority is a bundle at that path, and a
//! request that trusts only a compiled-in store is refused as an unknown issuer. The test serves a local TLS
//! endpoint with a certificate made for it, and asks it twice from a child process: once with the file
//! naming that certificate, and once without. The roots are read when a connection is first made and the
//! environment is the process's, so each ask is a process of its own rather than a variable set in a test
//! thread.

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Set in the child: the url it asks.
const URL_VAR: &str = "HOMMA_TLS_TEST_URL";
/// What the child prints before its answer.
const ANSWER: &str = "HOMMA_TLS_ANSWER:";

/// The test's own path as the test harness names it, for the child to run only this test.
fn this_test() -> String {
    let module = module_path!().split_once("::").map_or("", |(_, rest)| rest);
    format!("{module}::a_request_trusts_the_roots_ssl_cert_file_names_and_only_those")
}

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

/// What the child prints for one ask of `url`, with `SSL_CERT_FILE` set to `roots` or removed.
fn asked(url: &str, roots: Option<&Path>) -> String {
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
    // libtest has already written `test <name> ... ` on the line the child answers on
    text.lines()
        .find_map(|l| l.split_once(ANSWER).map(|(_, answer)| answer))
        .unwrap_or_else(|| {
            panic!(
                "the child printed no answer, so it did not run this test: {text}\n{}",
                String::from_utf8_lossy(&out.stderr)
            )
        })
        .to_string()
}

#[test]
fn a_request_trusts_the_roots_ssl_cert_file_names_and_only_those() {
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
    assert_eq!(
        asked(&url, Some(&dir.path().join("cert.pem"))),
        "ok 200",
        "a request did not trust the certificate `SSL_CERT_FILE` names"
    );
    // The control, and the case this exists for: the same endpoint with no file naming its certificate is
    // an unknown issuer. Without it the request above is equally consistent with a client that trusts
    // anything.
    let refused = asked(&url, None);
    assert!(
        refused.starts_with("err") && refused.contains("invalid peer certificate"),
        "a certificate no root vouches for was not refused as an invalid peer certificate: {refused}"
    );
}
