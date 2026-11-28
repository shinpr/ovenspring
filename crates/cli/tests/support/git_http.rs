//! GitHub's Git endpoint for the tests: plain HTTP on localhost, serving a bare repository through
//! `git http-backend` as CGI. It demands the test token as GitHub demands a credential: every
//! request without `Authorization: Basic base64(x-access-token:test-token)` is answered 401 with
//! a Basic challenge, as is a request carrying any other `Authorization` field beside it. It
//! records every `Authorization` field it receives, several on one request included, and each
//! service it accepted a request for.

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

/// `Authorization` with the username `x-access-token` and the password `test-token`.
pub const TEST_TOKEN_AUTHORIZATION: &str = "Basic eC1hY2Nlc3MtdG9rZW46dGVzdC10b2tlbg==";

#[derive(Default)]
struct Seen {
    requests: usize,
    authorizations: Vec<String>,
    accepted: Vec<String>,
}

pub struct GitHttp {
    /// `http://127.0.0.1:<port>/<repository directory name>`, the URL `origin` is set to.
    pub url: String,
    seen: Arc<Mutex<Seen>>,
}

impl GitHttp {
    /// Serves `bare` until the test process ends.
    pub fn serve(bare: &Path) -> GitHttp {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let root = bare.parent().unwrap().to_path_buf();
        let name = bare.file_name().unwrap().to_str().unwrap().to_owned();
        let seen = Arc::new(Mutex::new(Seen::default()));
        let recording = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (root, seen) = (root.clone(), recording.clone());
                std::thread::spawn(move || answer(stream, &root, &seen));
            }
        });
        GitHttp {
            url: format!("http://127.0.0.1:{port}/{name}"),
            seen,
        }
    }

    /// How many requests arrived, counted as each connection opens, before it is read or checked
    /// for the token.
    pub fn requests(&self) -> usize {
        self.seen.lock().unwrap().requests
    }

    /// Every `Authorization` field received, in order, each of a request's fields separately;
    /// requests without one are not listed.
    pub fn authorizations(&self) -> Vec<String> {
        self.seen.lock().unwrap().authorizations.clone()
    }

    /// The service (`git-upload-pack` or `git-receive-pack`) of each request accepted with the
    /// test token, in order.
    pub fn accepted(&self) -> Vec<String> {
        self.seen.lock().unwrap().accepted.clone()
    }
}

struct Request {
    method: String,
    path: String,
    query: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The Git service the request is for, from `info/refs?service=` or the POST path.
    fn service(&self) -> String {
        self.query
            .split('&')
            .find_map(|pair| pair.strip_prefix("service="))
            .or_else(|| self.path.rsplit('/').next())
            .unwrap_or_default()
            .to_owned()
    }
}

fn answer(stream: TcpStream, root: &Path, seen: &Mutex<Seen>) {
    seen.lock().unwrap().requests += 1;
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let Some(request) = read_request(&mut reader) else {
        return;
    };
    let authorizations: Vec<String> = request
        .headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("Authorization"))
        .map(|(_, value)| value.clone())
        .collect();
    seen.lock()
        .unwrap()
        .authorizations
        .extend(authorizations.iter().cloned());
    let mut stream = stream;
    if authorizations != [TEST_TOKEN_AUTHORIZATION] {
        let _ = stream.write_all(
            b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"GitHub\"\r\n\
              Content-Length: 0\r\nConnection: close\r\n\r\n",
        );
        return;
    }
    seen.lock().unwrap().accepted.push(request.service());
    let _ = stream.write_all(&backend(root, &request));
}

fn read_request(reader: &mut BufReader<TcpStream>) -> Option<Request> {
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_owned();
    let target = parts.next()?.to_owned();
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    let mut headers = Vec::new();
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).ok()?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        let (name, value) = header.split_once(':')?;
        headers.push((name.trim().to_owned(), value.trim().to_owned()));
    }
    let mut request = Request {
        method,
        path: path.to_owned(),
        query: query.to_owned(),
        headers,
        body: Vec::new(),
    };
    if request
        .header("Transfer-Encoding")
        .is_some_and(|v| v.eq_ignore_ascii_case("chunked"))
    {
        loop {
            let mut size = String::new();
            reader.read_line(&mut size).ok()?;
            let size = usize::from_str_radix(size.trim(), 16).ok()?;
            let mut chunk = vec![0; size + 2];
            reader.read_exact(&mut chunk).ok()?;
            if size == 0 {
                break;
            }
            request.body.extend_from_slice(&chunk[..size]);
        }
    } else if let Some(length) = request.header("Content-Length") {
        let mut body = vec![0; length.parse().ok()?];
        reader.read_exact(&mut body).ok()?;
        request.body = body;
    }
    Some(request)
}

/// Runs `git http-backend` for `request` and turns its CGI answer into an HTTP response.
fn backend(root: &Path, request: &Request) -> Vec<u8> {
    let mut command = Command::new("git");
    command
        .arg("http-backend")
        .env("GIT_PROJECT_ROOT", root)
        .env("GIT_HTTP_EXPORT_ALL", "1")
        // An authenticated user, without which `http-backend` refuses receive-pack.
        .env("REMOTE_USER", "x-access-token")
        .env("REQUEST_METHOD", &request.method)
        .env("PATH_INFO", &request.path)
        .env("QUERY_STRING", &request.query)
        .env("CONTENT_LENGTH", request.body.len().to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for (header, variable) in [
        ("Content-Type", "CONTENT_TYPE"),
        ("Content-Encoding", "HTTP_CONTENT_ENCODING"),
        ("Git-Protocol", "GIT_PROTOCOL"),
    ] {
        if let Some(value) = request.header(header) {
            command.env(variable, value);
        }
    }
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&request.body)
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let split = out
        .stdout
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| (i, 4))
        .or_else(|| {
            out.stdout
                .windows(2)
                .position(|w| w == b"\n\n")
                .map(|i| (i, 2))
        });
    let (head, body) = match split {
        Some((i, gap)) => (&out.stdout[..i], &out.stdout[i + gap..]),
        None => (&out.stdout[..], &[][..]),
    };
    let head = String::from_utf8_lossy(head);
    let mut status = "200 OK".to_owned();
    let mut response = String::new();
    for line in head.lines().filter(|l| !l.is_empty()) {
        match line.split_once(':') {
            Some((name, value)) if name.eq_ignore_ascii_case("Status") => {
                value.trim().clone_into(&mut status);
            }
            _ => {
                response.push_str(line);
                response.push_str("\r\n");
            }
        }
    }
    let mut bytes = format!(
        "HTTP/1.1 {status}\r\n{response}Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    bytes.extend_from_slice(body);
    bytes
}
