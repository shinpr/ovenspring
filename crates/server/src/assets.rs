//! The dashboard app, embedded at build time. Any path that is not a file is a route of the app,
//! so it gets `index.html` and deep links survive a reload.

use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
// `DASHBOARD`: every file of the built app, by its path inside it (written by build.rs).
include!(concat!(env!("OUT_DIR"), "/dashboard.rs"));

fn file(path: &str) -> Option<(&'static str, &'static [u8])> {
    DASHBOARD.iter().find(|(p, _)| *p == path).copied()
}

pub(crate) async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    match file(path).or_else(|| file("index.html")) {
        Some((path, contents)) => {
            ([(header::CONTENT_TYPE, content_type(path))], contents).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    }
}
