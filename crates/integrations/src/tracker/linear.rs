//! Linear as the tracker, over its GraphQL API: the configured team's issues in unstarted or
//! started states, with the issues that block them.

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::{Tracker, TrackerComment, TrackerIssue, TrackerState};
use crate::http::{HttpError, Method, Request, Transport, check};

pub const API: &str = "https://api.linear.app/graphql";

const OPEN_ISSUES: &str = r#"query OpenIssues($team: String!, $after: String) {
  issues(first: 50, after: $after, filter: {team: {key: {eq: $team}}, state: {type: {in: ["unstarted", "started"]}}}) {
    nodes {
      identifier title description url
      state { name type }
      labels { nodes { name } }
      comments { nodes { body user { name } } }
      inverseRelations { nodes { type issue { identifier state { type } } } }
    }
    pageInfo { hasNextPage endCursor }
  }
}"#;

const ISSUE_ID: &str = r"query IssueId($key: String!) { issue(id: $key) { id } }";

const STATES: &str = r"query States($team: String!, $type: String!) {
  workflowStates(filter: {team: {key: {eq: $team}}, type: {eq: $type}}) { nodes { id position } }
}";

const UPDATE_STATE: &str = r"mutation UpdateState($id: String!, $state: String!) {
  issueUpdate(id: $id, input: {stateId: $state}) { success }
}";

const VIEWER: &str = r"query Viewer { viewer { id } }";

const TEAM: &str = r"query Team($team: String!) {
  teams(filter: {key: {eq: $team}}) { nodes { name } }
}";

const START: &str = r"mutation Start($id: String!, $state: String!, $assignee: String!) {
  issueUpdate(id: $id, input: {stateId: $state, assigneeId: $assignee}) { success }
}";

const COMMENT: &str = r"mutation Comment($id: String!, $body: String!) {
  commentCreate(input: {issueId: $id, body: $body}) { success }
}";

pub struct Linear<T> {
    transport: T,
    api: String,
    token: String,
    team: String,
}

#[derive(Deserialize)]
struct Nodes<N> {
    nodes: Vec<N>,
}

#[derive(Deserialize)]
struct Named {
    name: String,
}

#[derive(Deserialize)]
struct NamedState {
    name: String,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
struct StateType {
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Related {
    identifier: String,
    state: StateType,
}

#[derive(Deserialize)]
struct Relation {
    #[serde(rename = "type")]
    kind: String,
    issue: Related,
}

#[derive(Deserialize)]
struct RawComment {
    body: String,
    user: Option<Named>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawIssue {
    identifier: String,
    title: String,
    #[serde(default)]
    description: Option<String>,
    url: String,
    state: NamedState,
    labels: Nodes<Named>,
    comments: Nodes<RawComment>,
    inverse_relations: Nodes<Relation>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IssuePage {
    nodes: Vec<RawIssue>,
    page_info: PageInfo,
}

#[derive(Deserialize)]
struct State {
    id: String,
    position: f64,
}

#[derive(Deserialize)]
struct Id {
    id: String,
}

#[derive(Deserialize)]
struct Mutation {
    success: bool,
}

impl<T: Transport> Linear<T> {
    pub fn new(transport: T, token: String, team: String) -> Linear<T> {
        Linear::with_api(transport, API, token, team)
    }

    pub fn with_api(transport: T, api: &str, token: String, team: String) -> Linear<T> {
        Linear {
            transport,
            api: api.to_owned(),
            token,
            team,
        }
    }

    /// Runs one GraphQL operation and returns `data.<field>`.
    fn query<D: DeserializeOwned>(
        &self,
        query: &str,
        variables: &Value,
        field: &str,
    ) -> Result<D, HttpError> {
        // Personal API keys go as they are; OAuth tokens as bearer tokens.
        let auth = if self.token.starts_with("lin_api_") {
            self.token.clone()
        } else {
            format!("Bearer {}", self.token)
        };
        let request = Request {
            method: Method::Post,
            url: self.api.clone(),
            headers: vec![("authorization".into(), auth)],
            body: Some(json!({"query": query, "variables": variables})),
            raw: None,
        };
        let response = check(&request, self.transport.send(&request)?)?;
        let value: Value = serde_json::from_str(&response.body)
            .map_err(|e| HttpError::Decode(format!("unexpected Linear response: {e}")))?;
        if let Some(errors) = value.get("errors").and_then(Value::as_array) {
            let limited = errors.iter().any(|e| {
                e.pointer("/extensions/code").and_then(Value::as_str) == Some("RATELIMITED")
            });
            let message = errors
                .iter()
                .filter_map(|e| e.get("message").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(if limited {
                HttpError::transient(
                    crate::http::Unanswered::RateLimited,
                    format!("Linear rate limit: {message}"),
                )
            } else {
                HttpError::Decode(format!("Linear: {message}"))
            });
        }
        let data = value
            .pointer(&format!("/data/{field}"))
            .cloned()
            .ok_or_else(|| HttpError::Decode(format!("Linear returned no {field}")))?;
        serde_json::from_value(data)
            .map_err(|e| HttpError::Decode(format!("unexpected Linear {field}: {e}")))
    }

    /// The configured team's name; an error naming the key when Linear has no such team (its
    /// issue filter answers an unknown key with no issues, not an error).
    pub fn team_name(&self) -> Result<String, HttpError> {
        let teams: Nodes<Named> = self.query(TEAM, &json!({"team": self.team}), "teams")?;
        teams
            .nodes
            .into_iter()
            .next()
            .map(|t| t.name)
            .ok_or_else(|| {
                HttpError::Decode(format!("Linear has no team with the key {}", self.team))
            })
    }

    fn issue_id(&self, key: &str) -> Result<String, HttpError> {
        let id: Id = self.query(ISSUE_ID, &json!({"key": key}), "issue")?;
        Ok(id.id)
    }

    /// The team's first workflow state of `kind` (`started`, `completed`, `canceled`).
    fn first_state(&self, kind: &str) -> Result<String, HttpError> {
        let states: Nodes<State> = self.query(
            STATES,
            &json!({"team": self.team, "type": kind}),
            "workflowStates",
        )?;
        states
            .nodes
            .into_iter()
            .min_by(|a, b| a.position.total_cmp(&b.position))
            .map(|s| s.id)
            .ok_or_else(|| HttpError::Decode(format!("the team has no {kind} state")))
    }

    /// Runs a mutation, failing when Linear reports it did not succeed.
    fn mutate(&self, mutation: &str, variables: &Value, field: &str) -> Result<(), HttpError> {
        let result: Mutation = self.query(mutation, variables, field)?;
        if result.success {
            Ok(())
        } else {
            Err(HttpError::Decode(format!("Linear {field} did not succeed")))
        }
    }
}

impl<T: Transport> Tracker for Linear<T> {
    fn open_issues(&self) -> Result<Vec<TrackerIssue>, HttpError> {
        let mut out = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let page: IssuePage = self.query(
                OPEN_ISSUES,
                &json!({"team": self.team, "after": after}),
                "issues",
            )?;
            out.extend(page.nodes.into_iter().map(|i| {
                TrackerIssue {
                    key: i.identifier,
                    title: i.title,
                    body: i.description.unwrap_or_default(),
                    url: i.url,
                    started: i.state.kind == "started",
                    state: i.state.name,
                    labels: i.labels.nodes.into_iter().map(|l| l.name).collect(),
                    comments: i
                        .comments
                        .nodes
                        .into_iter()
                        .map(|c| TrackerComment {
                            author: c.user.map(|u| u.name).unwrap_or_default(),
                            body: c.body,
                        })
                        .collect(),
                    // `A blocks B` is on A; B sees it as an inverse relation. Finished blockers no
                    // longer block.
                    depends_on: i
                        .inverse_relations
                        .nodes
                        .into_iter()
                        .filter(|r| {
                            r.kind == "blocks"
                                && !matches!(r.issue.state.kind.as_str(), "completed" | "canceled")
                        })
                        .map(|r| r.issue.identifier)
                        .collect(),
                }
            }));
            match page.page_info {
                PageInfo {
                    has_next_page: true,
                    end_cursor: Some(cursor),
                } => after = Some(cursor),
                PageInfo { .. } => return Ok(out),
            }
        }
    }

    fn comment(&self, key: &str, body: &str) -> Result<(), HttpError> {
        let id = self.issue_id(key)?;
        self.mutate(COMMENT, &json!({"id": id, "body": body}), "commentCreate")
    }

    fn start(&self, key: &str) -> Result<(), HttpError> {
        let state = self.first_state("started")?;
        let viewer: Id = self.query(VIEWER, &json!({}), "viewer")?;
        let id = self.issue_id(key)?;
        self.mutate(
            START,
            &json!({"id": id, "state": state, "assignee": viewer.id}),
            "issueUpdate",
        )
    }

    fn set_state(&self, key: &str, state: TrackerState) -> Result<(), HttpError> {
        let kind = match state {
            TrackerState::Done => "completed",
            TrackerState::Canceled => "canceled",
        };
        let state = self.first_state(kind)?;
        let id = self.issue_id(key)?;
        self.mutate(
            UPDATE_STATE,
            &json!({"id": id, "state": state}),
            "issueUpdate",
        )
    }
}
