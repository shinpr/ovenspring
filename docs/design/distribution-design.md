# Distribution and Configuration Design Document

## Overview

Oven Spring is one executable that an owner installs once per machine and starts in each repository. After `ovenspring start`, the owner sets the factory up and works with it in the dashboard and in Slack; the terminal is used only to install, start and stop it. This document designs the command line, the first start, the configuration file and credentials, updates, and the release pipeline.

## Background and Context

### Prerequisite ADRs

- `docs/adr/ADR-0001-rust-single-binary.md`: one self-contained executable per platform.
- `docs/adr/ADR-0003-binary-distribution.md`: the factory inside the binary; its skills written into the worktrees it creates; GitHub Releases with an install script; updating is replacing the executable.
- `docs/adr/ADR-0005-file-board-and-state-outside-repository.md`: the state directory.

### External Resources Used

| Resource (project-tier label) | Feature-specific identifier | Notes |
|-------------------------------|-----------------------------|-------|
| Release host | GitHub Releases of the Oven Spring repository | Archives, `SHA256SUMS`, install script |
| Latest release | `GET https://api.github.com/repos/shinpr/ovenspring/releases/latest`, unauthenticated | Its `tag_name`; drafts and prereleases are never the latest |

### Requirement Convergence

- **Outcome**: N/A — covered by PRD `docs/prd/oven-spring-prd.md`
- **Non-Goals**: N/A — covered by PRD `docs/prd/oven-spring-prd.md`
- **Open questions**: None

### Standards and Assumptions

#### Applicable Standards

- [x] Semantic Versioning 2.0 `[explicit]` - Source: ADR-0003
- [x] TOML 1.0 for configuration `[explicit]` - Source: this design

#### Assumed Behaviors

None.

#### Quality Assurance Mechanisms

- [x] Release workflow builds and tests on every target before attaching the release's files — Covers: `.github/workflows/release.yml` — Status: `adopted`
- [x] Configuration tests: a minimal configuration takes the defaults, each validation rule rejects its case by key, and serialization round-trips — Covers: `crates/core/src/config.rs` — Status: `adopted`

### Problem to Solve

Bring the factory into any repository and keep it current, with the terminal used only to install, start and stop it.

### Current Challenges

None open. The owner's only commands are `start` and `stop`; every other owner action is in the dashboard or Slack.

### Requirements

#### Functional Requirements

PRD R1, R2, R3 (AC-001, AC-002, AC-004, AC-007 to AC-014, AC-059, AC-060, AC-071).

#### Non-Functional Requirements

- **Integrity**: the install script verifies a downloaded archive against the release's published checksum before it replaces the installed executable - Source: selected in Design Convergence because the requirements leave it unset; the executable runs agents
- **Freshness of the update notice**: a new release shows within a day of its publication, or at the next start; a newly installed executable shows on the dashboard's next answer - Source: selected in Design Convergence because AC-071 leaves it unset; releases come days apart, and the restart notice is read right after the install script runs

## Acceptance Criteria (AC) - EARS Format

- [x] **When** the owner runs the install script on macOS or Linux, the system shall place one executable on the PATH and nothing else; running it again shall replace the executable with the latest release (AC-001, AC-004)
- [x] **When** the owner runs `ovenspring start` in a repository, the system shall start that repository's daemon detached from the terminal and open its dashboard in the browser; when the daemon already runs, it shall only open the dashboard (AC-059)
- [x] **While** the repository has no `.ovenspring/config.toml`, the dashboard shall open on setup and the factory shall start no work (AC-002, AC-060)
- [x] **When** the owner saves setup, the system shall write `.ovenspring/config.toml` in the repository and the credentials outside it, and change nothing else in the repository (AC-002, AC-013)
- [x] **If** the configuration does not name each role's agent CLI, **then** the system shall name the missing key and start no work; no CLI is assumed (AC-065)
- [x] **When** the owner runs `ovenspring stop` in a repository, the system shall stop its daemon after the daemon's current pass (AC-059)
- [x] **If** the configuration holds an unknown key or value, **then** the system shall name the key and start no work (AC-010 to AC-012)
- [x] Credentials shall be stored only in a file with mode 0600 under the user's configuration directory, never in the repository (AC-013)
- [x] The dashboard's header shall show the version the repository's daemon runs (AC-071)
- [x] **While** the executable at the path the daemon started from reports a version other than the running one, the header shall say that a restart runs it, and how to restart (AC-071)
- [x] **While** no such executable is installed and the latest release is newer than the running version by SemVer precedence, the header shall say it is out, with the install command and the restart (AC-071)
- [x] **If** the latest release cannot be read, **then** the system shall log why, keep what it read before, and show the owner no error (AC-071)

## Existing Codebase Analysis

### Implementation Path Mapping

| Type | Path | Description |
|------|------|-------------|
| Existing | `crates/core/src/config.rs` | The configuration, its defaults and its validation |
| Existing | `crates/cli/src/main.rs`, `daemon_cmd.rs` | The commands; `start` and `stop` are the owner's, and `start` opens the dashboard |
| Existing | `crates/server/src/setup.rs` | Setup in the dashboard writes the configuration and the credentials (owner-interface design) |
| Existing | `crates/store/src/credentials.rs` | The credentials file |
| Existing | `crates/agents/src/assets.rs` | The factory's files embedded in the executable |
| Existing | `.github/workflows/ci.yml`, `release.yml`, `install.sh` | Checks, release and install |
| New | `crates/server/src/version.rs` | The running, installed and latest versions, answered at `GET /api/v1/version` |
| Existing | `dashboard/src/components/AppHeader` | Shows the version and the way to a newer one (UI Spec, AppHeader) |

### Integration Points

- **Integration Target**: GitHub Releases; **Invocation Method**: HTTPS download of archives and checksums by the install script.
- **Integration Target**: GitHub's REST API (latest release); **Invocation Method**: an unauthenticated HTTPS `GET` from the daemon, at start and once a day.

### Code Inspection Evidence

| File/Function | Relevance |
|---------------|-----------|
| `crates/core/src/config.rs` `Config` | The configuration contract this design changes |
| `crates/cli/src/daemon_cmd.rs` `start`, `stop` | The two owner commands that remain |

### Fact Disposition Table

| Fact | Disposition |
|------|-------------|
| The owner's commands are `start` and `stop` | Keep; the dashboard and Slack cover every other owner action (AC-059) |
| No version is pinned per repository | Keep; a repository runs whichever executable is installed (ADR-0003) |
| Prompts are built from the embedded files; skills are written into each worktree | Keep; nothing is extracted to a cache (ADR-0003) |

## Design

### Change Impact Map

```yaml
Change Target: command line, configuration, credentials, release, version and updates
Direct Impact:
  - crates/core/src/config.rs, crates/cli, crates/agents/src/assets.rs, release workflow, install script
  - crates/server/src/version.rs, the dashboard's header
Indirect Impact:
  - target repository: .ovenspring/ only
No Ripple Effect:
  - target repository's other files, CI, hooks and agent settings
```

### Interface Change Matrix

None: no earlier version was released, so no command or key has a predecessor to convert.

### Architecture Overview

The `ovenspring` executable is the daemon, the owner's two commands, and the commands agents and CLIs call. Which of these a process is depends on the subcommand.

### Data Flow

```
install.sh ─▶ GitHub Releases ─▶ verify checksum ─▶ ~/.local/bin/ovenspring
ovenspring start ─▶ daemon (detached) ─▶ dashboard on the port it last used, else a free one ─▶ browser
dashboard setup ─▶ .ovenspring/config.toml (repository)
                └▶ <config>/ovenspring/credentials/<name>-<hash>.toml (0600)
```

### Integration Points List

| Integration Point | Location | Old Implementation | New Implementation | Switching Method | Verification Method |
|-------------------|----------|-------------------|-------------------|------------------|-------------------|
| Credentials | `crates/store/src/credentials.rs` | — | 0600 file in a 0700 directory | — | Tests with a temporary configuration directory |
| Install and update | `install.sh` | — | HTTPS download and checksum | — | Test against a local release fixture server |
| Version and updates | `crates/server/src/version.rs` | — | Latest release read by the daemon; installed version read from the executable | — | Dashboard API test with a stand-in release host and a real executable file |

### Main Components

#### Command line

| Command | Who | Does |
|---------|-----|------|
| `ovenspring start` | owner | Starts the repository's daemon detached from the terminal and opens its dashboard; when it runs already, opens the dashboard. Where no browser can open, prints the address. The daemon records `{pid, url}` in `<isolated>/dashboard.json` and keeps it after it stops; the next daemon serves on that port while it is free, so the owner's address stays the same |
| `ovenspring stop` | owner | Stops the repository's daemon after its current pass, ending the agents it runs and the issues' apps; their items run again on the next start |
| `ovenspring item set` | agents | Factory core design |
| `ovenspring app run` | agents | Factory core design |
| `ovenspring hook` | CLIs | Factory core design |

#### Configuration (`.ovenspring/config.toml`)

```toml
[repo]
base_branch = "main"

[merge]
by = "factory"                     # factory | owner

[held]                             # categories held for the owner, in addition to what designs declare
schema = { paths = ["db/migrations/**", "**/schema.prisma"] }
infrastructure = { paths = ["infra/**", "Dockerfile"] }
ci = { paths = [".github/workflows/**"] }
external-contract = { paths = ["api/openapi.yaml"] }
# factory configuration, working agreements and AGENTS.md/CLAUDE.md are always held

[agents]
permissions = "auto"               # auto | bypass (bypass skips the CLIs' permission checks; owner's responsibility)
mcp = ["playwright"]               # MCP servers launches may use, by name: from .mcp.json, ~/.claude.json or Codex config.toml

[roles.lead]
cli = "claude"                     # claude | codex | opencode
model = "claude-opus-5-5"
effort = "high"
skills = ["payments-domain"]       # names in the folder the role's CLI reads skills from
append = ""

[roles.executor]
cli = "codex"
model = "gpt-6-luna"
effort = "max"

[roles.reviewer]
cli = "codex"
model = "gpt-6.1-sol"
effort = "high"

[roles.analyzer]
cli = "codex"
model = "gpt-6.1-terra"
effort = "high"

[tracker]
kind = "linear"                    # linear | github
team = "ACME"                      # linear team key; for github, the repository's own issues
# optional, unset by default (no filter):
# filter = "scripts/issue-filter"  # the owner's command that drops issues before any agent reads them (issue-filter design)

[slack]
channel = "C0123456789"
owners = ["U0123456789"]           # the factory responds only to them and counts only their approvals

[notices]                          # optional; absent means on
errors = true                      # one Slack message when an error starts and one when it clears (events-and-error-notices design)

[app]                              # how an issue's worktree runs the product; settled in setup
setup = "pnpm install --frozen-lockfile"
start = "pnpm dev --port $PORT"
ready = "http://localhost:$PORT/health"
# or, for a product with nothing to start (a library, a CLI):
# nothing_to_run = true

[person_reviews]                   # kinds of change a person reviews before they merge, in the owner's words
payments = "How customers are charged or refunded, src/billing/** included"

[agreements]
path = ".ovenspring/agreements"    # where the working agreements are kept; a folder inside the repository git tracks

[knowledge]
path = ".ovenspring/knowledge"     # where the factory keeps what it learns about the product; a folder inside the repository git tracks

[guard]
deny = ["kubectl * --context *prod*", "terraform apply *"]

[review]
bots = ["ai-reviewer[bot]"]

[limits]
issues_at_once = 3
# optional, unset by default (no limit):
# review_rounds = 3                # a review loop reaching it goes to the lead as an exception
# fix_ci_rounds = 3                # likewise for CI fixes
```

Every field has a default except `tracker`, `slack` and each role's `cli`. Setup writes the whole configuration with every default, all four roles on the CLI chosen (no model or effort set); `[app]` is added when the owner settles how the product runs. `[app]` is unsettled until it has `start` or `nothing_to_run` (not both); the factory proposes no sprint until it is settled. Unknown keys and unknown values are errors. The held categories are `schema`, `external-connection`, `infrastructure`, `ci` and `external-contract`; any other name under `[held]` is an error, and a kind of change outside them is described under `[person_reviews]`. A role's `skills` are looked up where its CLI reads skills: `.claude/skills` for Claude Code, `.agents/skills` for Codex, either for OpenCode.

#### Credentials

| Name | Used by | Source |
|------|---------|--------|
| `slack.bot_token` (`xoxb-`) | Web API | setup |
| `slack.app_token` (`xapp-`, `connections:write`) | Socket Mode | setup |
| `linear.api_key` | Linear GraphQL | setup |
| `github.token` | GitHub REST, push | setup, or the token of a signed-in `gh` when none is entered |

Stored in `<config>/ovenspring/credentials/<name>-<hash>.toml` with mode 0600 (`<config>` is `$XDG_CONFIG_HOME`, else `~/.config`; `<name>-<hash>` as for the state directory). Pull requests are authored by the token's account; branch rules that require an approval from someone other than the author need a token of an account other than the owner's.

#### Updates

Running the install script again installs the latest release. After `ovenspring stop` and `ovenspring start`, each repository runs the new version's rules, role files, step files and skills; nothing in the repositories changes (AC-004).

#### Version and updates

The dashboard tells the owner which version the repository runs and when there is a newer one to move to (AC-071). The dashboard server owns this, since the header is its only reader.

- **Running**: the server's own package version.
- **Installed**: the daemon reads its executable's path once at start (`std::env::current_exe`) and gives it to the server and to the launches' hooks: on Linux, once the install script has moved a new file over it, the running executable's own path reads as deleted. On each `GET /api/v1/version` the server compares the file at that path (device, inode, modification time) with the one it last read; only a different file is run with `--version`, and its `ovenspring X.Y.Z` parsed as SemVer.
- **Latest**: a thread of the server reads the latest release's `tag_name` at start and every 24 hours (assumed value: releases come days apart, and every start reads it again). A failed read is logged to `daemon.log` and keeps the version read before.
- **Update**, the first that holds: the installed version differs from the running one → `restart`, with the installed version (the next start runs it, newer or not); the latest release is newer than the running version by SemVer precedence → `available`, with that version; otherwise none.

`GET /api/v1/version` answers `VersionState {running, update: null | {kind: restart | available, version}}`. The dashboard asks for it as often as for the overview and shows it in the header (UI Spec, AppHeader).

#### Release pipeline

- `ci.yml` on every push and pull request: Rust format, clippy, tests, release build, `cargo deny` and `cargo machete` on Linux and macOS; the dashboard's lint, format, unused-code check, build, unit and Storybook accessibility tests, and end-to-end tests.
- `release.yml` on a release a person publishes, its tag and notes together: builds `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` (statically linked on Linux; on macOS linking only system libraries); runs the tests on each; once every platform has passed and the tag names the crate's version, attaches the archives, `SHA256SUMS` and `install.sh` to the release.
- `install.sh`: detects the platform, downloads the archive and `SHA256SUMS`, verifies, installs to `~/.local/bin`, and prints how to add it to PATH when needed.

### Data Representation Decision

| Criterion | Assessment | Reason |
|-----------|-----------|--------|
| Semantic Fit | Yes | Repository configuration is shared by everyone running the factory there |
| Responsibility Fit | Yes | The owner changes it in the dashboard, the gate guards it |
| Lifecycle Fit | Yes | Changes with the repository |
| Boundary/Interop Cost | Low | One TOML file |

**Decision**: new — `.ovenspring/config.toml`, tracked; credentials separate.

### Selected Design

- **Addition**: checksum verification in the install script — **Current evidence**: Integrity NFR — **Lower-surface insufficiency**: HTTPS alone does not detect a corrupted or substituted archive on the release host — **Subtraction result**: a corrupted archive could replace the executable that runs agents.
- **Addition**: the daemon reads the latest release — **Current evidence**: AC-071 (a newer release is out) — **Lower-surface insufficiency**: the install script and `--version` say nothing until the owner runs them, and the page cannot read GitHub's API from the owner's browser without a cross-origin call — **Subtraction result**: the owner learns of a release only from the release page.
- **Addition**: the server reads the installed executable's version — **Current evidence**: AC-071 (installed, waiting for a restart) — **Lower-surface insufficiency**: the latest release alone cannot tell "not installed" from "installed, not restarted" — **Subtraction result**: the owner who has just run the install script is asked to run it again.
- **Not added**: an `ovenspring update` command (the install command is one line in the dashboard), a Slack notice (Slack carries what needs the owner's decision), restarting by itself (a restart ends the runs under way, so the owner chooses when).

### Data Contracts

#### Configuration

```yaml
Contract: .ovenspring/config.toml
Input:
  Type: TOML as above
  Validation: unknown keys and values rejected; repo.base_branch non-empty; tracker.team required for linear; slack ids match Slack id shape and slack.owners non-empty; held names are the held categories, factory-configuration is refused (always held), each held entry has a path; person_reviews names are not held categories and each has a description; roles.*.skills exist in the folder the role's CLI reads; agreements.path and knowledge.path are relative folders inside the repository; app.start and nothing_to_run not both; limits.issues_at_once at least 1; notices.errors a boolean, true when `[notices]` or the key is absent (its behavior: `docs/design/events-and-error-notices-design.md`)
Output:
  Type: typed Config
  On Error: the dashboard shows the key and the reason; the factory starts no work
Invariants:
  - No credential value ever appears in this file.
```

### Field Propagation Map

| Field | Boundary | Status | Serialized Format | Consumer Parse Rule | Detail |
|-------|----------|--------|-------------------|---------------------|--------|
| roles.*.append | config → prompt | transformed | TOML string | trimmed and appended at the end of every prompt of the role under `# Additional instructions`; omitted when blank | |
| agents.permissions | config → backend adapter | transformed | `auto` or `bypass` | enum; selects the CLI's flags (factory core design, Launch permission profiles) | |
| held.*.paths | config → gate | preserved | glob strings | globs on paths in the diff | |

### Error Handling

| Error Category | Example | Detection | Recovery Strategy | User Impact |
|---------------|---------|-----------|-------------------|-------------|
| Validation | Unknown configuration key | Parser | Start no work; the dashboard names the key | Owner fixes it in the dashboard |
| External | Release download fails or checksum mismatch | HTTP or hash | Keep the current executable | Owner runs the script again |
| External | The latest release cannot be read (no connection, rate limit, no release yet) | HTTP status or decode | Log to `daemon.log`; keep the version read before; read again the next day | No release notice until one is read |
| External | The installed file does not run or prints no version | Exit status or parse | Log; read as nothing installed | A release notice, if any, still shows |

## Implementation Plan

### Implementation Approach

**Selected Approach**: configuration first, then the owner's commands, setup, and the release pipeline.
**Selection Reason**: every other part reads the configuration; the owner can use the factory only once setup exists; the release pipeline depends on nothing else here.

### Technical Dependencies and Implementation Order

1. **Configuration parsing and defaults** — the core's first slice reads it.
2. **`start` and `stop`** — `start` serves and opens the dashboard.
3. **Setup and credentials** — through the dashboard (owner-interface design).
4. **Release pipeline and install script**.

### Migration Strategy

None. The state directory is not versioned; a release that changes its layout decides then how existing state is handled.

## Security Considerations

- **Authentication & Authorization**: credentials only in a 0600 file outside the repository; pull requests authored by the configured token's account.
- **Input Validation**: configuration strictly validated; release archives verified against `SHA256SUMS`, which is fetched over HTTPS from the same release. The latest release's tag and the installed executable's output are used only when they parse as SemVer, and reach the page only as that version.
- **Outbound data**: the latest-release request carries no credential and nothing of the repository; its User-Agent names the running version.
- **Sensitive Data Handling**: no credential in configuration, logs or exports; the dashboard shows each held credential only by its last four characters and never sends the value back.

## Test Boundaries

### Mock Boundary Decisions

| Component/Dependency | Mock? | Rationale |
|---------------------|-------|-----------|
| GitHub Releases | Yes (local fixture server) | Deterministic download and checksum tests |
| Latest release API | Yes (a `Transport` answering a tag) | The application-owned HTTP adapter; the rest of the read is real |
| Installed executable | No (the built executable, then a file moved over it as the install script does) | Its `--version` output and its replacement are the behavior under test |
| Configuration directory | No (temporary directory) | Writing the credentials file is the behavior under test |

### Data Layer Testing Strategy

- **Schema dependencies**: configuration schema.
- **Test data approach**: temporary repositories, state and configuration directories.
- **Mock limitations acknowledged**: none.

### Integration Verification Points

- The first `start` in a fresh repository opens setup and changes nothing in the repository until setup is saved; saving it changes only `.ovenspring/`.
- The install script with a corrupted archive keeps the installed executable.
- `GET /api/v1/version` offers a newer release while the built executable is installed, then a restart once a newer file is moved over it (`crates/cli/tests/dashboard_int.rs`). Which update comes first and SemVer precedence are unit tests (`crates/server/src/version.rs`); the header's two notices are in the fixture e2e lane (`dashboard/e2e/header.fixture.e2e.test.ts`).

## Verification Strategy

### Correctness Proof Method

- **Correctness definition**: install, start, setup, stop and update behave as the ACs state on macOS and Linux.
- **Verification method**: integration tests above; a manual install from a release candidate on each platform.
- **Verification timing**: integration tests with each change; a manual install from a release candidate before each release.

### Early Verification Point

- **First verification target**: `start` on a sandbox repository, setup saved in the dashboard.
- **Success criteria**: the repository's diff is only `.ovenspring/`; the daemon reads the saved configuration.
- **Failure response**: correct the step before building on it.

### Output Comparison

N/A — new behavior.

## Design Boundaries

- **Intentional limitations**: macOS and Linux only; one daemon per repository per machine, each with its own dashboard.
- **Existing extension points used**: `gh` authentication as an optional source for the GitHub token.

## Risks and Mitigation

None identified.

## References

- PRD, ADR-0001, ADR-0003, ADR-0005
- Semantic Versioning 2.0.0: https://semver.org/

## Update History

| Date | Version | Changes | Author |
|------|---------|---------|--------|
| 2026-11-21 | 1.0 | Initial version | Oven Spring team |
| 2026-11-25 | 1.1 | Reconciled with the implementation: the full configuration, setup, the dashboard address, CI, validation | Oven Spring team |
| 2026-11-25 | 1.2 | Version and updates (AC-071): the running, installed and latest versions in the dashboard's header; the daemon reads its executable's path once, at start | Oven Spring team |
