//! Every request must name this machine as its `Host` (`127.0.0.1` or `localhost`, on any port, so
//! an SSH port forward still reaches it): a page that reaches the port under another host name
//! (DNS rebinding) is refused, so another site cannot read or change anything through the owner's
//! browser. Writes take only JSON bodies (each handler's `Json` extractor), which another site's
//! page cannot send without a preflight this server never grants.

use axum::extract::Request;
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// Refuses a request that does not name this machine.
pub(crate) async fn require(request: Request, next: Next) -> Response {
    let local = request
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .map(|h| h.rsplit_once(':').map_or(h, |(name, _)| name))
        .is_some_and(|name| name == "127.0.0.1" || name == "localhost");
    if local {
        next.run(request).await
    } else {
        (
            StatusCode::FORBIDDEN,
            "Open the dashboard at the address `ovenspring start` opens.",
        )
            .into_response()
    }
}
