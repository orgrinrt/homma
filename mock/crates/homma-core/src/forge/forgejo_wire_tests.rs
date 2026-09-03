//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The forgejo client against a listener on the loopback, so what goes over
//! the wire is what is asserted: the status post and the release.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

use super::*;
use crate::forge::{Forge, StatusState};

/// A stub that records one request whole and answers `201`, or `404` where
/// the path names a repo it does not have. Returns the base url and the slot
/// the request lands in.
fn recording_server() -> (String, std::sync::Arc<std::sync::Mutex<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let slot = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen = slot.clone();
    std::thread::spawn(move || {
        let Ok((mut sock, _)) = listener.accept() else {
            return;
        };
        let mut reader = BufReader::new(sock.try_clone().unwrap());
        let mut text = String::new();
        let mut length = 0usize;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap() == 0 {
                break;
            }
            if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = v.trim().parse().unwrap_or(0);
            }
            let blank = line.trim().is_empty();
            text.push_str(&line);
            if blank {
                break;
            }
        }
        let mut body = vec![0u8; length];
        std::io::Read::read_exact(&mut reader, &mut body).unwrap();
        text.push_str(&String::from_utf8_lossy(&body));
        let missing = text.contains("/repos/o/nope/");
        *seen.lock().unwrap() = text;
        let response: &[u8] = if missing {
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n"
        } else {
            b"HTTP/1.1 201 Created\r\nContent-Length: 2\r\n\r\n{}"
        };
        let _ = sock.write_all(response);
        let _ = sock.flush();
    });
    (url, slot)
}

#[test]
fn a_commit_status_is_posted_under_forgejo_s_repo_path_with_its_context_and_state() {
    let (url, seen) = recording_server();
    let client = ForgejoClient::with_token(&url, "t");
    let status = CommitStatus {
        context:     "homma/gate".into(),
        state:       StatusState::Failure,
        description: "red, failed tests".into(),
        target_url:  None,
    };
    client
        .set_commit_status("o", "r", "abc123", &status)
        .unwrap();
    let text = seen.lock().unwrap().clone();
    let request_line = text.lines().next().unwrap_or("");
    assert!(request_line.starts_with("POST "), "{text}");
    assert!(
        request_line.ends_with("/repos/o/r/statuses/abc123 HTTP/1.1"),
        "{text}"
    );
    assert!(
        text.to_ascii_lowercase().contains("authorization:"),
        "{text}"
    );
    let body = text.rsplit("\r\n\r\n").next().unwrap();
    let json: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(json["context"], "homma/gate");
    assert_eq!(json["state"], "failure");
    assert_eq!(json["description"], "red, failed tests");
    assert!(
        json.get("target_url").is_none(),
        "an absent url is not sent as null"
    );
}

#[test]
fn a_release_is_posted_under_forgejo_s_repo_path_with_its_tag_name_and_body() {
    let (url, seen) = recording_server();
    let client = ForgejoClient::with_token(&url, "t");
    client
        .create_release(
            "o",
            "r",
            "v0.1.1",
            "## 0.1.1 (2026-09-02)\n\n- feat: the thing\n",
        )
        .unwrap();
    let text = seen.lock().unwrap().clone();
    let request_line = text.lines().next().unwrap_or("");
    assert!(
        request_line.starts_with("POST ") && request_line.ends_with("/repos/o/r/releases HTTP/1.1"),
        "{text}"
    );
    assert!(
        text.to_ascii_lowercase().contains("authorization:"),
        "{text}"
    );
    let body = text.rsplit("\r\n\r\n").next().unwrap();
    let json: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(json["tag_name"], "v0.1.1");
    assert_eq!(json["name"], "v0.1.1");
    assert_eq!(json["body"], "## 0.1.1 (2026-09-02)\n\n- feat: the thing\n");
    assert_eq!(json.as_object().unwrap().len(), 3, "{body}");
}

#[test]
fn a_write_to_a_repo_forgejo_does_not_have_is_repo_not_found() {
    let (url, _seen) = recording_server();
    let client = ForgejoClient::with_token(&url, "t");
    let status = CommitStatus {
        context:     "homma/gate".into(),
        state:       StatusState::Success,
        description: String::new(),
        target_url:  None,
    };
    assert!(matches!(
        client.set_commit_status("o", "nope", "abc", &status),
        Err(ForgeError::RepoNotFound { .. })
    ));
}

/// A stub answering each request with the status of the first route whose
/// path fragment the request line carries, `404` where none does, for a
/// fixed number of requests.
fn status_by_path(routes: &'static [(&'static str, u16)], requests: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for _ in 0 .. requests {
            let Ok((mut sock, _)) = listener.accept() else {
                return;
            };
            let mut reader = BufReader::new(sock.try_clone().unwrap());
            let mut request_line = String::new();
            reader.read_line(&mut request_line).unwrap();
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap() == 0 || header.trim().is_empty() {
                    break;
                }
            }
            let status = routes
                .iter()
                .find(|(path, _)| request_line.contains(path))
                .map_or(404, |(_, status)| *status);
            let response = format!("HTTP/1.1 {status} X\r\nContent-Length: 2\r\n\r\n{{}}");
            let _ = sock.write_all(response.as_bytes());
            let _ = sock.flush();
        }
    });
    url
}

#[test]
fn a_forgejo_404_is_the_commit_when_the_repo_answers_and_the_repo_when_it_does_not() {
    // the repo answers, so a 404 on the commit is a sha still on its way:
    // one request for the commit and one asking after the repo
    let url = status_by_path(
        &[
            ("/repos/o/r/git/commits/aaa", 200),
            ("/repos/o/r/git/commits/", 404),
            ("/repos/o/r ", 200),
        ],
        5,
    );
    let client = ForgejoClient::anonymous(&url);
    assert!(client.commit_known("o", "r", "aaa").unwrap());
    assert!(!client.commit_known("o", "r", "bbb").unwrap());
    // nothing under `gone` answers, so the 404 is the repository
    assert!(matches!(
        client.commit_known("o", "gone", "aaa"),
        Err(ForgeError::RepoNotFound { .. })
    ));
}
