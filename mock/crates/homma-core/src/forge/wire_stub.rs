//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The one reader every stub forge in the wire tests reads a request with.
//!
//! A stub answers after the whole request is off the socket, body included,
//! whether or not it looks at the body. A socket closed with a body still
//! unread reaches the client as a reset rather than the status, so a stub that
//! answers early makes its test assert on which of two threads won. Every stub
//! calls this rather than parsing the headers itself, so a new one cannot
//! leave the drain out.

use std::io::{BufRead, BufReader, Read};
use std::net::TcpStream;

/// A request as it came off the socket: the request line and headers as sent,
/// and exactly the body `Content-Length` announced.
pub(crate) struct Request {
    head: String,
    body: Vec<u8>,
}

impl Request {
    /// The request line, without its line ending.
    pub(crate) fn line(&self) -> &str {
        self.head.lines().next().unwrap_or("")
    }

    /// Whether a header of this name was sent, whatever case either side
    /// spells it in.
    pub(crate) fn carries(&self, header: &str) -> bool {
        let wanted = format!("{}:", header.to_ascii_lowercase());
        self.head
            .lines()
            .skip(1)
            .any(|line| line.to_ascii_lowercase().starts_with(&wanted))
    }

    /// The body, as many bytes as the request announced.
    pub(crate) fn body(&self) -> &[u8] {
        &self.body
    }

    /// The whole request as text, head and body, which is what a stub that
    /// records a request keeps.
    pub(crate) fn text(&self) -> String {
        format!("{}{}", self.head, String::from_utf8_lossy(&self.body))
    }
}

/// Read one request off the socket: the head up to the blank line, then the
/// `Content-Length` bytes after it, which a request without the header has
/// none of.
pub(crate) fn read_request(sock: &TcpStream) -> Request {
    let mut reader = BufReader::new(sock.try_clone().unwrap());
    let mut head = String::new();
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
        head.push_str(&line);
        if blank {
            break;
        }
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).unwrap();
    Request {
        head,
        body,
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::net::{TcpListener, TcpStream};

    use super::*;

    /// A connected pair on the loopback: the client end and the stub's end.
    fn pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        (client, server)
    }

    #[test]
    fn a_request_with_a_body_is_read_whole() {
        let (mut client, server) = pair();
        client
            .write_all(b"POST /user/repos HTTP/1.1\r\nContent-Length: 5\r\n\r\nhello")
            .unwrap();
        let r = read_request(&server);
        assert_eq!(r.line(), "POST /user/repos HTTP/1.1");
        assert_eq!(r.body(), b"hello");
        assert_eq!(
            r.text(),
            "POST /user/repos HTTP/1.1\r\nContent-Length: 5\r\n\r\nhello"
        );
    }

    #[test]
    fn a_request_without_a_length_has_no_body() {
        let (mut client, server) = pair();
        client
            .write_all(b"GET /repos/o/r HTTP/1.1\r\nAccept: */*\r\n\r\n")
            .unwrap();
        let r = read_request(&server);
        assert_eq!(r.line(), "GET /repos/o/r HTTP/1.1");
        assert!(r.body().is_empty());
    }

    #[test]
    fn a_header_is_found_whatever_case_it_is_spelled_in() {
        let (mut client, server) = pair();
        client
            .write_all(
                b"POST /x HTTP/1.1\r\nAUTHORIZATION: token t\r\nCONTENT-LENGTH: 3\r\n\r\nabc",
            )
            .unwrap();
        let r = read_request(&server);
        assert!(r.carries("authorization"));
        assert!(r.carries("Authorization"));
        assert_eq!(r.body(), b"abc", "an upper-case length is still the length");
    }

    #[test]
    fn a_header_that_only_ends_in_the_name_is_not_that_header() {
        let (mut client, server) = pair();
        client
            .write_all(b"GET /x HTTP/1.1\r\nX-Content-Length: 9\r\nX-Authorization: t\r\n\r\n")
            .unwrap();
        let r = read_request(&server);
        assert!(r.body().is_empty(), "a prefixed name announced no body");
        assert!(!r.carries("authorization"));
    }

    #[test]
    fn the_request_line_is_not_read_as_a_header() {
        // A line that would match `authorization:` if the first line were
        // counted among the headers.
        let (mut client, server) = pair();
        client
            .write_all(b"AUTHORIZATION: /x HTTP/1.1\r\nAccept: */*\r\n\r\n")
            .unwrap();
        let r = read_request(&server);
        assert!(!r.carries("authorization"));
        assert!(r.carries("accept"));
    }

    #[test]
    fn a_body_arriving_in_several_writes_is_waited_for() {
        let (mut client, server) = pair();
        let writer = std::thread::spawn(move || {
            client
                .write_all(b"POST /x HTTP/1.1\r\nContent-Length: 10\r\n\r\n")
                .unwrap();
            client.flush().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(50));
            client.write_all(b"01234").unwrap();
            client.flush().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(50));
            client.write_all(b"56789").unwrap();
            client
        });
        let r = read_request(&server);
        assert_eq!(r.body(), b"0123456789");
        drop(writer.join().unwrap());
    }
}
