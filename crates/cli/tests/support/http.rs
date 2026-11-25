//! A raw HTTP/1.1 client for the dashboard tests: each request says exactly which host it names,
//! and redirects are returned, not followed.

use std::fmt::Write as _;
use std::io::{Read as _, Write as _};
use std::net::{SocketAddr, TcpStream};

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).unwrap_or_else(|e| panic!("{e}: {}", self.body))
    }
}

/// Sends `line` (`"GET /path"`) with a JSON `body` (when given), addressed to `addr` itself.
pub fn request(addr: SocketAddr, line: &str, body: Option<&serde_json::Value>) -> Response {
    request_to(addr, &addr.to_string(), line, body)
}

/// Sends `line` to `addr` naming `host` in its `Host` header, as a page reaching the port under
/// another name does.
pub fn request_to(
    addr: SocketAddr,
    host: &str,
    line: &str,
    body: Option<&serde_json::Value>,
) -> Response {
    let (method, path) = line.split_once(' ').unwrap();
    let mut stream = TcpStream::connect(addr).unwrap();
    let body = body.map(ToString::to_string).unwrap_or_default();
    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    if !body.is_empty() {
        let _ = write!(
            head,
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        );
    }
    stream
        .write_all(format!("{head}\r\n{body}").as_bytes())
        .unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap();
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_owned(), v.trim().to_owned()))
        .collect();
    let body = if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        dechunk(body)
    } else {
        body.to_owned()
    };
    Response {
        status,
        headers,
        body,
    }
}

fn dechunk(mut body: &str) -> String {
    let mut out = String::new();
    while let Some((size, rest)) = body.split_once("\r\n") {
        let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
        if n == 0 {
            break;
        }
        out.push_str(&rest[..n]);
        body = &rest[n + 2..];
    }
    out
}
