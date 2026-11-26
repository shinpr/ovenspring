# Issue Filter Design Document

## Overview

The owner names a command in `[tracker] filter`; the daemon runs it on each new or changed tracker candidate and publishes to `sources/issues/` only what the command admits or cannot judge. Right after the owner first sets or changes the command, a candidate the new command has not judged yet is published as without a filter. Intake for an issue ends when the lead includes it in a sprint proposal: from then on its snapshot is frozen whether or not a filter is set, and while the lead proposes no snapshot changes at all. This document owns the tracker intake rules: `docs/design/factory-core-design.md` (Data Flow; Selected Design, Tracker; the `sprint-plan` filing and the proposal after an empty one) points here and describes what reaches agents as the issues intake publishes, and `docs/design/distribution-design.md` (Configuration) lists the optional `[tracker] filter` key whose contract is here. The rest of those designs is unchanged.

## Background and Context

### Prerequisite ADRs

- `docs/adr/ADR-0005-file-board-and-state-outside-repository.md`: snapshots stay files under the state directory's `sources/issues/`; events stay JSON Lines in `events.jsonl`. The filter adds no persisted record.
- `docs/adr/ADR-0007-daemon-owns-external-writes.md`: only the daemon reads the tracker; the filter runs in the daemon, and nothing about a drop or failure is written to Slack or the tracker.
- `docs/adr/ADR-0001-rust-single-binary.md`: the filter run is plain Rust process handling in the daemon crate; no new runtime.

### External Resources Used

| Resource (project-tier label) | Feature-specific identifier | Notes |
|-------------------------------|-----------------------------|-------|
| Tracker API | GitHub REST `GET /repos/{owner}/{repo}/issues?state=open`, field `user.login` of each issue | Already requested; the field is only newly read |
| Tracker API | Linear GraphQL `issues` query `OPEN_ISSUES`, field `creator { name }` of `Issue` (schema: `creator: User`, nullable: "Null if the creator's account has been deleted or if the issue was created by an integration or system process") | One field added to the existing query in `crates/integrations/src/tracker/linear.rs` |

### Requirement Convergence

- **Outcome**: N/A — covered by PRD `docs/prd/oven-spring-prd.md` (AC-072 to AC-075, Success Criteria 3)
- **Non-Goals**: N/A — covered by PRD `docs/prd/oven-spring-prd.md` (Future / Out of Scope rows for the filter)
- **Left open by the owner**: None. The owner settled when intake ends (`sources/answers/NOR-32-spec-2.md`: "An issue leaves intake when the lead includes it in a sprint proposal. From then on, use the content the lead selected.") and approved the PRD (`sources/answers/NOR-32-prd-3.md`).

### Standards and Assumptions

#### Applicable Standards

- [x] Rust 2024, `cargo fmt`, `cargo clippy --all-targets -D warnings` `[explicit]` - Source: `rustfmt.toml`, `Cargo.toml` `[lints]`, `lefthook.yml`
- [x] Background work runs on a thread whose `JoinHandle` the daemon polls each tick; the daemon's own state changes only on its thread `[implicit]` - Evidence: `crates/daemon/src/product_run.rs:100-110` (`product_check` handle checked with `is_finished`), `crates/daemon/src/recovery.rs` (procedure thread drained in `tick`) - Confirmed: Yes
- [x] A process the daemon starts runs in its own process group and is stopped as a group `[implicit]` - Evidence: `crates/daemon/src/lib.rs` `spawn` (`process_group(0)`), `crates/daemon/src/app.rs` `stop_group`, `crates/daemon/src/lib.rs` `stop_launch` (`stop_group(run.pid)`) - Confirmed: Yes
- [x] Test durations go through `Timing`; owners cannot set them `[implicit]` - Evidence: `crates/daemon/src/lib.rs:82-108`, `crates/cli/tests/support/mod.rs` `QUICK` - Confirmed: Yes

#### Assumed Behaviors

- [x] GitHub's issues list returns the opener as `user.login` on each issue - Evidence: GitHub REST "List repository issues" response schema; comments already read `user.login` (`crates/integrations/src/tracker/github.rs:77`) - Design effect: GitHub `author` needs no new request - Confirmed: Yes
- [x] Linear's `Issue` has `creator: User` (nullable) and `User.name: String!` ("The user's full name.") - Evidence: Linear SDK schema `packages/sdk/src/schema.graphql`, read 2026-11-26 - Design effect: Linear `author` is `creator.name`, empty when `creator` is null; this matches the comment authors, which are `user.name` (`crates/integrations/src/tracker/linear.rs:276`) - Confirmed: Yes
- [x] Rust ignores `SIGPIPE`, so writing stdin to a command that exits without reading it returns an error instead of ending the daemon - Evidence: Rust runtime sets `SIGPIPE` to ignore on Unix before `main` - Design effect: the stdin write error is ignored; the verdict comes from the exit status - Confirmed: Yes
- [x] A snapshot's first two header lines are `# <key>: <title>` and `- URL: <url>` - Evidence: `TrackerIssue::render` (`crates/integrations/src/tracker/mod.rs:37-41`) - Design effect: sprint start can read the selected title and URL back from the snapshot (Selected Design, Sprint start) - Confirmed: Yes

#### Quality Assurance Mechanisms

- [x] `cargo fmt --check`, `cargo clippy --all-targets -D warnings` — Enforces: format, lints — Config: `rustfmt.toml`, `Cargo.toml` — Covers: `crates/**` — Status: `adopted`
- [x] `cargo test` — Enforces: unit, contract and integration tests — Covers: `crates/**` — Status: `adopted`
- [x] Generated dashboard types — Enforces: `dashboard/src/api/types.gen.ts` matches `Config` — Config: `crates/server/src/types.rs` (rewrite with `OVENSPRING_WRITE_TYPES=1 cargo test -p ovenspring-server`) — Covers: `TrackerConfig` — Status: `adopted`
- [x] `cargo deny check` — Enforces: dependency licenses and advisories — Config: `deny.toml` — Covers: the daemon gaining `sha2`, already used by `crates/store` and `crates/agents` — Status: `adopted`

### Problem to Solve

Every open issue, with all its comments, is written to `sources/issues/<key>.md` on each tracker poll, and the sprint plan and every later step read it. On GitHub Issues that text can come from anyone. The owner needs their own deterministic command to keep such text away from every agent, while issues it admits or cannot judge flow as before.

### Current Challenges

- `poll_tracker` (`crates/daemon/src/issues.rs:1130-1169`) writes every open issue's snapshot before anything could judge it, and rewrites snapshots of issues the factory already works on, so a proposed issue's text can change after the lead chose it.
- `forget_closed_sources` keeps a snapshot only for an unfinished issue record, so an issue proposed but not yet approved loses its snapshot when closed on the tracker.
- `start_sprint_issues` takes the title, and `begin_issue` the URL, from `open_issues`, the latest tracker read, not from the content the lead selected; the title reaches the requirements request (`request_text`, `crates/daemon/src/issues.rs:623`).
- `IssueOpened` has a transition only in `WaitingForIssues` (`crates/core/src/sprint.rs:194-199`): an issue published while the lead proposes, after it read the directory, prompts no new proposal when that proposal is empty.
- `TrackerIssue` has no issue author.

### Requirements

#### Functional Requirements

PRD AC-072, AC-073, AC-074, AC-075; AC-015 and AC-059 as they name the filter.

#### Non-Functional Requirements

- **Responsiveness**: the filter never runs on the daemon's thread; the daemon keeps its existing half-second loop while a command runs up to its limit - Source: the owner's answer `NOR-32-spec-2.md` ("The command runs while other work continues"); `docs/design/factory-core-design.md` (Responsiveness)
- **Command time limit**: 60 seconds per issue, fixed - Source: AC-072
- **Judgment throughput**: one command run at a time - Source: selected in Design Convergence because the requirements leave it unset; very large numbers of open issues are out of scope (PRD Future / Out of Scope)

## Acceptance Criteria (AC) - EARS Format

### The command's contract (AC-072)

- [ ] **When** the filter runs for an issue, the system shall run `sh -c <filter>` with the owner's checkout (`Paths::repo`) as working directory and write to its stdin one JSON object with exactly `key`, `title`, `body`, `url`, `labels`, `author`, and `comments` (each `{author, body}`)
- [ ] **If** the command exits 0, **then** the system shall admit the issue
- [ ] **If** the command exits 2, **then** the system shall drop the issue with the first line of its stdout as the reason, exactly as written apart from its line terminator (`\n` or `\r\n`): surrounding whitespace and its full length kept
- [ ] **If** the command exits with any other status, ends by a signal, cannot start, or has not exited and closed its stdout 60 seconds after it started, **then** the system shall count a failure, stop the command's process group when it is still running, and let the issue go on as without a filter
- [ ] **When** the daemon stops normally while a filter command runs, the system shall stop that command's process group

### Intake (AC-072, AC-073)

- [ ] **When** the command drops a candidate, the system shall remove its snapshot, leave it out of `Daemon::open_issues`, fire no `IssueOpened` for it, and record one `filter-dropped` event with `issue` (key), `url` and `reason`, the reason recorded as the command wrote it except that each credential the daemon held at any moment from the command's start until its result was taken, and each value in a token format the downloads mask, is replaced by `[masked]`
- [ ] **If** the command's reason contains a credential the daemon held at any moment from the command's start until its result was taken, **then** the system shall still drop the candidate and record its `filter-dropped` event, and `events.jsonl` shall not contain that credential, even when the credential is replaced while the command runs or before the event is recorded
- [ ] **When** the command fails on a candidate, the system shall publish the candidate as without a filter and record one `filter-failed` event with `issue` (key) and `error`
- [ ] **While** no filter is configured, the system shall publish every candidate without running anything
- [ ] **When** a candidate's stdin JSON is identical to the one last judged by the configured command, the system shall not run the command for it again, whatever the last verdict was
- [ ] **When** the owner first sets the filter or changes its command, the system shall judge every open candidate once under the new command, and until a candidate is judged publish its current content as without a filter, an issue the earlier command dropped included
- [ ] **When** the owner removes the filter, the system shall publish every candidate
- [ ] **While** a filter is configured and a candidate is not awaiting its first judgment after a change of command (the command was already set when the daemon started, the candidate first appeared after the change, or the new command has judged it), the system shall leave the candidate's snapshot as it was until its new or changed content is judged: absent when it had none, the earlier content when it had one
- [ ] **When** the daemon starts, the system shall judge every open candidate once before publishing any candidate that has no snapshot
- [ ] **When** the tracker no longer reports an issue open, the system shall forget its verdict, so an issue reported open again is judged again
- [ ] **If** the configured filter changed, the issue left the tracker's open list (even if reopened), or its content changed while a command judged it, or the issue is selected when the result arrives, **then** the system shall discard that result: no verdict, no publication change and no event come from it
- [ ] The system shall start a filter run only for an issue that, when the run starts, is a candidate whose current content has no verdict under the configured filter

### End of intake (AC-074, AC-015)

- [ ] **While** an issue is selected (its key is in the current sprint state's plan, or an issue record exists for it), the system shall neither rewrite nor remove its snapshot because of tracker edits and shall not run the filter for it, with or without a filter configured
- [ ] **When** a revised proposal leaves out an issue the earlier proposal included and no issue record exists for it, the system shall treat it as a candidate again
- [ ] **While** the sprint is in `Proposing`, the system shall neither write nor remove any snapshot
- [ ] **If** an issue is proposed but not yet begun and the tracker no longer reports it open, **then** the system shall keep its snapshot, and approving the proposal shall begin it
- [ ] **When** the owner approves a proposal, the system shall take each issue's title and URL for its issue record, and so for its requirements request, from the snapshot the lead selected
- [ ] **When** a proposal with no issue ends and intake admitted an issue while the lead was proposing, the system shall publish that issue after the proposal ends and file a new proposal; an issue published before the proposal began shall not file another one

### Configuration and guide (AC-059, AC-075)

- [ ] **When** the owner saves the tracker section in Settings, the system shall keep `[tracker] filter` as the file had it
- [ ] The guide shall state the command's contract, that the reason is recorded in the factory's event log with credentials masked, and give a short example wrapper

## Existing Codebase Analysis

### Implementation Path Mapping

| Type | Path | Description |
|------|------|-------------|
| Existing | `crates/integrations/src/tracker/mod.rs` | `TrackerIssue` gains `author: String`; `render()` unchanged; gains `heading(key, snapshot)` that reads back the title and URL `render()` wrote |
| Existing | `crates/integrations/src/tracker/github.rs` | `RawIssue` gains `user: Option<RawLogin>`; `author` = login, empty when absent |
| Existing | `crates/integrations/src/tracker/linear.rs` | `OPEN_ISSUES` gains `creator { name }`; `RawIssue` gains `creator: Option<Named>`; `author` = name, empty when null |
| Existing | `crates/core/src/config.rs` | `TrackerConfig` gains `filter: Option<String>` |
| Existing | `crates/server/src/setup.rs` | Setup's `TrackerConfig` literal sets `filter: None` |
| Existing | `crates/cli/src/daemon_cmd.rs` | `services_for` fills `Services::secrets` with the held credentials and the GitHub token it used |
| Existing | `dashboard/src/api/types.gen.ts` | Regenerated |
| New | `crates/daemon/src/intake.rs` | Selection, verdicts, publication pass, the filter run |
| Existing | `crates/daemon/src/issues.rs` | `poll_tracker` and `forget_closed_sources` move to `intake.rs` and are reshaped there; `begin_issue` no longer reads `open_issues` |
| Existing | `crates/daemon/src/sprint.rs` | `start_sprint_issues` takes title and URL from the snapshot; `sprint_fact` runs the publication pass when the sprint leaves `Proposing` |
| Existing | `crates/daemon/src/lib.rs` | `Services` gains `secrets: Vec<String>`; `Daemon` gains the intake state; `replace_services` adds the new `secrets` to a held filter run's; `Timing` gains `filter_timeout`; `tick` takes a finished filter run; `reload_config` handles a filter change; `stop_launches` stops a running filter group |
| Existing | `crates/daemon/Cargo.toml` | Adds `sha2 = "0.11.1"` (the version `store` and `agents` use) |
| Existing | `docs/GUIDE.md` | Configuration example gains `filter`; new section under Safeguards |
| New | `crates/cli/tests/intake_int.rs` | Daemon-level intake tests |

### Integration Points

- **Integration Target**: `Daemon::tick` (`crates/daemon/src/lib.rs:430-470`), `Daemon::reload_config`, `Daemon::sprint_fact`, `Daemon::stop_launches`, `Daemon::replace_services`
- **Invocation Method**: after the halt-file check, each tick takes a finished filter run; on the existing `tracker_poll` interval, reads the tracker, at once after a filter change in `reload_config` (which clears `last_tracker_poll`); each of these, the read whether or not it succeeds, and the sprint leaving `Proposing` in `sprint_fact` run the publication pass, which also starts the next filter run when none is running

### Code Inspection Evidence

| File/Function | Relevance |
|---------------|-----------|
| `crates/daemon/src/issues.rs:Daemon::poll_tracker` | Current intake; replaced by the read plus publication pass |
| `crates/daemon/src/issues.rs:Daemon::forget_closed_sources` | Keep-condition extended to proposed-but-not-begun keys; not run while `Proposing` |
| `crates/daemon/src/issues.rs:Daemon::begin_issue` | Reads `open_issues` for the URL today; the URL moves to `start_sprint_issues` |
| `crates/daemon/src/issues.rs:request_text` | `Step::Requirements` request carries `record.title` |
| `crates/daemon/src/sprint.rs:start_sprint_issues` | Reads `open_issues` for the title today; reads the snapshot instead |
| `crates/daemon/src/sprint.rs:file_sprint_step` | `sprint-plan` reads the live `sources/issues/` directory during `Proposing` |
| `crates/daemon/src/sprint.rs:sprint_fact` | Every sprint stage change passes here, with `from` and `next.stage` |
| `crates/core/src/sprint.rs:next` | `state.plan` is set on `PlanDone`, kept through `Replied` until the next `PlanDone`; `IssueOpened` acts only in `WaitingForIssues` |
| `crates/daemon/src/product_run.rs:100-110` | `JoinHandle` polled with `is_finished`, the pattern the filter run uses |
| `crates/daemon/src/lib.rs:stop_launches` | Normal stop path (`crates/cli/src/daemon_cmd.rs:263-265`); gains the filter group |
| `crates/daemon/src/app.rs:stop_group` | Group stop reused for the time limit and the daemon stop |
| `crates/daemon/src/overview.rs:open_issues` | Dashboard count follows snapshot files; no change needed |
| `crates/daemon/src/download.rs:147-149` | Events with `issue` join that issue's download; no change needed |
| `crates/daemon/src/download.rs:mask` | Masks held values and token formats; reused unchanged for the reason before it is logged |
| `crates/cli/src/daemon_cmd.rs:services_for`, `connection_key` | Builds `Services` from the held credentials (and the signed-in `gh` token when none is held) and rebuilds them when the credentials file changes |
| `docs/prd/oven-spring-prd.md` (Security: "Credentials never enter the repository, logs, downloads, or prompts"); `docs/design/factory-core-design.md` (Logging and Monitoring) | The reason is owner-command text written to `events.jsonl`, a log |
| `crates/server/src/settings.rs:287-293` | Tracker save mutates the read config; `filter` is kept with no change |
| `factory/steps/facts.md` (Inputs) | The facts step reads `events.jsonl`, so the drop reason reaches it |

### Fact Disposition Table

| Fact ID | Focus Area | Disposition | Rationale | Evidence |
|---------|------------|-------------|-----------|----------|
| crates/daemon/src/issues.rs:Daemon::poll_tracker | Snapshot publication, open-issue list and IssueOpened are written in one synchronous pass per poll | transform | The poll only reads; publication becomes a pass over the last tracker read, the verdicts, the provisional keys and selection, run after each tracker poll (a failed read included, over the last successful read; a filter change makes a poll due), each finished filter run, and the sprint leaving `Proposing`; `open_issues` holds only issues with a published snapshot; `IssueOpened` fires when it gains a key (Selected Design, Publication pass) | crates/daemon/src/issues.rs:1138-1152; crates/daemon/src/lib.rs:456-462 |
| crates/daemon/src/issues.rs:poll_tracker+forget_closed_sources (selected issues) | Freezing intake for selected issues (AC-074) | transform | Selected = key in the current sprint state's plan or with any issue record; selected snapshots are never rewritten, removed by a verdict, or judged; a proposed key with no record keeps its snapshot when closed. A key with a finished record that the tracker still reports open is selected (it was in an approved proposal; `begin_issue` never restarts it), so it is not judged. Sprint start takes title and URL from the frozen snapshot | crates/daemon/src/issues.rs:1138-1140, 1186-1189; crates/core/src/sprint.rs:next (AwaitingApproval + Replied keeps state.plan); crates/daemon/src/sprint.rs:384-415 |
| crates/daemon/src/sprint.rs:file_sprint_step (live sources/issues during planning) | Intake changes while the lead is writing the proposal | transform | While the sprint is in `Proposing`, the publication pass writes and removes no snapshot, closed issues' included; candidates without a snapshot are still judged. The lead's chosen issues keep the text the lead read up to `PlanDone`, when they become selected; what was held is published on the pass that runs as the sprint leaves `Proposing`, so an empty proposal is followed by `IssueOpened` for an issue admitted meanwhile | crates/daemon/src/sprint.rs:121-125 (refs the live directory); crates/daemon/src/sprint.rs:298 (PlanDone taken as is); crates/daemon/src/sprint.rs:396-400 (title fallback); factory/steps/sprint-plan.md:12 |
| crates/integrations/src/tracker/mod.rs:TrackerIssue | Issue author for both trackers | transform | `author` added: GitHub `user.login`, Linear `creator.name`, empty string when absent, the same identity form as each tracker's comment authors. `render()` is unchanged, so admitted snapshots stay byte-identical | crates/integrations/src/tracker/github.rs:38-55 (RawIssue has no user), 77 (comment author = login); crates/integrations/src/tracker/linear.rs:13-24 (OPEN_ISSUES query has no creator), 276 (comment author = user.name) |
| crates/core/src/config.rs:TrackerConfig | The `[tracker] filter` setting and the code that writes the config | transform | `filter: Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`; setup literal sets `None`; Settings save keeps it because it mutates the config it read; generated types regenerated | crates/core/src/config.rs:263-270 (deny_unknown_fields, TS export); crates/server/src/setup.rs:464 (struct literal); crates/server/src/settings.rs:263-293 (save reads the file, mutates kind/team only); crates/server/src/types.rs:1-3, 93 (types.gen.ts must match); dashboard/src/api/types.gen.ts:655 |
| crates/daemon/src/lib.rs:Daemon::tick (blocking work) | Running the owner's command without stalling the event loop | transform | One filter run at a time, each on its own thread whose `JoinHandle` the tick polls; the daemon chooses each run from its current state as it starts it, so no queued job outlives a state change; the time limit and the daemon's normal stop stop the whole process group with `stop_group` | crates/daemon/src/lib.rs:430-470; docs/design/factory-core-design.md:222 (half-second loop, 5-second responsiveness target); crates/daemon/src/recovery.rs:170-193 and take_recoveries (thread + mpsc drained each tick); crates/daemon/src/product_run.rs:100-110 (JoinHandle polled each tick); crates/daemon/src/lib.rs:916-938 spawn (process_group(0)); crates/daemon/src/app.rs:250-268 stop_group (TERM then KILL to the process group) |
| crates/daemon/src/lib.rs:Daemon (in-memory state, restart) | What the daemon remembers and what a restart or a command change re-checks | transform | Verdicts and provisional keys live only in memory. A start has no verdicts, so every open candidate is judged once; a candidate with no snapshot (a dropped one among them) is judged before it is published, one with a snapshot stays published until judged. A filter change while running makes the current candidates provisional instead (published as without a filter until judged) | crates/daemon/src/lib.rs:160-190 (in-memory fields: open_issues, tracker_read, config_text); crates/daemon/src/lib.rs:280-297 (reload_config); sources/issues/*.md persist across restarts (store) |

## Design

### Change Impact Map

```yaml
Change Target: tracker intake (crates/daemon/src/issues.rs:poll_tracker, forget_closed_sources)
Direct Impact:
  - crates/integrations/src/tracker/{mod,github,linear}.rs (TrackerIssue.author, TrackerIssue::heading)
  - crates/core/src/config.rs (TrackerConfig.filter)
  - crates/daemon/src/intake.rs (new), issues.rs (begin_issue), sprint.rs (start_sprint_issues, sprint_fact), lib.rs (Daemon fields, Timing, tick, reload_config, stop_launches)
  - crates/server/src/setup.rs, dashboard/src/api/types.gen.ts
  - crates/cli/src/daemon_cmd.rs (services_for fills Services.secrets)
  - docs/GUIDE.md
Indirect Impact:
  - Daemon::open_issues now holds only published issues; its titles and URLs are no longer read to begin issues
  - Snapshots of selected issues no longer refresh on tracker edits, with or without a filter (AC-074)
  - While Proposing, no snapshot is written or removed (with or without a filter); what was held is published when the sprint leaves Proposing
  - Every TrackerIssue, Timing and Services struct literal in tests gains the new field
No Ripple Effect:
  - TrackerIssue::render output and the snapshot path
  - factory/ step and role files (sprint-plan still reads "every open issue under sources/issues/", which now holds only admitted ones)
  - The prompt and request text the code writes (request_text's wording is unchanged; its title now comes from the snapshot)
  - crates/core/src/sprint.rs (no new transition)
  - The dashboard (its open count follows snapshot files) and downloads (download::mask is reused, unchanged)
  - Slack and tracker writes
```

### Interface Change Matrix

| Existing | New | Conversion Required | Compatibility Method |
|----------|-----|--------------------|--------------------|
| `TrackerIssue { .. }` | `TrackerIssue { .., author }` | Yes | Struct literals add `author`; `Default` gives `""` |
| — | `TrackerIssue::heading(key, snapshot) -> Option<(String, String)>` | No | New associated function beside `render()` |
| `TrackerConfig { kind, team }` | `TrackerConfig { kind, team, filter }` | Yes | Optional field; existing files parse unchanged |
| `Timing { .. }` | `Timing { .., filter_timeout }` | Yes | `Default` = 60 s; test literals set it |
| `Services { github, channel, tracker }` | `Services { .., secrets: Vec<String> }` | Yes | `services_for` fills it; test literals set `Vec::new()` or the values they plant |
| `Daemon::open_issues()` | same signature | No | Now returns issues with a published snapshot |
| `Daemon::begin_issue(key, title, deps)` | same signature | No | No longer sets `url`; `start_sprint_issues` sets it with `sprint` |
| `Daemon::poll_tracker` (issues.rs) | `Daemon::poll_tracker` (intake.rs) | No | Same call site in `tick` |

### Architecture Overview

```
tick ──(after halt check)──► take_filter_run: run finished? → keep or discard result → pass
   └─(every tracker_poll)──► poll_tracker: tracker.open_issues() → retain (started by others out)
                                         → hash each; forget verdicts/provisional of keys not open;
                                           void the run if its key is gone → pass
reload_config ──(filter differs)──► clear verdicts, void the run, set provisional → pass
sprint_fact ──(stage leaves Proposing)──► pass
pass = publish (unless Proposing) → cleanup (unless Proposing) → open_issues/IssueOpened → start next run
run thread: judge(child, stdin JSON, limit) → Outcome (returned through its JoinHandle)
```

### Data Flow

```
tracker API JSON → TrackerIssue (+author)
TrackerIssue → FilterInput JSON (key,title,body,url,labels,author,comments) → sha256 → content hash
             → command stdin → exit status / first stdout line → Outcome
Outcome Dropped(reason) → (taken) download::mask(reason, FilterRun.secrets) → verdict → (pass) filter-dropped event → events.jsonl
TrackerIssue + verdict + provisional + selection + sprint stage → render() → sources/issues/<key>.md (written, kept, or removed)
sources/issues/<key>.md (selected) → TrackerIssue::heading → IssueRecord.title, .url → requirements request
```

### Integration Points List

| Integration Point | Location | Old Implementation | New Implementation | Switching Method | Verification Method |
|-------------------|----------|-------------------|-------------------|------------------|-------------------|
| Tracker poll | `Daemon::tick` → `poll_tracker` | write every snapshot | read, then pass | Code replacement | `intake_int.rs`; existing `sprint_int.rs` |
| Filter run result | `Daemon::tick` | N/A | `take_filter_run` after halt check | New call | `intake_int.rs` |
| Filter change | `Daemon::reload_config` | replaces config | also clears verdicts, voids the run, sets provisional keys and clears `last_tracker_poll` when `tracker.filter` differs; the pass runs from the poll past the halt check | Code addition | `intake_int.rs` (transition) |
| Leaving `Proposing` | `Daemon::sprint_fact` | saves and acts | also runs the pass when `from` is `Proposing` and `next.stage` is not | Code addition | `intake_int.rs` (empty proposal) |
| Sprint start | `start_sprint_issues` | title from `open_issues`, URL in `begin_issue` | title and URL from the snapshot | Code replacement | `intake_int.rs` (selected content) |
| Daemon stop | `Daemon::stop_launches` | launches and apps | also the running filter group | Code addition | `intake_int.rs` (stop) |
| Config | `TrackerConfig` | kind, team | + filter | Serde field | `core` config test; server settings test |

### Main Components

#### Intake (`crates/daemon/src/intake.rs`)

- **Responsibility**: decide, for each issue the tracker reports, whether its snapshot is written, kept or removed; which content needs the filter and when its run starts; which run results count; run the filter.
- **Interface**: `Daemon::poll_tracker`, `Daemon::take_filter_run` (both `pub(crate)`, called from `tick`), `Daemon::filter_changed` (called from `reload_config`), `Daemon::intake_pass` (called from the above and from `sprint_fact`), `Daemon::stop_filter` (called from `stop_launches`), `judge(child, input, limit) -> Outcome` (free function, unit-tested).
- **Dependencies**: `Tracker`, `Store` (`write_source`, `source`), `records::issue`, `Daemon::sprint`, `app::stop_group`, `download::mask`, `Services::secrets`, `sha2`.

#### State on `Daemon`

```rust
/// The open issues the tracker last reported, after issues started by others are left out,
/// each with the hash of its filter input.
tracker_issues: Vec<(TrackerIssue, [u8; 32])>,
/// The last verdict per key, for the content hash it judged, under the filter configured now.
verdicts: HashMap<String, Judged>,      // Judged { hash, outcome: Outcome (a Dropped reason already masked), logged: bool }
/// Candidates published as without a filter until the command set or changed while running judges them.
provisional: HashSet<String>,
/// The one filter run under way.
filter_run: Option<FilterRun>,          // FilterRun { key, hash, url, pid: Option<u32>, void: bool, secrets: Vec<String> /* held since start */, handle: JoinHandle<Outcome> }
```

`open_issues` stays, holding the published subset (see Publication pass).

### Selected Design

**Selection.** `selected(key)` is true when the current sprint state (`records::sprint`) has a plan whose issues include `key`, or `records::issue(key)` exists. Both are persisted today; nothing new is stored. A revised proposal replaces `state.plan` on `PlanDone`, so a left-out key without a record becomes a candidate again with no extra code. A candidate is an issue the tracker reports open (after the existing "started by someone else" filter) that is not selected.

**Content hash.** At each tracker read the daemon serializes each issue's `FilterInput` (Data Contracts) with `serde_json::to_string` and hashes it with SHA-256. The same bytes go to the command's stdin, so "unchanged" means exactly "the command would read the same input".

**Verdict lookup.** `verdict(issue)` is:
- `Admitted` when no filter is configured (nothing runs);
- the stored outcome when `verdicts[key].hash` equals the current hash;
- none otherwise (the content needs judging).

**Filter change** (`filter_changed`, from `reload_config` when `tracker.filter` of the old and new config differ). `verdicts` is cleared and a run under way is voided (`void = true`). When the new setting names a command (first set, or changed), `provisional` becomes the keys of the current candidates; when the filter is removed, `provisional` is cleared and every candidate is `Admitted`. It also clears `last_tracker_poll`, and runs nothing itself: `reload_config` runs before the halt-file check, so a pass here would start a command, write snapshots and fire `IssueOpened` while the factory is paused (AC-034). The poll this makes due runs on the first tick past the halt check, at once when the factory is not paused, and its pass applies the change whether or not that read succeeds (Publication pass), so a filter removed while paused publishes the candidates it dropped even when the first read after the pause fails. A candidate that first appears after the change is not provisional and follows the steady rule. This is the transition the owner described ("until the command judges a candidate, handle it as before"); it applies only to a change the running daemon sees (Start and restart).

**Publication pass** (`intake_pass`), run after each tracker poll, a failed read included (the pass then works on the last successful read, which the failure leaves in place), after each finished filter run (kept or discarded), and when the sprint leaves `Proposing` (in `sprint_fact`, after the fact's actions, when `from` is `Proposing` and `next.stage` is not; an `IssueOpened` the pass fires moves the sprint out of `WaitingForIssues`, not out of `Proposing`, so the pass does not run itself again):

1. **Publish**, skipped entirely while the sprint is in `Proposing`, so the directory the lead reads does not change until `PlanDone` makes its chosen issues selected. Otherwise, for each issue in `tracker_issues`:

| Issue | Snapshot | Judged? |
|---|---|---|
| selected | left as it is | no |
| candidate, verdict `Admitted` (no filter included) or `Failed` | written with `render()` (as today) | no |
| candidate, verdict `Dropped` | removed if present | no |
| candidate, no verdict, key in `provisional` | written with `render()` (as without a filter) | yes |
| candidate, no verdict, not provisional | left as it is (absent stays absent; existing keeps its earlier content) | yes |

   When the pass acts on a `Dropped` or `Failed` verdict whose `logged` is false, it records `filter-dropped` or `filter-failed` and sets `logged`; the snapshot is removed before the event is recorded, and a failed event write leaves `logged` false so the next pass records it. The `filter-dropped` reason is the verdict's reason as stored, already masked when the run was taken (Taking a run), so a credential replaced while the verdict waits for the pass, for example until the sprint leaves `Proposing`, is still masked. A verdict the pass never acts on (its key became selected first, or its content changed) records nothing, so each event describes a decision intake took.
2. **Cleanup**, skipped while `Proposing`: `forget_closed_sources` with its keep-condition extended: a snapshot is kept when the key is open, has an unfinished record (today), or is in the current plan with no record yet (proposed, not begun).
3. **Open list**: `open_issues` becomes the issues in `tracker_issues` whose snapshot exists; the `tracker-issues` event is logged when that key list changes and `IssueOpened` fires when it gains a key, as today. An issue admitted while the lead proposed is therefore published, and fires `IssueOpened`, on the pass that runs as the sprint leaves `Proposing`; after an empty `PlanDone` the sprint is in `WaitingForIssues`, where `IssueOpened` files a new proposal (`crates/core/src/sprint.rs:194-199`). An issue published before the proposal began is already in `open_issues` and fires nothing.
4. **Next run**: when a filter is configured and no run is under way, the first issue in `tracker_issues` that is a candidate, has no verdict, and — while `Proposing` — has no snapshot, starts a run. Eligibility is decided here, on the daemon's thread from its current state, as each run starts; nothing is queued ahead of it. A published candidate's changes wait until the proposal ends, when the issue is either selected (and never judged) or still a candidate.

**Starting a run.** The daemon spawns `sh -c <filter>` (The filter run, step 1) on its own thread, so the run's process group is known (`pid`), and hands the child, or the spawn error, to a new thread that returns the `Outcome`; "could not start" thus comes back through the same handle as every other outcome. It stores `FilterRun { key, hash, url, pid, void: false, secrets, handle }`, `secrets` being a copy of `Services::secrets` as the run starts, to which `replace_services` adds each newly held set while the run is held (Credentials for masking).

**Taking a run** (`take_filter_run`, each tick after the halt check). When `filter_run.handle` is finished, the daemon joins it and keeps the outcome only when the run was not voided, its key is a candidate in the latest read, and its hash equals that candidate's current hash. A kept outcome is stored as `verdicts[key] = Judged { hash, outcome, logged: false }` and the key leaves `provisional`; a `Dropped` reason is stored as `download::mask(reason, run.secrets)`, so every credential value the daemon held at any moment from the command's start until its result is taken (the current `Services::secrets` among them), and every token format the downloads mask, becomes `[masked]`, any other text staying as the command wrote it. The unmasked reason is dropped with the joined outcome and is never stored; a discarded one (a panicked thread included) changes nothing and its content is judged again if still current. Either way `filter_run` is cleared and the pass runs, which applies the verdict and starts the next run. A run is voided when the filter setting changes and when a tracker read no longer holds its key, so a command changed away and back, or an issue closed and reopened, while the run was under way, cannot pass off the earlier judgment as the current one (AC-073 requires both to be judged anew). Selection needs no flag: a result arriving while its key is selected is discarded by the candidacy check, and an issue a revised proposal leaves out is a candidate again (AC-074), whose unchanged content (same hash) under the same command is exactly what that run judged, so its verdict is the candidate's current one and applies as any other.

**Forgetting.** After each successful read, `verdicts` and `provisional` keep only keys in the read.

**Start and restart.** `verdicts` and `provisional` start empty, so the daemon's first reads judge every open candidate once: published snapshots stay until judged, and a candidate without a snapshot (a dropped one) is judged before being published. A command edited while the daemon was stopped is handled the same way: the daemon keeps no record of the earlier command (holding state across restarts is out of scope), and publishing snapshot-less candidates provisionally at start would readmit every issue dropped before the restart, which AC-073 has checked once more instead.

**Sprint start reads the selected content.** `start_sprint_issues` takes each planned issue's title and URL from `sources/issues/<key>.md` through `TrackerIssue::heading(key, snapshot)`, which reads back the `# <key>: <title>` and `- URL: <url>` lines `render()` writes; it lives beside `render()` so the format has one owner. The snapshot is the content the lead read and intake has kept since, so the issue record, and the requirements request built from `record.title`, carry the selected text even when the tracker's title changed or the issue closed in between. `start_sprint_issues` sets `url` in the `update_issue` call that already sets `sprint`, and `begin_issue` no longer reads `open_issues`. A missing or unreadable snapshot falls back as today: the key as title, no URL.

**Daemon stop.** `stop_launches` calls `stop_filter`, which stops the process group of a run whose handle has not finished (`app::stop_group(pid)`). Nothing is queued to start after it, and the next start judges again.

**The filter run**:
1. `Command::new("sh").arg("-c").arg(command).current_dir(repo)`, stdin and stdout piped, stderr null, `process_group(0)`. A spawn error is `Failed("could not start: <error>")`.
2. A thread writes the stdin JSON and closes stdin; a write error (the command exited or closed stdin) is ignored.
3. A thread reads stdout to its end, keeping the bytes before the first `\n`, whether an `\n` was found, and discarding the rest.
4. The caller waits, polling `try_wait`, until the command has exited and stdout has closed, or until `Timing::filter_timeout` (60 s) after the start. At the limit it calls `stop_group(pid)`, reaps the child, and returns `Failed("ran longer than 60 seconds")`.
5. Exit 0 → `Admitted`; exit 2 → `Dropped(reason)`, the kept bytes with one `\r` removed from their end only when an `\n` followed it (the rest of a `\r\n` terminator; stdout that ends in `\r` with no `\n` keeps it), decoded as UTF-8 with invalid bytes replaced (the event is JSON text), otherwise unchanged and possibly empty; other status → `Failed("exited with status N")`; signal → `Failed("ended by signal N")`.

**Credentials for masking.** `Services` gains `secrets: Vec<String>`: `services_for` (`crates/cli/src/daemon_cmd.rs`) fills it with `held.values()` and, when no GitHub token is held, the signed-in `gh` token it used for the GitHub API. `Services` is rebuilt and handed over with `replace_services` whenever the credentials file changes (`connection_key`, `crates/cli/src/daemon_cmd.rs` reload loop), which can happen while a run is under way or while its verdict waits for the pass. The run therefore keeps a copy of the values held as it started, and `replace_services`, when `filter_run` is `Some`, appends the new `Services::secrets` to `filter_run.secrets` before installing them; so from the run's start until it is taken, `filter_run.secrets` holds every value the daemon held, the current ones included, and the reason is masked with that list alone (Taking a run). Only the masked reason is kept, so a replacement after the take, while the verdict waits for the pass, cannot expose it. No file is read and no error path is added at logging. A value written to the credentials file and replaced again before the reload loop reads the file is never held by the daemon and cannot be masked (Design Boundaries).

**Revisions to existing code.** `poll_tracker` and `forget_closed_sources` move from `issues.rs` to `intake.rs`, which owns all intake. `issues.rs` keeps issue records and steps. `begin_issue` and `start_sprint_issues` stop reading `open_issues`, which is left to intake.

#### Added design surface

- **Addition**: `[tracker] filter` setting
  - **Current evidence**: AC-072 (owner's words in NOR-32)
  - **Lower-surface insufficiency**: the owner names the command; nothing else holds it
  - **Subtraction result**: AC-072 unmet
- **Addition**: `TrackerIssue.author`
  - **Current evidence**: AC-072 stdin field list; NOR-32 "added for both trackers"
  - **Lower-surface insufficiency**: no existing field carries the opener
  - **Subtraction result**: stdin JSON contract unmet
- **Addition**: in-memory `verdicts` with SHA-256 content hash
  - **Current evidence**: AC-073 (no rerun for unchanged content, dropped issues remembered in memory)
  - **Lower-surface insufficiency**: a snapshot on disk cannot stand for a verdict (a dropped issue has none; `render()` differs from the stdin JSON); keeping verdicts for admitted and failed content too is what stops their reruns
  - **Subtraction result**: Done-when "An unchanged issue, passed or dropped, does not run the command again on the next poll" unmet
- **Addition**: in-memory `provisional` keys
  - **Current evidence**: NOR-32 ("Work continues during the transition; until the command judges a candidate, handle it as before"); AC-073
  - **Lower-surface insufficiency**: without it a candidate pending after a change follows the steady rule (held absent or at its earlier content), which the owner's words exclude; telling a changed command from a restart needs the change the running daemon saw, which only `reload_config` knows
  - **Subtraction result**: AC-073's transition behavior unmet
- **Addition**: one filter run at a time on its own thread, with a void flag
  - **Current evidence**: owner's answer (the command runs while other work continues); 60-second limit (AC-072); 5-second responsiveness (`factory-core-design.md`); AC-073 and AC-074 (a changed command or a reopened issue is judged anew; a selected issue is not judged)
  - **Lower-surface insufficiency**: inline runs block `tick` for up to 60 s per issue. Choosing each run from current state as it starts replaces a batch queue, so a selected issue or a removed filter never starts a run. Comparing state at receipt cannot tell a command changed away and back, or an issue closed and reopened, from no change; the flag set at those two events does, with no counter or table. The polled `JoinHandle` is the existing `product_run.rs` pattern, so no channel is added
  - **Subtraction result**: responsiveness unmet, or an obsolete result counted as the current judgment
- **Addition**: the `Proposing` freeze on all snapshot writes and removals, and the pass when the sprint leaves `Proposing`
  - **Current evidence**: AC-074 ("the work uses the content the lead selected"); AC-015 ("the lead proposes again once intake admits an open issue"); `sprint-plan` reads the live directory (`sprint.rs:121-125`) while polls continue
  - **Lower-surface insufficiency**: freezing only at `PlanDone` lets an edit, drop or close during planning change or remove text the lead chose; publishing new admissions during `Proposing` loses `IssueOpened` when they land after the lead read the directory and the proposal is empty. One rule covers both, and the pass at the stage change publishes what waited with no new sprint transition
  - **Subtraction result**: AC-074 unmet for issues edited, dropped or closed while the lead plans; AC-015 unmet after an empty proposal
- **Addition**: `TrackerIssue::heading`
  - **Current evidence**: owner's answer `NOR-32-spec-2.md` ("use the content the lead selected"); `start_sprint_issues` and `request_text` carry the title to the first agent
  - **Lower-surface insufficiency**: the tracker read holds the latest, unjudged title, and an in-memory copy of the published version is lost on restart while the proposal waits for approval; the snapshot already persists the selected title and URL
  - **Subtraction result**: a later, unjudged title reaches the requirements request
- **Addition**: `Timing::filter_timeout`
  - **Current evidence**: the 60 s limit (AC-072) must be proven without a 60 s test
  - **Lower-surface insufficiency**: `Timing` is the existing test-only seam; owners cannot set it
  - **Subtraction result**: the time-limit proof becomes a 60-second test
- **Addition**: `Services::secrets`, its copy in `FilterRun` extended by `replace_services` while the run is held, and masking the reason with `download::mask` when the run is taken
  - **Current evidence**: PRD Non-Functional Requirements, Security ("Credentials never enter the repository, logs, downloads, or prompts"); `docs/design/factory-core-design.md` (Logging and Monitoring); the reason is the first text from outside the factory's own code that the daemon appends to `events.jsonl`, which the facts step reads directly (`factory/steps/facts.md`, Inputs), before any download masking
  - **Lower-surface insufficiency**: `download::mask` already holds the masking rule and is reused unchanged; masking token formats alone misses a held value with no known prefix, which the daemon can only learn from the credentials it holds; reading the credentials file at each drop would add an I/O error path and could not be planted by an in-process test (`credentials_path` follows the process environment), while `Services` is already rebuilt from exactly those credentials. `Services` can be replaced, more than once, while a run is under way and while its verdict waits for the pass (review finding Conflict-003: masking with the values held at only some moments let a value held between them through), so the run's list collects every set held from its start until the take, the reason is masked at the take with that list, and only that masked text is stored; this needs one list per run, released with it, and no record of credentials beyond the run
  - **Subtraction result**: a held credential printed by the owner's command reaches `events.jsonl` and the facts step's prompt material
- **Addition**: `filter-dropped` and `filter-failed` events, with `Judged.logged`
  - **Current evidence**: AC-072 (event with key, URL, reason; event with key and what failed)
  - **Lower-surface insufficiency**: no existing event carries them; logging at receipt would record drops intake never applied (the key was selected first), so the event is logged once where the pass applies it
  - **Subtraction result**: AC-072 unmet

### Data Contracts

#### Filter command (owner-facing; stated in the guide)

```yaml
Contract: sh -c "<[tracker] filter>"
Input:
  Working directory: the owner's checkout (the directory holding .ovenspring/config.toml)
  Environment: the daemon's environment
  Stdin: one JSON object, UTF-8, no trailing newline:
    {"key": str, "title": str, "body": str, "url": str, "labels": [str],
     "author": str, "comments": [{"author": str, "body": str}]}
    author: GitHub login, or Linear full name; "" when the tracker gives none
    (Linear: an issue created by an integration or a deleted account)
Output:
  Exit 0: admitted
  Exit 2: dropped; first line of stdout (without its \n or \r\n) is the reason, recorded as written except that credentials the factory holds at any moment while the command runs, until its result is taken, and values in common token formats become [masked]
  Anything else, cannot start, or not done within 60 s: failure, the issue goes on
Invariants:
  - Stderr is discarded
  - A command still running at 60 s, or when the daemon stops, is stopped with its whole process group
  - The reason is recorded in the factory's event log (events.jsonl), which the factory's agents read when they collect a sprint's facts; a credential in it is masked first, the drop still applies
```

#### Events (`events.jsonl`, via `records::event`)

```yaml
filter-dropped: {"event": "filter-dropped", "issue": "<key>", "url": "<url>", "reason": "<first stdout line, credentials masked>"}
filter-failed:  {"event": "filter-failed", "issue": "<key>", "error": "could not start: …" | "exited with status N" | "ended by signal N" | "ran longer than 60 seconds"}
```

Each is logged once per verdict, by the pass that applies it. The `issue` field puts them in that issue's download (`download.rs:147-149`).

### Field Propagation Map

| Field | Boundary | Status | Serialized Format | Consumer Parse Rule | Detail |
|-------|----------|--------|-------------------|---------------------|--------|
| `author` | GitHub API → `TrackerIssue` | transformed | `user.login` string | `Option`, `""` when absent | Same as comment authors |
| `author` | Linear API → `TrackerIssue` | transformed | `creator.name` string, `creator` nullable | `Option<Named>`, `""` when null | Same as comment authors |
| `filter` | `config.toml` → `TrackerConfig` | preserved | `[tracker] filter = "<command>"` | serde `Option<String>`; absent = no filter | Unknown keys still refused; listed as optional in `docs/design/distribution-design.md` (Configuration) |
| `key,title,body,url,labels,author,comments` | `TrackerIssue` → command stdin | preserved | JSON object above via `serde_json::to_string` | owner's script | `state`, `started`, `depends_on` not sent |
| first stdout line | command → `filter-dropped.reason` | transformed | bytes before the first `\n`, one trailing `\r` removed only when an `\n` followed it, UTF-8 with invalid bytes replaced, then, at the take, `download::mask` with the run's `secrets` (every set held from the run's start to the take) | — | No trimming or length cut; `reason\r` then end of stdout keeps the `\r`; only credential values and token-format words change |
| `title`, `url` | snapshot → `IssueRecord` | preserved | `# <key>: <title>`, `- URL: <url>` lines of `render()` | `TrackerIssue::heading` | Replaces the read from `open_issues` |

### State Transitions and Invariants

```yaml
Per tracker-open key:
  selected → (never judged; snapshot frozen)
  candidate:
    unjudged(content) → run kept → Admitted | Dropped | Failed (for that content)
    any verdict → content changes → unjudged(new content)
    any → filter set/changed → unjudged and provisional
    any → filter removed → Admitted
    any → daemon start → unjudged
    provisional → verdict kept → not provisional
  candidate → included in plan (PlanDone) → selected
  selected (plan only, no record) → left out of next PlanDone → candidate
  any → no longer reported open → forgotten (a run under way for it is voided)

Filter run: none → started (eligible candidate, chosen at start) → finished → kept | discarded → none
            started → filter change or key gone → voided (finishes, then discarded)
            finished while its key is selected or its content changed → discarded
            started → daemon stop → process group stopped

System Invariants:
  - A candidate's snapshot is written only for content with verdict Admitted or Failed, by no-filter admission, or while provisional
  - No snapshot of a selected key is written or removed by intake
  - While Proposing, no snapshot is written or removed
  - At most one filter command runs at a time, and only for a candidate unjudged when it started
  - Each filter event corresponds to a verdict the publication pass applied
  - No credential value in Services::secrets at any moment from a run's start until its result was taken appears in a filter event, or in a stored verdict
```

### Error Handling

| Error Category | Example | Detection | Recovery Strategy | User Impact |
|---------------|---------|-----------|-------------------|-------------|
| Owner's command | exit 1, cannot start, past 60 s | `judge` outcome | Issue goes on as without a filter; `filter-failed` event; not rerun until the content, the filter setting, or the daemon run changes | Silent here; shown by shinpr/ovenspring#9 |
| Tracker read | transient / other | existing `poll_tracker` arms | Logged as today; verdicts and the last successful read kept; the pass runs over that read, so a filter change waiting on this poll still applies | As today |
| Snapshot write/remove | I/O error | `StoreError` | Propagated to `settle_part`, as today | As today |
| Filter run thread | panics | `join` returns `Err` | Result discarded; the content is judged again by a later run | None |

## Implementation Plan

### Implementation Approach

**Selected Approach**: Vertical slice: the end-to-end drop path first (config → author → judge → run → pass → event), then the transition, freeze, selection and sprint-start rules on the same pass.
**Selection Reason**: The riskiest premise is that asynchronous judgment can sit in front of publication without the lead or `IssueOpened` seeing dropped text; one daemon-level test proves it before the selection rules build on it.

### Technical Dependencies and Implementation Order

1. **`TrackerIssue.author` and `TrackerConfig.filter`** (integrations, core, server setup, generated types)
   - Technical Reason: the stdin JSON and the setting everything else reads
   - Prerequisites / Dependent Elements: none / intake
2. **`judge`** with `Timing::filter_timeout`
   - Technical Reason: the owner's contract; unit-testable alone
   - Prerequisites / Dependent Elements: 1 / run
3. **Intake pass**: verdicts, one run at a time, `take_filter_run` with voiding, publication, events (the reason masked at the take with the run's `secrets`, which `replace_services` extends while the run is held), `filter_changed` with provisional keys, `stop_filter`
   - Technical Reason: the outcome; early verification point
   - Prerequisites / Dependent Elements: 1, 2 / 4
4. **Selection, the `Proposing` freeze, the pass on leaving `Proposing`, the extended keep-condition, and sprint start from the snapshot** (`TrackerIssue::heading`)
   - Technical Reason: AC-074 and AC-015; uses the pass from 3
   - Prerequisites / Dependent Elements: 3
5. **Guide**
   - Technical Reason: states the contract as built
   - Prerequisites / Dependent Elements: 1-4

### Migration Strategy

No data migration. Existing `config.toml` files parse unchanged. On the first start after upgrading, existing snapshots stay; selected issues' snapshots freeze from then on.

## Security Considerations

- **Authentication & Authorization**: N/A — no new entry point. The command is set only by whoever can edit `.ovenspring/config.toml`; a pull request changing it is held for the owner (AC-014).
- **Input Validation**: tracker text reaches the owner's command only on stdin as JSON, never on its command line or in its environment, so issue text cannot change what `sh -c` runs. The verdict is the exit status alone.
- **Sensitive Data Handling**: events hold the key, URL, the command's reason and the failure kind; no issue body or stderr is logged by the factory. The reason is the owner's command's own text, recorded as written (AC-072) except for credentials: when the run's result is taken it passes through `download::mask` with every credential the daemon held from the command's start until that moment, and only the masked text is kept until it is appended to `events.jsonl`, so a credential the command prints (the command runs with the daemon's environment and user) stays out of the raw log, as the PRD (Non-Functional Requirements, Security) and the core design's logging invariant require; the drop and its event still apply. `events.jsonl` is read by the facts step at sprint end (`factory/steps/facts.md`, Inputs), so the guide says so and its example wrapper gives a reason that names the rule rather than quoting the issue. A reason that quotes the issue is the owner's choice under the contract.

## Test Boundaries

### Mock Boundary Decisions

| Component/Dependency | Mock? | Rationale |
|---------------------|-------|-----------|
| Tracker API | Yes (`RecordingTracker`; recorded responses in `tracker_contract.rs`) | External I/O; the adapter is the existing seam |
| The owner's command | No — a real `sh` script written into the test repository | The contract is the process boundary itself |
| Sprint state, issue records, snapshots | No — real store in a temp state directory | Freeze and publication are observed on disk |

### Data Layer Testing Strategy

N/A — no database; files under the state directory are asserted directly.

### Integration Verification Points

- `judge` unit tests in `crates/daemon/src/intake.rs`: stdin equals the expected JSON and `pwd` is the repo (script writes both to a file); exit 0; exit 2 with stdout `"  keep these spaces  \r\nsecond"` (reason = `"  keep these spaces  "`), with stdout `"reason\r"` and no `\n` (reason = `"reason\r"`), and with a 5,000-character first line (kept whole); exit 1; missing interpreter (`sh -c` exit 127 → failure); a non-existent working directory (cannot start); a script that sleeps past a 1-second limit and spawns a child (failure, and the child is gone).
- `TrackerIssue::heading` unit test in `crates/integrations/src/tracker/mod.rs`: reads back title and URL from `render()` of an issue whose title contains `: `.
- `crates/cli/tests/intake_int.rs` with `RecordingTracker` and filter scripts that append their key to a counter file; where ordering matters, the script waits for a gate file the test creates:
  - exit 2 → snapshot absent, key absent from `d.open_issues()`, no `sprint-fact` `IssueOpened` for it, one `filter-dropped` with key, URL, reason (Done-when 1)
  - exit 2 with a reason holding a value planted in `Services::secrets` (generated at test time, no known token prefix) beside ordinary words → snapshot absent, one `filter-dropped` whose reason has `[masked]` in place of the value and the words unchanged, and the raw `events.jsonl` text does not contain the value
  - credentials replaced during the run (Conflict-003): a gated run for a candidate without a snapshot starts with `Services::secrets` holding only A; while it waits for the gate, `replace_services` with `secrets` holding only B, then again with only C; the gate opens and the script exits 2 with a reason holding A and B (generated at test time, no known token prefix) beside ordinary words → snapshot absent, exactly one `filter-dropped` whose reason has `[masked]` in place of A and of B and the words unchanged, and the raw `events.jsonl` text contains neither A nor B
  - credential replaced before the drop is logged (Conflict-003): a sprint in `Proposing`; a gated run for a candidate without a snapshot exits 2 with a reason holding a value A planted in `Services::secrets` beside ordinary words; after the gate opens and the run is taken, `replace_services` with `secrets` holding only B; then the sprint leaves `Proposing` → snapshot absent, exactly one `filter-dropped` whose reason has `[masked]` in place of A and the words unchanged, and the raw `events.jsonl` text does not contain A
  - exit 0 and no filter → snapshot present (Done-when 2)
  - exit 1 → snapshot present and one `filter-failed` (Done-when 3; start failure and timeout are owned by the `judge` unit tests, this test proves the wiring)
  - two polls with unchanged issues, one admitted and one dropped → counter shows one run each; an edit → one more run (Done-when 4)
  - steady filter, a new issue with a gated script → no snapshot at any tick before the gate opens
  - transition: command A drops X; the config changes to gated command B → X's snapshot reappears before B's verdict; B drops X → removed, one `filter-dropped`
  - change while paused: command A drops X; `HALT` is created; the config changes to counter command B → over several ticks B never runs, X has no snapshot and is absent from `d.open_issues()`, and no `sprint-plan` item is filed (AC-034); then the filter is removed, the tracker is made unreachable and `HALT` is removed → on the first tick X's snapshot is present from the last successful read, X is in `d.open_issues()`, and the counter is still empty
  - void: a gated run for X; the config changes A→B→A (or the tracker read drops X then returns it) before the gate opens → no verdict and no event from that run, and X runs once more
  - selection during a run: a gated run for X; a `PlanDone` including X before the gate opens → no event, X not run again, its snapshot unchanged
  - selected then left out during a run: a gated run for X exiting 2; a `PlanDone` including X, the owner's reply, and a revised `PlanDone` leaving X out, all before the gate opens, X's content unchanged → the run's verdict applies to X as a candidate: snapshot removed, one `filter-dropped`, and the counter shows no second run
  - a new `Daemon` on the same state → the dropped issue runs once more before any snapshot appears
  - proposal then tracker edit → the proposed issue's snapshot is byte-identical and the counter shows no run; owner reply then a `PlanDone` leaving it out → it is judged again (AC-074)
  - while `Proposing`, an admitted issue's tracker title changes and the issue closes; `PlanDone` with it, approval → snapshot present, issue record title equals the earlier title, the `requirements` item's request names the earlier title
  - while `Proposing`, a new issue is admitted; an empty `PlanDone` → after the next tick a new `sprint-plan` item is filed; a second empty `PlanDone` with no new admission → the sprint stays in `WaitingForIssues` and no item is filed (AC-015)
  - `stop_launches` while a gated script with a child runs → the script's process group is gone
- `crates/integrations/tests/tracker_contract.rs`: fixtures gain `user.login` (GitHub) and `creator { name }` (Linear, plus one `null`); assertions on `author`.
- `crates/core` config test: `[tracker] filter` parses; `crates/server` settings test: a tracker save keeps `filter`; generated types match.

## Verification Strategy

### Correctness Proof Method

- **Correctness definition**: the `filter-dropped` reason equals the command's first line with only held credentials and token-format words replaced by `[masked]`; for the same tracker issues, without a filter or with a command that admits everything, the published snapshots equal today's for candidates outside `Proposing`; selected issues' snapshots, and their records' title and URL, keep the content they had at proposal; with a steady filter, only admitted or failed content is ever written, and dropped content is never present at a tick boundary after its verdict; after a command is set or changed, unjudged candidates are published as without a filter until judged; every filter event matches a verdict the pass applied.
- **Verification method**: daemon-level integration tests at `tick` with the real store and real scripts; unit tests on `judge` and `heading`; existing `sprint_int.rs` passing unchanged except for struct literals.
- **Verification timing**: after step 3 (early point), then after each later step.

### Early Verification Point

- **First verification target**: one `intake_int.rs` test: a new issue, a steady filter script exiting 2, ticks until the run is taken.
- **Success criteria**: no snapshot existed at any tick, `open_issues()` never held the key, no `IssueOpened`, one `filter-dropped` event with key, URL and reason.
- **Failure response**: if publication cannot be kept behind the asynchronous verdict within `tick` (the snapshot or `IssueOpened` appears first), return to this design's Publication pass before building the selection rules.

### Output Comparison

- **Comparison input**: the issue fixtures of `crates/cli/tests/sprint_int.rs` (tracker poll tests around lines 1044-1108) with no filter configured.
- **Expected output fields**: each `sources/issues/<key>.md` text; `open_issues()` keys; `tracker-issues` events.
- **Diff method**: the existing assertions keep passing; snapshot text equals `render()` of the same `TrackerIssue` (unchanged function).
- **Transformation pipeline coverage**: tracker JSON → `TrackerIssue` (contract tests, now with `author`); `TrackerIssue` → stdin JSON (`judge` stdin test); `TrackerIssue` → `render()` → snapshot (existing tests, unchanged output); snapshot → `heading` → issue record (`heading` unit test, selected-content integration test).

## Design Boundaries

- **Intentional limitations**:
  - A failed judgment is not retried for the same content until the content, the filter setting or the daemon run changes, so an owner's command that fails intermittently lets those issues through until then; failures are visible through events (shinpr/ovenspring#9). Stopping intake on failure is out of scope (PRD).
  - Editing the script a fixed `filter` string names is not a filter change; it takes effect for new or changed content and after a restart.
  - A command changed while the daemon was stopped is treated as a restart, not a transition: snapshot-less candidates wait for their verdict rather than being published as without a filter (Selected Design, Start and restart).
  - While the lead proposes, new admissions and edits to published candidates are published only once the proposal ends, with or without a filter.
  - Snapshots of selected issues no longer refresh their `State` and `Blocked by` lines; the sprint already carries dependencies from the plan (`start_sprint_issues`).
  - The snapshot does not show the issue author; `render()` is unchanged.
  - Commands run one at a time; a very large number of open issues is out of scope (PRD).
  - Credentials are masked in the reason as the daemon held them from the command's start until its result was taken. A value written to the credentials file and replaced again before the daemon's reload loop reads the file is never held by the daemon, so it is not masked.
  - The drop reason is recorded as the owner's command wrote it apart from masked credentials, and agents collecting a sprint's facts can read it (Security Considerations). A word that only looks like a token (for example one starting `sk-`) is masked too, as in downloads.
- **Existing extension points used**: `Tracker::open_issues` (consumers: `poll_tracker`, `setup_checks`); `records::event` (consumers: downloads, the facts step, shinpr/ovenspring#9); `Timing` (consumers: daemon, tests).

## Risks and Mitigation

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| The daemon ends without its normal stop (killed) while a filter command runs; the command's group outlives it | Low | Low | The command ends by itself or at its own pace; its result is lost and the next start judges again |
| An owner's script reads only part of stdin and blocks | Low | Medium | The writer thread is separate; the 60 s limit stops it as a failure |

## References

- `docs/prd/oven-spring-prd.md` (AC-015, AC-059, AC-072 to AC-075)
- `docs/design/factory-core-design.md` (Sprint; Data Flow; Tracker; Logging and Monitoring)
- `docs/design/distribution-design.md` (Configuration)
- NOR-32 and the owner's answers `NOR-32-spec-2.md`, `NOR-32-prd-3.md`
- Linear GraphQL schema, `Issue.creator`: https://github.com/linear/linear/blob/master/packages/sdk/src/schema.graphql
- GitHub REST, List repository issues: https://docs.github.com/rest/issues/issues#list-repository-issues
