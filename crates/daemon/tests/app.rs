//! Each issue's worktree runs the product on its own port with its own data directory, so
//! issues in parallel can run their checks against their own server at the same time (AC-016).
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        reason = "tests fail loudly"
    )
)]

use std::fs;
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::time::{Duration, Instant};

use ovenspring_core::config::AppConfig;
use ovenspring_daemon::app::{App, AppError};

const WAIT: Duration = Duration::from_secs(20);

/// The CLI tests' HTTP server, which listens without first looking up its host's name.
const SERVE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../cli/tests/support/serve");

/// A server that answers `/health` with 200 and writes the data directory it was given to a file
/// there, so the test sees which environment each app received.
fn server_config() -> AppConfig {
    AppConfig {
        setup: Some("echo setup >> \"$OVENSPRING_DATA_DIR/setup.log\"".into()),
        start: Some(format!(
            r#"echo "$OVENSPRING_DATA_DIR" > "$OVENSPRING_DATA_DIR/started"; exec python3 "{SERVE}" "$PORT" "$OVENSPRING_DATA_DIR""#
        )),
        ready: Some("http://127.0.0.1:$PORT/started".into()),
        nothing_to_run: false,
    }
}

/// Stops the app when the test ends, passed or failed, so no server outlives it.
struct Stopping<'a>(&'a App<'a>);

impl Drop for Stopping<'_> {
    fn drop(&mut self) {
        let _ = self.0.stop();
    }
}

fn answers(port: u16) -> bool {
    TcpStream::connect(("127.0.0.1", port)).is_ok()
}

/// Another program listening on `port`.
fn answer_on(port: u16) -> TcpListener {
    let deadline = Instant::now() + WAIT;
    loop {
        match TcpListener::bind(("127.0.0.1", port)) {
            Ok(listener) => return listener,
            Err(e) => {
                assert!(Instant::now() < deadline, "port {port} stayed taken: {e}");
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

fn worktree(state: &Path, issue: &str) -> std::path::PathBuf {
    let w = state.join("worktrees").join(issue);
    fs::create_dir_all(&w).unwrap();
    w
}

#[test]
fn two_worktrees_run_their_apps_on_different_ports_each_with_its_own_data_directory() {
    let state = tempfile::tempdir().unwrap();
    let config = server_config();
    let trees = [
        worktree(state.path(), "ACME-1"),
        worktree(state.path(), "ACME-2"),
    ];
    let apps: Vec<App<'_>> = ["ACME-1", "ACME-2"]
        .iter()
        .zip(&trees)
        .map(|(issue, worktree)| App {
            state: state.path(),
            issue,
            worktree,
            config: &config,
        })
        .collect();
    let _stop: Vec<Stopping<'_>> = apps.iter().map(Stopping).collect();

    let ports: Vec<u16> = apps.iter().map(|a| a.start(WAIT).unwrap()).collect();

    assert_ne!(ports[0], ports[1]);
    assert!(answers(ports[0]) && answers(ports[1]), "both serve at once");
    for app in &apps {
        let started = fs::read_to_string(app.data_dir().join("started")).unwrap();
        assert_eq!(Path::new(started.trim()), app.data_dir());
    }
}

#[test]
fn starting_a_running_app_again_keeps_its_port_and_runs_setup_once() {
    let state = tempfile::tempdir().unwrap();
    let config = server_config();
    let tree = worktree(state.path(), "ACME-1");
    let app = App {
        state: state.path(),
        issue: "ACME-1",
        worktree: &tree,
        config: &config,
    };
    let _stop = Stopping(&app);
    let port = app.start(WAIT).unwrap();

    assert_eq!(app.start(WAIT).unwrap(), port);
    assert_eq!(
        fs::read_to_string(app.data_dir().join("setup.log")).unwrap(),
        "setup\n"
    );
}

#[test]
fn an_app_whose_port_another_process_took_starts_on_another_port() {
    let state = tempfile::tempdir().unwrap();
    let config = server_config();
    let tree = worktree(state.path(), "ACME-1");
    let app = App {
        state: state.path(),
        issue: "ACME-1",
        worktree: &tree,
        config: &config,
    };
    let _stop = Stopping(&app);
    let kept = app.start(WAIT).unwrap();
    app.stop().unwrap();
    let _other = answer_on(kept);

    let port = app.start(WAIT).unwrap();

    assert_ne!(port, kept);
    assert!(answers(port));
}

#[test]
fn a_command_run_against_an_app_gets_its_port_and_data_directory() {
    let state = tempfile::tempdir().unwrap();
    let config = server_config();
    let tree = worktree(state.path(), "ACME-1");
    let app = App {
        state: state.path(),
        issue: "ACME-1",
        worktree: &tree,
        config: &config,
    };
    let _stop = Stopping(&app);
    let port = app.start(WAIT).unwrap();
    let out = app.data_dir().join("env.txt");

    let status = app
        .run(
            &[
                "sh".into(),
                "-c".into(),
                format!("echo \"$PORT $OVENSPRING_DATA_DIR\" > {}", out.display()),
            ],
            WAIT,
        )
        .unwrap();

    assert!(status.success());
    assert_eq!(
        fs::read_to_string(&out).unwrap().trim(),
        format!("{port} {}", app.data_dir().display())
    );
}

#[test]
fn a_stopped_app_no_longer_answers_and_has_nothing_left_to_stop() {
    let state = tempfile::tempdir().unwrap();
    let config = server_config();
    let tree = worktree(state.path(), "ACME-1");
    let app = App {
        state: state.path(),
        issue: "ACME-1",
        worktree: &tree,
        config: &config,
    };
    let _stop = Stopping(&app);
    let port = app.start(WAIT).unwrap();

    assert!(app.stop().unwrap());

    assert!(!answers(port));
    assert!(!app.stop().unwrap());
}

#[test]
fn an_app_that_exits_before_it_is_ready_reports_its_log() {
    let state = tempfile::tempdir().unwrap();
    let worktree = state.path().join("worktrees/ACME-3");
    fs::create_dir_all(&worktree).unwrap();
    let config = AppConfig {
        setup: None,
        start: Some("echo 'cannot find module next' >&2; exit 1".into()),
        ready: None,
        nothing_to_run: false,
    };
    let app = App {
        state: state.path(),
        issue: "ACME-3",
        worktree: &worktree,
        config: &config,
    };
    match app.start(WAIT) {
        Err(AppError::Exited { tail }) => {
            assert!(tail.contains("cannot find module next"), "{tail}");
        }
        other => panic!("expected Exited, got {other:?}"),
    }
}

#[test]
fn a_failing_setup_stops_before_the_app_starts() {
    let state = tempfile::tempdir().unwrap();
    let worktree = state.path().join("worktrees/ACME-4");
    fs::create_dir_all(&worktree).unwrap();
    let config = AppConfig {
        setup: Some("echo 'lockfile out of date'; exit 3".into()),
        start: Some("touch \"$OVENSPRING_DATA_DIR/started\"; sleep 30".into()),
        ready: None,
        nothing_to_run: false,
    };
    let app = App {
        state: state.path(),
        issue: "ACME-4",
        worktree: &worktree,
        config: &config,
    };
    match app.start(WAIT) {
        Err(AppError::Setup { tail, .. }) => assert!(tail.contains("lockfile out of date")),
        other => panic!("expected Setup, got {other:?}"),
    }
    assert!(!app.data_dir().join("started").exists());
}
