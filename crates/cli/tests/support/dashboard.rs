//! The real dashboard server on a test's state directory, answering over HTTP.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ovenspring_server::setup::{GitHubAccount, SetupChecks, SlackSetup, TrackerSetup};
use ovenspring_server::{Context, Server};
use ovenspring_store::Store;
use serde_json::Value;

use super::Env;
use super::http::{Response, request};

pub struct Dashboard {
    pub server: Server,
    pub store: Store,
    pub credentials: PathBuf,
}

/// Setup's connection checks, which a test that saves no connection never calls.
pub struct NoChecks;

impl SetupChecks for NoChecks {
    fn tracker(&self, _: &Path, _: &TrackerSetup, _: Option<&str>) -> Result<String, String> {
        Err("not checked".into())
    }
    fn slack(&self, _: &SlackSetup) -> Result<String, String> {
        Err("not checked".into())
    }
    fn github(&self, _: &Path, _: Option<&str>, _: &str) -> Result<GitHubAccount, String> {
        Err("not checked".into())
    }
}

impl Dashboard {
    /// The server on `env`'s state, with setup's checks unused.
    pub fn start(env: &Env) -> Dashboard {
        Dashboard::with_checks(env, NoChecks)
    }

    pub fn with_checks(env: &Env, checks: impl SetupChecks + 'static) -> Dashboard {
        let credentials = env
            .dir
            .path()
            .join("config/ovenspring/credentials/storefront.toml");
        let server = ovenspring_server::start(
            Context {
                repo: env.repo.clone(),
                store: Store::open(env.state()).unwrap(),
                credentials: credentials.clone(),
                checks: Arc::new(checks),
                home: env.dir.path().join("home"),
                codex_home: env.dir.path().join("home/.codex"),
                version: super::no_new_version(),
            },
            0,
        )
        .unwrap();
        Dashboard {
            server,
            store: Store::open(env.state()).unwrap(),
            credentials,
        }
    }

    /// Plants `value` as a held credential.
    pub fn hold(&self, value: &str) {
        let mut held = ovenspring_store::credentials::Credentials::default();
        held.linear.api_key = Some(value.to_owned());
        ovenspring_store::credentials::write(&self.credentials, &held).unwrap();
    }

    pub fn get(&self, path: &str) -> Response {
        request(self.server.addr, &format!("GET {path}"), None)
    }

    pub fn put(&self, path: &str, body: &Value) -> Response {
        request(self.server.addr, &format!("PUT {path}"), Some(body))
    }

    pub fn post(&self, path: &str, body: &Value) -> Response {
        request(self.server.addr, &format!("POST {path}"), Some(body))
    }

    /// The lines `GET /api/v1/events` lists for `issue`, newest first, of `kind`.
    pub fn events_of(&self, issue: &str, kind: &str) -> Vec<Value> {
        let page = self
            .get(&format!("/api/v1/events?issue={issue}&from=0"))
            .json();
        page["lines"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|l| l["kind"] == kind)
            .cloned()
            .collect()
    }
}
