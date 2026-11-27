//! The dashboard's HTTP server: the embedded app and its JSON API on `127.0.0.1`, answering only
//! requests addressed to this machine.

#![cfg_attr(
    test,
    allow(
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used,
        clippy::expect_used,
        reason = "Test helpers use concise assertions; production paths remain strict."
    )
)]

mod api;
mod assets;
mod host;
pub mod settings;
pub mod setup;
#[cfg(test)]
mod types;
pub mod version;

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;

use axum::Router;
use ovenspring_store::Store;
use tokio::sync::oneshot;

pub use setup::SetupChecks;

/// What the server reads and writes.
pub struct Context {
    /// The repository this dashboard belongs to.
    pub repo: PathBuf,
    pub store: Store,
    /// The repository's credentials file.
    pub credentials: PathBuf,
    /// The connection checks setup and settings run before saving.
    pub checks: Arc<dyn SetupChecks>,
    /// The owner's home and Codex home, where their own MCP servers are defined.
    pub home: PathBuf,
    pub codex_home: PathBuf,
    /// Where the installed and the latest released versions are read.
    pub version: version::Sources,
}

/// The dashboard while it serves: dropping it closes its port and its connections, and returns
/// once they are closed.
#[derive(Debug)]
pub struct Server {
    pub addr: SocketAddr,
    /// Dropped to end the serving thread; nothing is ever sent.
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Server {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Server {
    /// The address that opens the dashboard.
    pub fn url(&self) -> String {
        format!("http://{}/", self.addr)
    }
}

pub(crate) struct Shared {
    pub context: Context,
    pub versions: version::Known,
}

/// Serves the dashboard on `127.0.0.1` from a thread of its own, on `port` while it is free, so
/// the owner's address and bookmarks keep working across restarts; otherwise (or with 0) on any
/// free port. It serves until the returned [`Server`] is dropped.
pub fn start(context: Context, port: u16) -> std::io::Result<Server> {
    let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .or_else(|_| std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)))?;
    listener.set_nonblocking(true)?;
    let addr = listener.local_addr()?;
    let shared = Arc::new(Shared {
        context,
        versions: version::Known::default(),
    });
    version::watch_releases(Arc::clone(&shared))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()?;
    let (stop, stopped) = oneshot::channel();
    let thread = std::thread::Builder::new()
        .name("dashboard".into())
        .spawn(move || {
            runtime.block_on(async move {
                match tokio::net::TcpListener::from_std(listener) {
                    Ok(listener) => {
                        tokio::spawn(async move {
                            if let Err(e) = axum::serve(listener, router(shared)).await {
                                eprintln!("ovenspring: the dashboard stopped: {e}");
                            }
                        });
                        // The runtime's end, once this returns, drops the listener and every
                        // connection with it.
                        let _ = stopped.await;
                    }
                    Err(e) => eprintln!("ovenspring: the dashboard could not listen: {e}"),
                }
            });
        })?;
    Ok(Server {
        addr,
        stop: Some(stop),
        thread: Some(thread),
    })
}

fn router(shared: Arc<Shared>) -> Router {
    Router::new()
        .nest("/api/v1", api::routes())
        .fallback(assets::serve)
        .layer(axum::middleware::from_fn(host::require))
        .with_state(shared)
}
