//! @lane: integration
//! Area: installing and updating with `install.sh` against a release, served by a local HTTP server
//! the way GitHub Releases serves one.

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

use std::fs;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use support::{Running, wait_until};

/// The archive name the release publishes for the machine running the test.
fn archive() -> String {
    let arch = match std::env::consts::ARCH {
        "aarch64" => "aarch64",
        "x86_64" => "x86_64",
        other => panic!("no release is built for {other}"),
    };
    let os = match std::env::consts::OS {
        "macos" => "apple-darwin",
        "linux" => "unknown-linux-musl",
        other => panic!("no release is built for {other}"),
    };
    format!("ovenspring-{arch}-{os}.tar.gz")
}

struct Release {
    dir: PathBuf,
    url: String,
    _server: Running,
}

impl Release {
    fn serve(root: &Path) -> Release {
        let dir = root.join("release");
        fs::create_dir_all(&dir).unwrap();
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let server = Command::new("python3")
            .arg(support::SERVE)
            .arg(port.to_string())
            .arg(&dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let server = Running(server);
        assert!(wait_until(10, || std::net::TcpStream::connect((
            "127.0.0.1",
            port
        ))
        .is_ok()));
        Release {
            url: format!("http://127.0.0.1:{port}"),
            dir,
            _server: server,
        }
    }

    /// Publishes an `ovenspring` that prints `version`, and its checksum.
    fn publish(&self, version: &str) {
        let stage = self.dir.join("stage");
        fs::create_dir_all(&stage).unwrap();
        let exe = stage.join("ovenspring");
        fs::write(&exe, format!("#!/bin/sh\necho ovenspring {version}\n")).unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
        let name = archive();
        let tar = Command::new("tar")
            .arg("-czf")
            .arg(self.dir.join(&name))
            .arg("-C")
            .arg(&stage)
            .arg("ovenspring")
            .status()
            .unwrap();
        assert!(tar.success());
        fs::remove_dir_all(&stage).unwrap();
        let sums = Command::new("shasum")
            .args(["-a", "256", &name])
            .current_dir(&self.dir)
            .output()
            .unwrap();
        assert!(sums.status.success());
        fs::write(self.dir.join("SHA256SUMS"), sums.stdout).unwrap();
    }

    /// Changes the published archive's bytes after its checksum was published.
    fn corrupt(&self) {
        let path = self.dir.join(archive());
        let mut bytes = fs::read(&path).unwrap();
        let middle = bytes.len() / 2;
        bytes[middle] ^= 0xff;
        fs::write(path, bytes).unwrap();
    }
}

fn install(home: &Path, release: &Release) -> Output {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../install.sh");
    Command::new("sh")
        .arg(script)
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("OVENSPRING_RELEASE_URL", &release.url)
        .output()
        .unwrap()
}

fn installed_version(home: &Path) -> String {
    let out = Command::new(home.join(".local/bin/ovenspring"))
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

// AC: docs/design/distribution-design.md § Acceptance Criteria: "When the owner runs the install
//     script on macOS or Linux, the system shall place one executable on the PATH and nothing
//     else; running it again shall replace the executable with the latest release (AC-001,
//     AC-004)"; § Non-Functional Requirements, Integrity: "the install script verifies a downloaded
//     archive against the release's published checksum before it replaces the installed
//     executable"; § Error Handling: "Release download fails or checksum mismatch ... Keep the
//     current executable".
//     Integration Verification Point: "The install script with a corrupted archive keeps the
//     installed executable."
// Behavior: release 0.1.0 published -> install.sh installs it -> release 0.2.0 published ->
//     install.sh replaces it -> the archive is corrupted after its checksum was published ->
//     install.sh refuses it.
// @lane: integration
// @dependency: install.sh, curl, tar, the system checksum tool
#[test]
fn install_places_one_verified_executable_and_a_corrupted_archive_changes_nothing() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let release = Release::serve(root.path());
    let bin_entries = || {
        fs::read_dir(home.join(".local/bin"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>()
    };

    release.publish("0.1.0");
    let first = install(&home, &release);
    assert!(first.status.success(), "{first:?}");
    assert_eq!(bin_entries(), ["ovenspring"]);
    assert_eq!(installed_version(&home), "ovenspring 0.1.0");
    assert!(
        String::from_utf8_lossy(&first.stdout).contains("export PATH=\"$HOME/.local/bin:$PATH\"")
    );

    release.publish("0.2.0");
    let update = install(&home, &release);
    assert!(update.status.success(), "{update:?}");
    assert_eq!(installed_version(&home), "ovenspring 0.2.0");

    release.corrupt();
    let refused = install(&home, &release);
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("does not match SHA256SUMS"),
        "{refused:?}"
    );
    assert_eq!(installed_version(&home), "ovenspring 0.2.0");
    assert_eq!(bin_entries(), ["ovenspring"]);
}
