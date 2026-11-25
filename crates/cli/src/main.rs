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

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use ovenspring_agents::guard::GuardRules;
use ovenspring_store::{SetRequest, Store, StoreError};

mod app_cmd;
mod daemon_cmd;
mod setup_checks;

#[derive(Parser)]
#[command(
    name = "ovenspring",
    version,
    about = "A software factory that takes issues to merged pull requests"
)]
struct Cli {
    #[command(subcommand)]
    command: Command_,
}

#[derive(Subcommand)]
enum Command_ {
    /// Change a board item (agents): `ovenspring item set <item> status=done`.
    Item {
        #[command(subcommand)]
        action: ItemAction,
    },
    /// Run a command against the product from an issue's worktree (agents).
    App {
        #[command(subcommand)]
        action: AppAction,
    },
    /// Start the factory for this repository in the background.
    Start,
    /// Stop the factory for this repository.
    Stop,
    /// Run the factory in the foreground (what `start` runs).
    #[command(hide = true)]
    Daemon,
    /// The PreToolUse hook of a factory launch (run by the agent CLI).
    Hook {
        #[arg(long)]
        rules: PathBuf,
    },
}

#[derive(Subcommand)]
enum ItemAction {
    Set {
        item: PathBuf,
        /// `status=done|blocked`
        #[arg(required = true)]
        assignments: Vec<String>,
    },
}

#[derive(Subcommand)]
enum AppAction {
    /// Run a command against this worktree's app, starting it first (agents):
    /// `ovenspring app run -- pnpm e2e`.
    Run {
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
}

/// Exit status for a refusal the caller should read and act on.
const REFUSED: u8 = 2;

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("ovenspring: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command_::Item {
            action: ItemAction::Set { item, assignments },
        } => item_set(&item, &assignments),
        Command_::Hook { rules } => Ok(hook(&rules)),
        Command_::App {
            action: AppAction::Run { command },
        } => app_cmd::run(&command),
        Command_::Start => {
            println!("{}", daemon_cmd::start(&repository_root()?)?);
            Ok(ExitCode::SUCCESS)
        }
        Command_::Stop => {
            println!("{}", daemon_cmd::stop(&repository_root()?)?);
            Ok(ExitCode::SUCCESS)
        }
        Command_::Daemon => {
            daemon_cmd::run(&repository_root()?)?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn item_set(item: &Path, assignments: &[String]) -> Result<ExitCode> {
    let item = item
        .canonicalize()
        .with_context(|| format!("no item at {}", item.display()))?;
    // <state>/board/active/<item>.md
    let root = item
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or_else(|| anyhow!("{} is not a board item", item.display()))?;
    let store = Store::open(root)?;
    let pairs = assignments
        .iter()
        .map(|a| {
            a.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .ok_or_else(|| anyhow!("{a:?} is not key=value"))
        })
        .collect::<Result<Vec<_>>>()?;
    let launch = std::env::var(ovenspring_store::LAUNCH_ENV).ok();
    let request = SetRequest {
        path: &item,
        assignments: &pairs,
        launch: launch.as_deref(),
    };
    match store.agent_set(&request) {
        Ok(updated) => {
            println!(
                "{} is {}",
                ovenspring_store::item_name(&item),
                updated.front.status.as_str()
            );
            Ok(ExitCode::SUCCESS)
        }
        Err(StoreError::Refused(reason)) => {
            eprintln!("ovenspring item set: {reason}");
            Ok(ExitCode::from(REFUSED))
        }
        Err(e) => Err(e.into()),
    }
}

/// Any failure here denies the call: an agent CLI runs the tool when its hook fails in any other
/// way than the deny exit code, so a hook that cannot decide must not let the call through.
fn hook(rules: &Path) -> ExitCode {
    let decision = std::fs::read_to_string(rules)
        .map_err(|e| {
            format!(
                "the factory's guard rules at {} could not be read: {e}",
                rules.display()
            )
        })
        .and_then(|text| {
            serde_json::from_str::<GuardRules>(&text)
                .map_err(|e| format!("the factory's guard rules are invalid: {e}"))
        })
        .and_then(|rules| {
            let mut input = String::new();
            std::io::stdin()
                .read_to_string(&mut input)
                .map_err(|e| format!("the hook could not read the tool call: {e}"))?;
            ovenspring_agents::hook::decide(&input, &rules)
        });
    match decision {
        Ok(()) => ExitCode::SUCCESS,
        Err(reason) => {
            eprintln!("{reason}");
            ExitCode::from(REFUSED)
        }
    }
}

fn repository_root() -> Result<PathBuf> {
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("running git")?;
    if !out.status.success() {
        bail!("not inside a git repository");
    }
    Ok(PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
}
