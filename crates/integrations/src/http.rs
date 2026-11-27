//! HTTPS requests behind a trait, so clients can be tested against recorded responses.

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<serde_json::Value>,
    /// Bytes sent as they are when there is no JSON body, such as a file's content.
    pub raw: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    /// Header names in lowercase.
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum HttpError {
    /// The request may succeed later. `status` is the answer's HTTP status, when an answer with
    /// an error status is why (429, 5xx); none when nothing answered or a 403 told a rate limit.
    #[error("{message}")]
    Transient {
        why: Unanswered,
        status: Option<u16>,
        message: String,
    },
    /// The service refused the request.
    #[error("{method} {url}: HTTP {status}: {message}")]
    Status {
        method: &'static str,
        url: String,
        status: u16,
        message: String,
    },
    #[error("{0}")]
    Decode(String),
}

/// Why a service did not answer a request that may succeed later; each asks something different
/// of the owner, from checking their own network to only waiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Unanswered {
    /// No connection, or no answer in time.
    NoConnection,
    /// The service answered with an error of its own (5xx).
    ServerError,
    /// The service limits how often it is called.
    RateLimited,
}

impl HttpError {
    pub fn status(&self) -> Option<u16> {
        match self {
            HttpError::Status { status, .. } => Some(*status),
            HttpError::Transient { .. } | HttpError::Decode(_) => None,
        }
    }

    pub fn transient(why: Unanswered, message: String) -> HttpError {
        HttpError::Transient {
            why,
            status: None,
            message,
        }
    }
}

pub trait Transport: Send + Sync {
    /// Sends the request and returns the response whatever its status; only failures to get a
    /// response are errors.
    fn send(&self, request: &Request) -> Result<Response, HttpError>;
}

/// The real transport: rustls with the platform's certificate verifier, so a corporate root
/// installed on the machine is trusted.
pub struct Ureq {
    agent: ureq::Agent,
}

impl Ureq {
    pub fn new() -> Ureq {
        let tls = ureq::tls::TlsConfig::builder()
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .build();
        let config = ureq::Agent::config_builder()
            .tls_config(tls)
            .http_status_as_error(false)
            // A call blocks the daemon's loop, so one that hangs must end. GitHub ends any API
            // request that runs over 10 seconds; a minute leaves room for the network.
            .timeout_global(Some(Duration::from_secs(60)))
            .user_agent(concat!("ovenspring/", env!("CARGO_PKG_VERSION")))
            .build();
        Ureq {
            agent: config.into(),
        }
    }
}

impl Default for Ureq {
    fn default() -> Self {
        Ureq::new()
    }
}

impl Transport for Ureq {
    fn send(&self, request: &Request) -> Result<Response, HttpError> {
        let mut builder = ureq::http::Request::builder()
            .method(request.method.as_str())
            .uri(&request.url);
        for (k, v) in &request.headers {
            builder = builder.header(k, v);
        }
        let body = match (&request.body, &request.raw) {
            (Some(json), _) => {
                builder = builder.header("content-type", "application/json");
                serde_json::to_vec(json).map_err(|e| HttpError::Decode(e.to_string()))?
            }
            (None, Some(bytes)) => {
                builder = builder.header("content-type", "application/octet-stream");
                bytes.clone()
            }
            (None, None) => Vec::new(),
        };
        let req = builder
            .body(body)
            .map_err(|e| HttpError::Decode(e.to_string()))?;
        let mut res = self.agent.run(req).map_err(|e| {
            HttpError::transient(
                Unanswered::NoConnection,
                format!("{} {}: {e}", request.method.as_str(), request.url),
            )
        })?;
        let status = res.status().as_u16();
        let headers = res
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_owned(),
                    v.to_str().unwrap_or_default().to_owned(),
                )
            })
            .collect();
        let body = res.body_mut().read_to_string().map_err(|e| {
            HttpError::transient(
                Unanswered::NoConnection,
                format!("{} {}: {e}", request.method.as_str(), request.url),
            )
        })?;
        Ok(Response {
            status,
            headers,
            body,
        })
    }
}

/// Turns a response into its body, classifying failures: 429 and 5xx are transient.
pub fn check(request: &Request, response: Response) -> Result<Response, HttpError> {
    match response.status {
        200..=299 => Ok(response),
        403 if rate_limited(&response) => Err(HttpError::transient(
            Unanswered::RateLimited,
            format!("{} {}: rate limited", request.method.as_str(), request.url),
        )),
        429 => Err(HttpError::Transient {
            why: Unanswered::RateLimited,
            status: Some(429),
            message: format!("{} {}: HTTP 429", request.method.as_str(), request.url),
        }),
        500..=599 => Err(HttpError::Transient {
            why: Unanswered::ServerError,
            status: Some(response.status),
            message: format!(
                "{} {}: HTTP {}",
                request.method.as_str(),
                request.url,
                response.status
            ),
        }),
        status => Err(HttpError::Status {
            method: request.method.as_str(),
            url: request.url.clone(),
            status,
            message: error_message(&response.body),
        }),
    }
}

/// GitHub answers both its primary and secondary rate limits with 403.
fn rate_limited(response: &Response) -> bool {
    response.header("x-ratelimit-remaining") == Some("0")
        || response.header("retry-after").is_some()
        || error_message(&response.body)
            .to_ascii_lowercase()
            .contains("rate limit")
}

fn error_message(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(str::to_owned))
        .unwrap_or_else(|| body.chars().take(200).collect())
}
