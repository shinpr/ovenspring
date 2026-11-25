//! A transport that answers from recorded exchanges and keeps what was sent.
#![allow(dead_code, reason = "Each test binary uses the part it needs.")]

use std::sync::Mutex;

use ovenspring_integrations::http::{HttpError, Method, Request, Response, Transport};
use serde_json::Value;

pub struct Route {
    method: Method,
    url: String,
    /// Text the request body must contain, such as a GraphQL operation name.
    body_has: Option<String>,
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

/// Answers each request from a list of recorded exchanges, in any order, and keeps what was sent.
pub struct Recorded {
    routes: Vec<Route>,
    sent: Mutex<Vec<Request>>,
}

impl Recorded {
    pub fn new() -> Recorded {
        Recorded {
            routes: Vec::new(),
            sent: Mutex::new(Vec::new()),
        }
    }

    pub fn on(mut self, (method, url): (Method, &str), status: u16, body: &Value) -> Recorded {
        self.routes.push(Route {
            method,
            url: url.to_owned(),
            body_has: None,
            status,
            headers: Vec::new(),
            body: body.to_string(),
        });
        self
    }

    pub fn on_with(
        mut self,
        (method, url): (Method, &str),
        headers: &[(&str, &str)],
        body: &Value,
    ) -> Recorded {
        let headers = headers
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        self.routes.push(Route {
            method,
            url: url.to_owned(),
            body_has: None,
            status: 200,
            headers,
            body: body.to_string(),
        });
        self
    }

    /// A POST to `url` whose body contains `has`, answered with 200 and `body`.
    pub fn on_body(mut self, url: &str, has: &str, body: &Value) -> Recorded {
        self.routes.push(Route {
            method: Method::Post,
            url: url.to_owned(),
            body_has: Some(has.to_owned()),
            status: 200,
            headers: Vec::new(),
            body: body.to_string(),
        });
        self
    }

    pub fn sent(&self) -> Vec<Request> {
        self.sent.lock().unwrap().clone()
    }
}

impl Transport for &Recorded {
    fn send(&self, request: &Request) -> Result<Response, HttpError> {
        self.sent.lock().unwrap().push(request.clone());
        let route = self
            .routes
            .iter()
            .find(|r| {
                r.method == request.method
                    && r.url == request.url
                    && r.body_has.as_deref().is_none_or(|has| {
                        request
                            .body
                            .as_ref()
                            .is_some_and(|b| b.to_string().contains(has))
                    })
            })
            .unwrap_or_else(|| panic!("unexpected {} {}", request.method.as_str(), request.url));
        Ok(Response {
            status: route.status,
            headers: route.headers.clone(),
            body: route.body.clone(),
        })
    }
}
