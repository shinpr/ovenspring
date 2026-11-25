//! How the product runs, settled during setup: while `[app]` is unsettled, the lead's
//! `run-commands` step reads the repository, and the factory starts the product once with the
//! commands it found. The owner keeps, changes or replaces the answer; the server writes `[app]`.

use std::net::TcpStream;
use std::path::Path;
use std::time::Instant;

use ovenspring_core::config::{AppConfig, Role};
use ovenspring_core::item::{Front, Item, Status};
use ovenspring_core::output::{RunCommandsOutput, Runs, StepOutput};
use ovenspring_core::step::Step;
use ovenspring_store::{Store, item_name};
use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;

use crate::app::{App, READY_WAIT};
use crate::{Daemon, DaemonError};

const FILE: &str = "product-run.json";

/// Where finding how the product runs stands. What comes next is read from it: no answer and no
/// item on the board files the lead's step; commands with no start recorded start the product.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProductRun {
    /// The lead's latest item, whose messages setup shows.
    pub item: Option<String>,
    pub found: Option<RunCommandsOutput>,
    /// How the start with the answer's commands ended.
    pub check: Option<RunCheck>,
}

impl ProductRun {
    /// The answer's commands, while their start has not ended.
    pub fn to_check(&self) -> Option<AppConfig> {
        match (&self.found, &self.check) {
            (Some(found), None) => commands_of(found),
            (Some(_) | None, Some(_)) | (None, None) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum RunCheck {
    Started { port: u16, seconds: u64 },
    Failed { output: String },
}

pub fn read(store: &Store) -> Result<ProductRun, DaemonError> {
    Ok(store.read_record(FILE)?.unwrap_or_default())
}

fn write(store: &Store, run: &ProductRun) -> Result<(), DaemonError> {
    store.write_record(FILE, run)?;
    Ok(())
}

/// Starts the product once in `worktree` with `config`, waits until it answers, and stops it. A
/// port something else already answers on fails the check: the answer would not be this start's.
pub fn check(state: &Path, worktree: &Path, config: &AppConfig) -> RunCheck {
    let app = App {
        state,
        issue: "setup",
        worktree,
        config,
    };
    let _ = app.stop();
    let port = match app.port() {
        Ok(port) => port,
        Err(e) => {
            return RunCheck::Failed {
                output: e.to_string(),
            };
        }
    };
    if TcpStream::connect(("127.0.0.1", port)).is_ok() {
        return RunCheck::Failed {
            output: format!(
                "Something already answers on port {port}, which the check uses; stop it and continue again."
            ),
        };
    }
    let began = Instant::now();
    let result = app.start(READY_WAIT);
    let _ = app.stop();
    match result {
        Ok(port) => RunCheck::Started {
            port,
            seconds: began.elapsed().as_secs(),
        },
        Err(e) => RunCheck::Failed {
            output: e.to_string(),
        },
    }
}

/// The commands of a `commands` answer, as `[app]` would hold them.
pub fn commands_of(found: &RunCommandsOutput) -> Option<AppConfig> {
    (found.runs == Runs::Commands).then(|| AppConfig {
        setup: found.setup.clone(),
        start: found.start.clone(),
        ready: found.ready.clone(),
        nothing_to_run: false,
    })
}

impl Daemon {
    pub(crate) fn find_product_run(&mut self) -> Result<(), DaemonError> {
        if self.config.app.settled() {
            return Ok(());
        }
        let mut run = read(&self.store)?;
        if let Some(config) = run.to_check() {
            if self
                .product_check
                .as_ref()
                .is_none_or(std::thread::JoinHandle::is_finished)
            {
                let worktree = self.checkout_for(None)?;
                let store = Store::open(self.store.root())?;
                self.product_check = Some(std::thread::spawn(move || {
                    record_check(&store, &worktree, &config);
                }));
            }
            return Ok(());
        }
        if run.found.is_some() {
            return Ok(());
        }
        for path in self.active_items()? {
            if self.store.read_item(&path)?.front.step == Step::RunCommands {
                return Ok(());
            }
        }
        let name = self.new_item_name("run-commands");
        let item = Item {
            front: Front {
                step: Step::RunCommands,
                to: Role::Lead,
                status: Status::Open,
                issue: None,
                sprint: None,
                round: None,
                unit: None,
                refs: vec![],
                out: self.store.output_path(&name).display().to_string(),
                prepare: false,
                resume: None,
            },
            body: "Find how this repository's product starts, for each issue's own copy.\n\n## Result\n"
                .into(),
        };
        self.store.file_item(&name, &item)?;
        run.item = Some(name.clone());
        write(&self.store, &run)?;
        self.event(&json!({"event": "filed", "item": name, "step": Step::RunCommands}))
    }

    /// Takes in the lead's finished answer; the next pass starts the product with its commands. An
    /// item that ended without an answer leaves the commands to the owner.
    pub(crate) fn consume_run_commands_item(
        &self,
        file: &Path,
        item: &Item,
    ) -> Result<(), DaemonError> {
        let found = match item.front.status {
            Status::Open | Status::Doing => return Ok(()),
            Status::Done => match self.read_output(item)? {
                Some(StepOutput::RunCommands(found)) => found,
                _ => unanswered(),
            },
            Status::Blocked | Status::Failed => unanswered(),
        };
        let run = ProductRun {
            item: Some(item_name(file)),
            found: Some(found),
            check: None,
        };
        write(&self.store, &run)?;
        self.store.archive_item(file)?;
        self.event(&json!({"event": "run-commands", "item": item_name(file), "runs": run.found.as_ref().map(|f| f.runs)}))
    }
}

fn unanswered() -> RunCommandsOutput {
    RunCommandsOutput {
        runs: Runs::Unknown,
        setup: None,
        start: None,
        ready: None,
        reason: "The lead's run ended without an answer; its run is in Activity.".into(),
    }
}

fn record_check(store: &Store, worktree: &Path, config: &AppConfig) {
    let result = check(store.root(), worktree, config);
    let recorded = read(store).and_then(|mut run| {
        run.check = Some(result);
        write(store, &run)
    });
    if let Err(e) = recorded {
        eprintln!("ovenspring: recording the product's start: {e}");
    }
}
