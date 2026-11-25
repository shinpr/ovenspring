//! `/api/v1`: JSON for the dashboard app.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use ovenspring_core::config::Config;
use ovenspring_daemon::download::{self, Scope};
use ovenspring_daemon::{DaemonError, history, overview, records};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::settings::{Refusal, SettingsSection};
use crate::setup::{RunRequest, SetupCheck, SetupRefused, SetupRequest};
use crate::{Shared, settings, setup};

pub(crate) fn routes() -> Router<Arc<Shared>> {
    Router::new()
        .route("/overview", get(get_overview))
        .route("/goal", put(put_goal))
        .route("/setup", get(get_setup).put(put_setup))
        .route("/setup/check", post(post_setup_check))
        .route("/setup/run", put(put_setup_run))
        .route("/issues/{key}", get(get_issue))
        .route("/issues/{key}/cancel", post(post_cancel))
        .route("/issues/{key}/prototype", get(get_prototype))
        .route("/sprints", get(get_sprints))
        .route("/launches", get(get_launches))
        .route("/launches/{id}", get(get_launch))
        .route("/launches/{id}/log", get(get_launch_log))
        .route("/export", get(get_export))
        .route("/settings", get(get_settings))
        .route("/settings/{section}", put(put_settings))
        .route("/skills", get(get_skills))
        .route("/version", get(get_version))
}

/// Every refused or failed call answers this: why, by the field or section it concerns, or by
/// `request` when it concerns the request as a whole.
#[derive(Debug, Serialize, TS)]
pub struct ApiErrors {
    pub errors: BTreeMap<String, String>,
}

fn refuse(status: StatusCode, errors: BTreeMap<String, String>) -> Response {
    (status, Json(ApiErrors { errors })).into_response()
}

fn refuse_request(status: StatusCode, why: &str) -> Response {
    refuse(
        status,
        BTreeMap::from([("request".to_owned(), why.to_owned())]),
    )
}

/// A failure the owner cannot fix from the request: logged, and answered without details.
fn internal(e: &impl std::fmt::Display) -> Response {
    eprintln!("ovenspring: dashboard: {e}");
    refuse_request(
        StatusCode::INTERNAL_SERVER_ERROR,
        "The factory could not read its records.",
    )
}

/// The goal as it is saved.
#[derive(Debug, Serialize, TS)]
pub struct GoalSaved {
    pub goal: String,
}

async fn get_overview(State(shared): State<Arc<Shared>>) -> Response {
    // The running factory keeps the settings it last read without an error; the file's error is
    // what the owner fixes.
    let config_error = Config::load(&shared.context.repo)
        .err()
        .map(|e| e.to_string());
    let config = Config::read(&shared.context.repo).ok();
    match overview::overview(&shared.context.store, config_error, config.as_ref()) {
        Ok(o) => Json(o).into_response(),
        Err(e) => internal(&e),
    }
}

async fn get_version(State(shared): State<Arc<Shared>>) -> Response {
    Json(shared.versions.state(&shared.context.version)).into_response()
}

#[derive(Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct GoalRequest {
    text: String,
}

async fn put_goal(State(shared): State<Arc<Shared>>, Json(body): Json<GoalRequest>) -> Response {
    let store = &shared.context.store;
    match records::set_goal(store, &body.text) {
        Ok(()) => match records::goal(store) {
            Ok(goal) => Json(GoalSaved { goal }).into_response(),
            Err(e) => internal(&e),
        },
        Err(DaemonError::Invalid(message)) => refuse(
            StatusCode::BAD_REQUEST,
            BTreeMap::from([("text".to_owned(), message)]),
        ),
        Err(e) => internal(&e),
    }
}

async fn get_setup(State(shared): State<Arc<Shared>>) -> Response {
    match setup::state(&shared.context) {
        Ok(state) => Json(state).into_response(),
        Err(e) => internal(&e),
    }
}

// The checks call Slack, the tracker and GitHub, and start the product; they run off the
// server's own thread.

async fn post_setup_check(
    State(shared): State<Arc<Shared>>,
    Json(body): Json<SetupCheck>,
) -> Response {
    match tokio::task::spawn_blocking(move || setup::check(&shared.context, &body)).await {
        Ok(Ok(found)) => Json(found).into_response(),
        Ok(Err(errors)) => refuse(StatusCode::BAD_REQUEST, errors),
        Err(e) => internal(&e),
    }
}

async fn put_setup(State(shared): State<Arc<Shared>>, Json(body): Json<SetupRequest>) -> Response {
    setup_answer(tokio::task::spawn_blocking(move || setup::save(&shared.context, &body)).await)
}

async fn put_setup_run(
    State(shared): State<Arc<Shared>>,
    Json(body): Json<RunRequest>,
) -> Response {
    setup_answer(tokio::task::spawn_blocking(move || setup::save_run(&shared.context, body)).await)
}

fn setup_answer(
    result: Result<Result<setup::SetupState, SetupRefused>, tokio::task::JoinError>,
) -> Response {
    match result {
        Ok(Ok(state)) => Json(state).into_response(),
        Ok(Err(SetupRefused::Invalid(errors))) => refuse(StatusCode::BAD_REQUEST, errors),
        Ok(Err(SetupRefused::Done)) => refuse_request(
            StatusCode::CONFLICT,
            "This step of setup is done; change it in Settings.",
        ),
        Ok(Err(SetupRefused::Failed(e))) => internal(&e),
        Err(e) => internal(&e),
    }
}

#[derive(Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CancelRequest {
    /// Why the owner cancels it, told to the tracker; may be empty.
    words: String,
}

/// The owner cancels an issue; the factory ends it within seconds, on its next pass.
async fn post_cancel(
    State(shared): State<Arc<Shared>>,
    Path(key): Path<String>,
    Json(body): Json<CancelRequest>,
) -> Response {
    match records::request_cancel(&shared.context.store, &key, &body.words) {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(DaemonError::Invalid(why)) => refuse_request(StatusCode::CONFLICT, &why),
        Err(e) => internal(&e),
    }
}

async fn get_issue(State(shared): State<Arc<Shared>>, Path(key): Path<String>) -> Response {
    match history::issue_detail(&shared.context.store, &key) {
        Ok(Some(detail)) => Json(detail).into_response(),
        Ok(None) => refuse_request(StatusCode::NOT_FOUND, "No such issue."),
        Err(e) => internal(&e),
    }
}

/// The issue's prototype as the page it is. An agent wrote it, so it runs sandboxed: in an
/// opaque origin, it cannot read the dashboard's API or send it JSON.
async fn get_prototype(State(shared): State<Arc<Shared>>, Path(key): Path<String>) -> Response {
    match history::prototype_html(&shared.context.store, &key) {
        Ok(Some(html)) => (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CONTENT_SECURITY_POLICY, "sandbox allow-scripts"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ],
            html,
        )
            .into_response(),
        Ok(None) => refuse_request(StatusCode::NOT_FOUND, "This issue has no prototype."),
        Err(e) => internal(&e),
    }
}

async fn get_sprints(State(shared): State<Arc<Shared>>) -> Response {
    match history::ended_sprints(&shared.context.store) {
        Ok(sprints) => Json(sprints).into_response(),
        Err(e) => internal(&e),
    }
}

#[derive(Deserialize)]
struct LaunchFilter {
    sprint: Option<String>,
    issue: Option<String>,
}

async fn get_launches(
    State(shared): State<Arc<Shared>>,
    Query(f): Query<LaunchFilter>,
) -> Response {
    match history::launch_rows(
        &shared.context.store,
        f.sprint.as_deref(),
        f.issue.as_deref(),
    ) {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => internal(&e),
    }
}

fn no_launch() -> Response {
    refuse_request(StatusCode::NOT_FOUND, "No such launch.")
}

async fn get_launch(State(shared): State<Arc<Shared>>, Path(id): Path<String>) -> Response {
    match history::launch_detail(&shared.context.store, &id) {
        Ok(Some(detail)) => Json(detail).into_response(),
        Ok(None) => no_launch(),
        Err(e) => internal(&e),
    }
}

async fn get_launch_log(State(shared): State<Arc<Shared>>, Path(id): Path<String>) -> Response {
    match history::launch_log(&shared.context.store, &id) {
        Ok(Some(log)) => (
            [(
                axum::http::header::CONTENT_TYPE,
                "text/plain; charset=utf-8",
            )],
            log,
        )
            .into_response(),
        Ok(None) => no_launch(),
        Err(e) => internal(&e),
    }
}

#[derive(Deserialize)]
struct ExportQuery {
    scope: String,
}

/// The records of a sprint or an issue as one JSON Lines attachment, every secret masked.
async fn get_export(State(shared): State<Arc<Shared>>, Query(q): Query<ExportQuery>) -> Response {
    let Some(scope) = Scope::parse(&q.scope) else {
        return refuse(
            StatusCode::BAD_REQUEST,
            BTreeMap::from([(
                "scope".to_owned(),
                "Ask for `sprint:<id>` or `issue:<key>`.".to_owned(),
            )]),
        );
    };
    let held = match ovenspring_store::credentials::read(&shared.context.credentials) {
        Ok(held) => held,
        Err(e) => return internal(&e),
    };
    match download::download(&shared.context.store, &scope, &held.values()) {
        Ok(text) => (
            [
                (
                    axum::http::header::CONTENT_TYPE,
                    "application/jsonl; charset=utf-8".to_owned(),
                ),
                (
                    axum::http::header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"ovenspring-{}.jsonl\"", scope.name()),
                ),
            ],
            text,
        )
            .into_response(),
        Err(e) => internal(&e),
    }
}

fn refused(refusal: Refusal) -> Response {
    match refusal {
        Refusal::Unconfigured(why) => refuse_request(StatusCode::CONFLICT, &why),
        Refusal::Invalid(errors) => refuse(StatusCode::BAD_REQUEST, errors),
        Refusal::Failed(e) => internal(&e),
    }
}

async fn get_settings(State(shared): State<Arc<Shared>>) -> Response {
    match settings::state(&shared.context) {
        Ok(state) => Json(state).into_response(),
        Err(refusal) => refused(refusal),
    }
}

async fn put_settings(
    State(shared): State<Arc<Shared>>,
    Path(section): Path<SettingsSection>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    // A connection section calls the tracker or Slack; that runs off the server's own thread.
    let result =
        tokio::task::spawn_blocking(move || settings::save(&shared.context, section, body)).await;
    match result {
        Ok(Ok(state)) => Json(state).into_response(),
        Ok(Err(refusal)) => refused(refusal),
        Err(e) => internal(&e),
    }
}

async fn get_skills(State(shared): State<Arc<Shared>>) -> Response {
    match settings::skills(&shared.context) {
        Ok(skills) => Json(skills).into_response(),
        Err(e) => internal(&e),
    }
}
