//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The github client against a listener on the loopback, so what goes over
//! the wire is what is asserted: the redirect, the status post, the release.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

use super::*;
use crate::forge::{Forge, StatusState};

/// A stub of the one GitHub behaviour that matters here: a renamed repo
/// answers `301` to a new path, and that path is private, so it answers
/// `404` to anyone without credentials and `200` to anyone with them.
///
/// Returns the base url. The thread ends when the listener is dropped
/// after the last request, which is bounded because each test makes a
/// fixed number.
fn renamed_private_repo(requests: usize) -> String {
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
            let mut authorized = false;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap() == 0 {
                    break;
                }
                if header.trim().is_empty() {
                    break;
                }
                if header.to_ascii_lowercase().starts_with("authorization:") {
                    authorized = true;
                }
            }
            let response = if request_line.contains("/repos/o/renamed") {
                "HTTP/1.1 301 Moved Permanently\r\nLocation: /repositories/123\r\n\
                 Content-Length: 0\r\n\r\n"
                    .to_string()
            } else if request_line.contains("/repositories/123") {
                if authorized {
                    "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}".to_string()
                } else {
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_string()
                }
            } else {
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_string()
            };
            let _ = sock.write_all(response.as_bytes());
            let _ = sock.flush();
        }
    });
    url
}

#[test]
fn a_renamed_private_repo_is_still_found_across_the_redirect() {
    // The failure this pins: ureq defaults to dropping `Authorization` on
    // any redirect, GitHub answers 301 for a renamed repo, and the
    // followed request then arrives anonymous. A private repo answers 404
    // to that, `repo_exists` maps 404 to `Ok(false)`, and `verify --forge`
    // reports a repo that exists as absent. A rename is the normal reason
    // an owner or name goes stale, which is the case the check exists for.
    let url = renamed_private_repo(2);
    let client = GitHubClient::with_token(&url, "t");
    assert_eq!(
        client.repo_exists("o", "renamed").unwrap(),
        true,
        "the credential was dropped following the redirect, so a repo that \
         exists was reported absent"
    );
}

#[test]
fn the_stub_answers_absent_without_a_credential() {
    // The control. Without it the test above passes for a stub that says
    // 200 to everyone, which would prove nothing about the header at all.
    let url = renamed_private_repo(2);
    let client = GitHubClient::anonymous(&url);
    assert_eq!(client.repo_exists("o", "renamed").unwrap(), false);
}

/// A stub that records one request whole and answers `201`, for checking
/// what a status post actually sends. Returns the base url and the slot the
/// request lands in.
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
        *seen.lock().unwrap() = text;
        let _ = sock.write_all(b"HTTP/1.1 201 Created\r\nContent-Length: 2\r\n\r\n{}");
        let _ = sock.flush();
    });
    (url, slot)
}

#[test]
fn a_commit_status_is_posted_to_the_statuses_endpoint_with_its_context_and_state() {
    let (url, seen) = recording_server();
    let client = GitHubClient::with_token(&url, "t");
    let status = CommitStatus {
        context:     "homma/gate".into(),
        state:       StatusState::Success,
        description: "green, 12 tests".into(),
        target_url:  None,
    };
    client
        .set_commit_status("o", "r", "abc123", &status)
        .unwrap();
    let text = seen.lock().unwrap().clone();
    assert!(
        text.starts_with("POST /repos/o/r/statuses/abc123 "),
        "{text}"
    );
    assert!(
        text.to_ascii_lowercase().contains("authorization:"),
        "{text}"
    );
    let body = text.rsplit("\r\n\r\n").next().unwrap();
    let json: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(json["context"], "homma/gate");
    assert_eq!(json["state"], "success");
    assert_eq!(json["description"], "green, 12 tests");
    assert!(
        json.get("target_url").is_none(),
        "an absent url is not sent as null"
    );
}

#[test]
fn a_status_on_a_repo_the_forge_does_not_know_is_repo_not_found() {
    let url = renamed_private_repo(1);
    let client = GitHubClient::anonymous(&url);
    let status = CommitStatus {
        context:     "homma/gate".into(),
        state:       StatusState::Pending,
        description: String::new(),
        target_url:  None,
    };
    assert!(matches!(
        client.set_commit_status("o", "nope", "abc", &status),
        Err(ForgeError::RepoNotFound { .. })
    ));
}

#[test]
fn a_release_is_posted_to_the_releases_endpoint_with_its_tag_name_and_body() {
    let (url, seen) = recording_server();
    let client = GitHubClient::with_token(&url, "t");
    client
        .create_release(
            "o",
            "r",
            "v0.1.1",
            "## 0.1.1 (2026-09-02)\n\n- feat: the thing\n",
        )
        .unwrap();
    let text = seen.lock().unwrap().clone();
    assert!(text.starts_with("POST /repos/o/r/releases "), "{text}");
    assert!(
        text.to_ascii_lowercase().contains("authorization:"),
        "{text}"
    );
    let body = text.rsplit("\r\n\r\n").next().unwrap();
    let json: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(json["tag_name"], "v0.1.1");
    assert_eq!(json["name"], "v0.1.1", "the release is named for its tag");
    assert_eq!(
        json["body"], "## 0.1.1 (2026-09-02)\n\n- feat: the thing\n",
        "the block goes over verbatim"
    );
    assert_eq!(
        json.as_object().unwrap().len(),
        3,
        "nothing beyond the three fields: {body}"
    );
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
fn a_sha_github_has_not_received_is_unknown_and_a_repo_it_has_not_is_an_error() {
    // 422 is what GitHub answers for a sha it does not have, and is the
    // only status that means the commit is still on its way
    let url = status_by_path(
        &[("/repos/o/r/commits/aaa", 200), ("/repos/o/r/commits/bbb", 422)],
        3,
    );
    let client = GitHubClient::anonymous(&url);
    assert!(client.commit_known("o", "r", "aaa").unwrap());
    assert!(!client.commit_known("o", "r", "bbb").unwrap());
    // 404 is the repository, and a poster waiting on it would wait for nothing
    assert!(matches!(
        client.commit_known("o", "gone", "aaa"),
        Err(ForgeError::RepoNotFound { .. })
    ));
}
