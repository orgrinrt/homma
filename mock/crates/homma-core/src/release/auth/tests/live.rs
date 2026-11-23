//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The identity request, over a local port.

use std::io::{Read, Write};

use super::*;

/// Serve one request on a local port and hand back what arrived.
fn one_request(respond: &'static str) -> (String, std::thread::JoinHandle<String>) {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/me", l.local_addr().unwrap());
    let h = std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut buf = [0u8; 4096];
        let mut got = Vec::new();
        while !got.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = s.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            got.extend_from_slice(&buf[.. n]);
        }
        s.write_all(respond.as_bytes()).unwrap();
        String::from_utf8_lossy(&got).into_owned()
    });
    (url, h)
}

const OK_EMPTY: &str = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}";

#[test]
fn the_request_names_itself_and_carries_the_token_bare_where_asked() {
    let (url, h) = one_request(OK_EMPTY);
    assert_eq!(Live.whoami(&url, "tok", false).0, 200);
    let req = h.join().unwrap().to_ascii_lowercase();
    assert!(req.contains("\r\nuser-agent: homma/"), "{req}");
    assert!(req.contains("\r\nauthorization: tok\r\n"), "{req}");
}

#[test]
fn the_request_carries_a_bearer_where_asked() {
    let (url, h) = one_request(OK_EMPTY);
    Live.whoami(&url, "tok", true);
    let req = h.join().unwrap().to_ascii_lowercase();
    assert!(req.contains("\r\nauthorization: bearer tok\r\n"), "{req}");
    assert!(req.contains("\r\nuser-agent: homma/"), "{req}");
}

#[test]
fn a_status_the_registry_sends_is_the_answer_and_no_server_is_zero() {
    let (url, h) =
        one_request("HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    assert_eq!(Live.whoami(&url, "tok", true).0, 401);
    h.join().unwrap();
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let gone = format!("http://{}/me", l.local_addr().unwrap());
    drop(l);
    assert_eq!(Live.whoami(&gone, "tok", true).0, 0);
}
