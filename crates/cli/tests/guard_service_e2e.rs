//! @lane: service-integration-e2e
//! Area: `ovenspring hook` as the agent CLIs call it, with each CLI's hook input as recorded under
//! `tests/fixtures/hooks/<cli>/`.

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

// AC: "When an agent on Claude Code or Codex runs `git push`, a merge, an approval, or a configured
//     production command, the hook shall deny it (AC-054)" — docs/design/factory-core-design.md
//     § Acceptance Criteria > Launching and guard. Integration Verification Point: "`ovenspring hook`
//     with each CLI's recorded hook inputs for each guard rule, including
//     commands chained with `&&`, `;` or `|` and commands after environment assignments, and with
//     allowed commands that resemble guarded ones (`git log`, `gh pr view`)."
// Consolidation: the matrix of every guard rule against every wrapping is owned by the hook rule
//     table tests of P1-T5 (`crates/agents` hook rules). This test owns the executable's contract with
//     each CLI, with one representative input per decision.
// Behavior: the CLI calls `ovenspring hook` with a recorded `PreToolUse` payload on stdin -> the
//     hook reads the command and the launch's guard rules -> it answers deny with a reason, or
//     allow, in that CLI's protocol.
// Cases (reported distinctly): for each of `claude` and `codex`: `cd web && git push origin HEAD`
//     (deny); `terraform apply -auto-approve` (deny through `guard.deny`); `cat <isolated>/...`
//     (deny: a protected path); `git log --oneline -5` (allow).
// @lane: service-integration-e2e
// @dependency: built `ovenspring` executable, hook rules, configuration
#[test]
fn hook_executable_answers_each_cli_with_deny_or_allow_in_its_protocol() {
    let dir = tempfile::tempdir().unwrap();
    let rules = dir.path().join("guard.json");
    let isolated = dir.path().join("isolated");
    std::fs::write(
        &rules,
        serde_json::json!({"deny": ["terraform apply *"], "protected": [isolated]}).to_string(),
    )
    .unwrap();
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hooks");

    for cli in ["claude", "codex"] {
        let payload = std::fs::read_to_string(fixtures.join(cli).join("pretooluse.json")).unwrap();
        let run = |command: &str| {
            let input = payload.replace("COMMAND", command);
            let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_ovenspring"))
                .args(["hook", "--rules", rules.to_str().unwrap()])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            std::io::Write::write_all(&mut child.stdin.take().unwrap(), input.as_bytes()).unwrap();
            child.wait_with_output().unwrap()
        };

        // Both CLIs treat exit code 2 with the reason on stderr as a deny.
        let push = run("cd web && git push origin HEAD");
        assert_eq!(push.status.code(), Some(2), "{cli}");
        assert_eq!(push.stdout, b"");
        assert!(
            String::from_utf8_lossy(&push.stderr).contains("agents do not push"),
            "{cli}"
        );

        let apply = run("terraform apply -auto-approve");
        assert_eq!(apply.status.code(), Some(2), "{cli}");
        assert!(
            String::from_utf8_lossy(&apply.stderr).contains("terraform apply *"),
            "{cli}"
        );

        let record = run(&format!("cat {}/decisions/d-1.json", isolated.display()));
        assert_eq!(record.status.code(), Some(2), "{cli}");
        assert!(
            String::from_utf8_lossy(&record.stderr).contains("agents do not read or change it"),
            "{cli}"
        );

        let log = run("git log --oneline -5");
        assert_eq!(log.status.code(), Some(0), "{cli}");
        assert!(log.stdout.is_empty() && log.stderr.is_empty(), "{cli}");
    }

    // A hook that cannot read its rules denies rather than lets the call through.
    let missing = std::process::Command::new(env!("CARGO_BIN_EXE_ovenspring"))
        .args([
            "hook",
            "--rules",
            dir.path().join("absent.json").to_str().unwrap(),
        ])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(2));
}
