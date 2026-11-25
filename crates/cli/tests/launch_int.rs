//! @lane: integration
//! Area: agent launching (configuration -> launcher -> agent child process -> `ovenspring item set`
//! -> store).

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

mod support;

use ovenspring_core::step::Step;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use std::sync::Arc;

use ovenspring_core::item::Status;
use ovenspring_integrations::github::CheckState;
use ovenspring_server::Context;
use ovenspring_server::setup::{GitHubAccount, SetupChecks, SlackSetup, TrackerSetup};
use ovenspring_store::Store;
use serde_json::json;
use support::fake_github::FakeGitHub;
use support::http::request;
use support::recording::RecordingChannel;
use support::recording::RecordingTracker;
use support::scripts::requiring_test;
use support::{Env, Running, git, wait_until};

// AC: "Each launch shall run on the role's configured CLI, model and effort with the same rules,
//     role and step files (AC-007, AC-051)"; "Each launch shall run in its role's permission mode,
//     `auto` unless the owner set `bypass` (AC-058)" — docs/design/factory-core-design.md
//     § Acceptance Criteria > Issue, Launching and guard.
//     PRD AC-054 (part): "people's own sessions in the same repository are unaffected."
// Behavior: an `implement` item for ACME-123 is on the board with `roles.executor.cli` set to the
//     case's CLI -> the daemon launches that CLI (fake agent) in the issue worktree with the prompt
//     on stdin -> the fake agent commits, writes its output and runs `ovenspring item set <item>
//     status=done` -> the item is `done` with a valid output and the launch is logged.
// Cases (reported distinctly): claude, codex, opencode.
// @lane: integration
// @dependency: daemon, launcher, backend adapters, prompt assembly, store, built `ovenspring` executable, fake agent
#[test]
fn implement_item_runs_on_each_configured_cli_and_reaches_done_through_item_set() {
    let mut prompts = Vec::new();
    for cli in ["claude", "codex", "opencode"] {
        prompts.push(run_implement_item_on(cli));
    }
    assert!(
        prompts.windows(2).all(|w| w[0] == w[1]),
        "rules, role and step text differ between CLIs"
    );
}

// AC: PRD AC-044, AC-045: the owner sees each launch's cost, and an issue's.
// Behavior: an `implement` item runs on Codex with no model set -> Codex records the model it ran
//     in its session and reports tokens with cached input inside its input -> the launch is
//     recorded with that model and its cost at that model's price.
// @lane: integration
// @dependency: daemon, launcher, backend adapters, price table, store, fake agent
#[test]
fn a_codex_run_is_priced_by_the_model_its_session_records() {
    let env = Env::new();
    env.install_fake_agents();
    env.configure("[roles.executor]\ncli = \"codex\"\n");
    let store = Store::open(env.state()).unwrap();
    file_implement_item(&store, "20261121-120400-implement-ACME-123");

    run_daemon_until_the_launch_ends(&env, "codex");

    let end: serde_json::Value = std::fs::read_to_string(env.state().join("launches.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .find(|l| l["ended"] == true)
        .unwrap();
    assert_eq!(end["model"], "gpt-6.2-sol", "{end}");
    // gpt-6.2-sol: 600,000 uncached input x 2.00 + 400,000 cached x 0.10 + 100,000 output x 10.00
    // per million.
    let cost = end["cost_usd"].as_f64().unwrap();
    assert!((cost - 2.24).abs() < 1e-9, "{end}");
}

/// Runs one `implement` item for ACME-123 on `cli` through the daemon, checks the case's
/// verification items, and returns the prompt up to the working agreements (rules, role, step).
fn run_implement_item_on(cli: &str) -> String {
    let env = Env::new();
    env.install_fake_agents();
    env.configure(&format!(
        "[roles.executor]\ncli = \"{cli}\"\nmodel = \"model-a\"\neffort = \"high\"\n"
    ));

    let store = Store::open(env.state()).unwrap();
    let name = "20261121-120400-implement-ACME-123";
    let path = file_implement_item(&store, name);
    run_daemon_until_the_launch_ends(&env, cli);

    let prompt = check_launch_record(&env, cli, name);
    check_item_done_and_logged(&env, &store, &path, cli);
    check_repository_untouched(&env, cli);
    prompt
}

fn file_implement_item(store: &Store, name: &str) -> PathBuf {
    let item = support::item(store, name, Step::Implement, Some("ACME-123"));
    store.file_item(name, &item).unwrap()
}

fn run_daemon_until_the_launch_ends(env: &Env, cli: &str) {
    let log = env.dir.path().join("daemon.log");
    let daemon = Running(
        env.ovenspring(&["daemon"])
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(&log).unwrap())
            .spawn()
            .unwrap(),
    );
    let ended = || {
        std::fs::read_to_string(env.state().join("launches.jsonl"))
            .unwrap_or_default()
            .contains("\"ended\":true")
    };
    assert!(
        wait_until(60, ended),
        "{cli}: the launch did not end; daemon log:\n{}\nstate: {}\nstates: {:?}",
        std::fs::read_to_string(&log).unwrap_or_default(),
        env.state().display(),
        std::fs::read_dir(env.state_base().join("ovenspring/repos"))
            .map(|d| d.map(|e| e.unwrap().path()).collect::<Vec<_>>())
    );
    drop(daemon);
}

/// Checks the fake agent's record of the launch and returns the prompt up to the working
/// agreements.
fn check_launch_record(env: &Env, cli: &str, name: &str) -> String {
    let records = env.records();
    assert_eq!(records.len(), 1, "{cli}: one launch");
    let record = &records[0];
    let argv: Vec<String> = record["argv"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap().to_string())
        .collect();
    let joined = argv.join(" ");
    assert!(
        joined.contains("model-a") && joined.contains("high"),
        "{cli}: {joined}"
    );
    let isolated = env.isolated();
    assert!(
        !joined.contains(isolated.to_str().unwrap()),
        "{cli}: {joined}"
    );
    if cli == "opencode" {
        // OpenCode has no hook: its permissions deny a command that names the isolated directory.
        let config: serde_json::Value =
            serde_json::from_str(record["env"]["OPENCODE_CONFIG_CONTENT"].as_str().unwrap())
                .unwrap();
        let named = format!("*{}*", isolated.display());
        let denied = config["permission"]["bash"][&named] == "deny"
            || config["permissions"].as_array().is_some_and(|rules| {
                rules.iter().any(|r| {
                    r["action"] == "shell"
                        && r["resource"] == named.as_str()
                        && r["effect"] == "deny"
                })
            });
        assert!(denied, "{config}");
    } else {
        let rules: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(env.state().join(format!("launches/{name}.1/guard.json")))
                .unwrap(),
        )
        .unwrap();
        let protected: Vec<&str> = rules["protected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap())
            .collect();
        // The daemon runs with the test's home, so the credentials file is named by its key.
        let credentials = format!(
            "/ovenspring/credentials/{}.toml",
            ovenspring_store::repo_key(&env.repo)
        );
        assert!(
            protected.len() == 2
                && protected[0] == isolated.to_str().unwrap()
                && protected[1].ends_with(&credentials),
            "{cli}: {protected:?}"
        );
    }
    let worktree = env.state().join("worktrees/ACME-123");
    assert_eq!(
        record["cwd"].as_str().unwrap(),
        worktree.to_str().unwrap(),
        "{cli}"
    );
    assert_eq!(record["env"]["OVENSPRING_LAUNCH"], name, "{cli}");

    let prompt = record["prompt"].as_str().unwrap();
    prompt
        .split("# Working agreements")
        .next()
        .unwrap()
        .to_string()
}

fn check_item_done_and_logged(env: &Env, store: &Store, path: &Path, cli: &str) {
    let name = ovenspring_store::item_name(path);
    let done = store.read_item(path).unwrap();
    assert_eq!(done.front.status, Status::Done, "{cli}");
    let output: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(store.output_path(&name)).unwrap()).unwrap();
    assert_eq!(output["status"], "completed");
    let lines: Vec<serde_json::Value> = std::fs::read_to_string(env.state().join("launches.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 2, "{cli}: one start and one end line");
    assert_eq!(
        (
            lines[0]["cli"].as_str(),
            lines[0]["model"].as_str(),
            lines[0]["effort"].as_str()
        ),
        (Some(cli), Some("model-a"), Some("high"))
    );
    assert_eq!(lines[1]["failure"], serde_json::Value::Null, "{cli}");
}

fn check_repository_untouched(env: &Env, cli: &str) {
    assert_eq!(
        git(
            &env.repo,
            &["status", "--porcelain", "--untracked-files=all"]
        ),
        "",
        "{cli}: main checkout changed"
    );
    for dir in [".claude", ".codex", ".agents"] {
        assert!(
            !env.repo.join(dir).exists(),
            "{cli}: {dir} created in the main checkout"
        );
    }
    // The worktree holds the factory's skills where each CLI loads them, and nothing a CLI wrote
    // of its own; git sees none of it.
    let worktree = env.state().join("worktrees/ACME-123");
    for root in [".agents/skills", ".claude/skills"] {
        assert!(
            worktree
                .join(root)
                .join("ovenspring-coding-principles/SKILL.md")
                .is_file(),
            "{cli}: no factory skills under {root}"
        );
    }
    let claude: Vec<String> = std::fs::read_dir(worktree.join(".claude"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(claude, ["skills"], "{cli}");
    assert!(
        !worktree.join(".codex").exists(),
        "{cli}: .codex created in the worktree"
    );
    let status = git(
        &worktree,
        &["status", "--porcelain", "--untracked-files=all"],
    );
    assert!(!status.contains("ovenspring-"), "{cli}: {status}");
}

// AC: "When the owner saves a role's settings, the system shall write `.ovenspring/config.toml` and
//     the next launch of that role shall use them (AC-007 to AC-009)" —
//     docs/design/owner-interface-design.md § Acceptance Criteria > Dashboard.
//     "Each launch's prompt shall end with the role's additional instructions when set (AC-009)";
//     "Each launch shall have the factory's skills and the role's selected repository skills
//     available as skills through its CLI, and no other repository skill (AC-008)" —
//     docs/design/factory-core-design.md § Launching and guard.
// Behavior: while the daemon runs, `PUT /api/v1/settings/roles` saves the lead as cli `codex`,
//     model `model-b`, effort `high`, skills `["payments-domain"]`, append `Prefer small pull
//     requests.`, and `PUT /api/v1/settings/agents` sets permissions `bypass` ->
//     `.ovenspring/config.toml` is rewritten -> the next lead item launches on the `codex` shim with
//     those values.
// @lane: integration
// @dependency: dashboard server, configuration, daemon, launcher, prompt assembly, store, fake agent
#[test]
fn saved_role_settings_apply_from_that_roles_next_launch() {
    let env = env_with_skills(&["payments-domain", "migrations"]);
    env.configure("[roles.executor]\ncli = \"claude\"\nmodel = \"model-a\"\neffort = \"low\"\n");
    let mut daemon = in_process_daemon(&env);
    daemon.tick().unwrap();
    let server = settings_server(&env);
    let put = |section: &str, body: &serde_json::Value| {
        request(
            server.addr,
            &format!("PUT /api/v1/settings/{section}"),
            Some(body),
        )
    };
    let lead = json!({"role": "lead", "cli": "codex", "model": "model-b", "effort": "high",
                      "skills": ["payments-domain"], "append": "Prefer small pull requests."});
    let config_file = env.repo.join(".ovenspring/config.toml");
    let before = std::fs::read_to_string(&config_file).unwrap();
    let mut absent = lead.clone();
    absent["skills"] = json!(["absent-skill"]);

    let refused = put("roles", &absent);
    assert_eq!(refused.status, 400, "{}", refused.body);
    assert!(
        refused.json()["errors"]["roles.lead.skills"].is_string(),
        "{}",
        refused.body
    );
    assert_eq!(std::fs::read_to_string(&config_file).unwrap(), before);

    assert_eq!(put("roles", &lead).status, 200);
    let agents = put("agents", &json!({"permissions": "bypass", "mcp": []}));
    assert_eq!(agents.status, 200, "{}", agents.body);
    env.tracker_issue("ACME-124", "Order history", "Customers see past orders.");
    file(daemon.store(), Step::Requirements, "ACME-124");
    file(daemon.store(), Step::Implement, "ACME-123");
    support::run_until(&mut daemon, 60, |_| {}, |_| env.records().len() >= 2);

    let saved = std::fs::read_to_string(&config_file).unwrap();
    assert!(
        saved.contains("model-b") && saved.contains("Prefer small pull requests."),
        "{saved}"
    );
    let records = env.records();
    let launched = |cli: &str| {
        records
            .iter()
            .find(|r| r["cli"] == cli)
            .unwrap_or_else(|| panic!("no {cli} launch in {records:?}"))
    };
    check_lead_launch(launched("codex"));
    let executor = launched("claude")["argv"].to_string();
    assert!(
        executor.contains("model-a") && executor.contains("low"),
        "{executor}"
    );
}

/// A repository with the named skills under `.agents/skills`, where Codex loads them.
fn env_with_skills(skills: &[&str]) -> Env {
    let env = Env::new();
    for skill in skills {
        let dir = env.repo.join(".agents/skills").join(skill);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {skill}\ndescription: The {skill} rules.\n---\n"),
        )
        .unwrap();
    }
    env.install_fake_agents();
    env
}

fn in_process_daemon(env: &Env) -> ovenspring_daemon::Daemon {
    support::daemon(
        env,
        support::services(
            Arc::new(FakeGitHub::new(
                env.origin.clone(),
                requiring_test(),
                Box::new(|_| vec![("test".to_owned(), CheckState::Success)]),
            )),
            Arc::new(RecordingChannel::default()),
            Arc::new(RecordingTracker::default()),
        ),
    )
}

fn settings_server(env: &Env) -> ovenspring_server::Server {
    ovenspring_server::start(
        Context {
            repo: env.repo.clone(),
            store: Store::open(env.state()).unwrap(),
            credentials: env.dir.path().join("config/credentials.toml"),
            checks: Arc::new(Unchecked),
            home: env.home.clone(),
            codex_home: env.home.join(".codex"),
            version: support::no_new_version(),
        },
        0,
    )
    .unwrap()
}

/// The lead's launch after the save: Codex with the saved model, effort and permission mode,
/// `payments-domain` among the skills to load and left on, and the additional instructions last.
fn check_lead_launch(record: &serde_json::Value) {
    let argv: Vec<&str> = record["argv"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    let joined = argv.join(" ");
    assert!(
        joined.contains("model-b")
            && joined.contains("high")
            && joined.contains("--dangerously-bypass-approvals-and-sandbox"),
        "{joined}"
    );
    let switched_off: Vec<&&str> = argv
        .iter()
        .filter(|a| a.starts_with("skills.config="))
        .collect();
    assert!(
        switched_off.iter().all(|a| !a.contains("payments-domain"))
            && switched_off.iter().any(|a| a.contains("migrations")),
        "{switched_off:?}"
    );
    let prompt = record["prompt"].as_str().unwrap();
    assert!(
        prompt.trim_end().ends_with("Prefer small pull requests."),
        "{prompt}"
    );
    let skills_line = prompt
        .lines()
        .find(|l| l.contains("payments-domain"))
        .unwrap();
    assert!(!skills_line.contains("migrations"), "{skills_line}");
}

/// Files an open `step` item for `issue` on the board.
fn file(store: &Store, step: Step, issue: &str) {
    let name = format!("20261121-120500-{step}-{issue}");
    let item = support::item(store, &name, step, Some(issue));
    store.file_item(&name, &item).unwrap();
}

/// Setup checks the settings sections under test never reach.
struct Unchecked;

impl SetupChecks for Unchecked {
    fn tracker(&self, _: &Path, _: &TrackerSetup, _: Option<&str>) -> Result<String, String> {
        Err("not checked here".into())
    }
    fn slack(&self, _: &SlackSetup) -> Result<String, String> {
        Err("not checked here".into())
    }
    fn github(&self, _: &Path, _: Option<&str>, _: &str) -> Result<GitHubAccount, String> {
        Err("not checked here".into())
    }
}
