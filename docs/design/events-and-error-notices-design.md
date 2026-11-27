# Events Page and Error Notices Design Document

## Overview

The dashboard gets an Events page (S-08) that reads the factory's `events.jsonl` through a new `GET /api/v1/events`. The daemon gets one owner for "an error" (`errors.rs`). Each error's first failure is a start line in `events.jsonl`, its later failures are repeat lines, and its next success (or the owner removing the filter command) is a cleared line. The state of the errors going on, and of the notices still to post, is derived from those lines alone, so one append is the one commit of each transition. Each start and clear is told by one Slack message (M-13, M-14). The messages go out while the `[notices] errors` setting is on, in order, until Slack accepts each. A message carries a marker, so a retry finds a message Slack already accepted instead of posting it again. The same lines replace `services-unanswered.json`, so the Now page's "Can't reach" status and the Events page's Errors now read one source.

### Referenced UI Spec
- UI Spec path: `docs/ui-spec/oven-spring-ui-spec.md` (version 1.7): S-08 Events, EventsPage, ErrorsNow, EventFilters, EventLine, ErrorNoticesRow, Contact, Vocabulary "Events (read layer)" and "Errors", Slack Messages M-08, M-13, M-14 and Delivery, Visual Acceptance (golden states 9–15)
- Binding prototype: `docs/ui-spec/assets/events/prototype.html` (the owner chose binding: `sources/answers/GH-9-spec-13.md`; approved: `sources/answers/GH-9-prototype-15.md`)
- Component structure, states, wording and layout are inherited from the UI Spec. This document owns the server contract, the daemon's error rules, and which failures are errors.

## Background and Context

### Prerequisite ADRs

- `docs/adr/ADR-0005-file-board-and-state-outside-repository.md`: errors are recorded in `events.jsonl` (JSON Lines under the state directory). Restart rebuilds the errors going on and the waiting notices from it. No new state file is added.
- `docs/adr/ADR-0007-daemon-owns-external-writes.md`: only the daemon posts to Slack and writes events. The server only reads state, and it saves `config.toml` as every Settings section does.
- `docs/adr/ADR-0004-slack-socket-mode-channel.md`: error notices are `Channel::post_notice` messages and need no new channel.
- `docs/adr/ADR-0002-embedded-web-dashboard.md`: the page is part of the embedded React dashboard and reads `/api/v1`.

### External Resources Used

| Resource (project-tier label) | Feature-specific identifier | Notes |
|-------------------------------|-----------------------------|-------|
| UI prototype | `docs/ui-spec/assets/events/prototype.html` (`#/events`, `#/settings/connections` row `row-notices`, `#/slack`; its situation select: `open`, `clear`, `off`, `new`, `unreadable`, `lost`) | Binding for S-08, the Error notices row and the wording of M-13/M-14 (UI Spec, Prototype Management). No `docs/project-context/external-resources.md` exists, so the path is given here directly. |
| Slack Web API | `chat.postMessage` (`metadata`), `conversations.history` (`include_all_metadata`) | Already used by `crates/integrations/src/channel/slack.rs` (decisions carry metadata; `mark_resolved` and `catch_up` read history). The bot scopes `channels:history` and `groups:history` are in the app manifest (`slack.rs:353-354`). |

### Requirement Convergence

- **Outcome**: N/A — covered by PRD `docs/prd/oven-spring-prd.md` (AC-072, AC-076 to AC-078; Success Criteria 4)
- **Non-Goals**: N/A — covered by PRD `docs/prd/oven-spring-prd.md` (Future / Out of Scope)
- **Left open by the owner**: None. The issue (GH-9, "Left to the design") leaves two points to this document: which events are errors, and where the notice goes. The UI Spec already settles narrowing and the destination (Slack only, AC-077 row). The owner added the switch (`sources/answers/GH-9-prd-10.md`: "on by default", "Turning it off stops only the notices. The events page (AC-076) still shows everything.") and worded its two options (`sources/answers/GH-9-prototype-14.md`).

### Standards and Assumptions

#### Applicable Standards
- [x] Rust 2024, `cargo fmt`, `cargo clippy --all-targets -D warnings` `[explicit]` - Source: `rustfmt.toml`, `Cargo.toml` `[lints]`, `.github/workflows/ci.yml`
- [x] Dashboard `npm run check:all` (biome, knip, tokens, tsc, vite build, vitest, Playwright fixture e2e) `[explicit]` - Source: `dashboard/package.json`, `.github/workflows/ci.yml`
- [x] Every config struct is `deny_unknown_fields`; an optional section has `#[serde(default)]` `[explicit]` - Source: `crates/core/src/config.rs:17-46`
- [x] The factory's own Slack messages are fixed English templates built in the daemon; the dashboard words codes in `dashboard/src/lib/vocabulary.ts` `[implicit]` - Evidence: `crates/daemon/src/owner.rs` `announce_pause`; `docs/design/owner-interface-design.md` (Slack message builders); `dashboard/src/lib/vocabulary.ts` - Confirmed: Yes
- [x] A dashboard list page polls with `usePoll` every 5 s, keeps filters in the URL query, and shows `Contact` for freshness `[implicit]` - Evidence: `dashboard/src/app/activity/ActivityPage.tsx`, `dashboard/src/api/usePoll.ts` - Confirmed: Yes
- [x] Server responses are ts-rs types listed in `crates/server/src/types.rs` roots; a stale `dashboard/src/api/types.gen.ts` fails a test `[explicit]` - Source: `crates/server/src/types.rs`
- [x] The header's current page comes only from `Shell.tsx` `area()`, passed to `AppHeader` as `current` (`aria-current="page"`) `[implicit]` - Evidence: `dashboard/src/app/Shell.tsx:21-27,63-67`; `dashboard/src/components/AppHeader/AppHeader.tsx:72-73,112` - Confirmed: Yes

#### Assumed Behaviors
- [x] `records::lines` reads a JSON Lines file whole as UTF-8 (`fs::read_to_string`) and skips a line that does not parse; a file holding a cut multibyte character fails as a whole - Evidence: `crates/daemon/src/records.rs:226-238` - Design effect: `records::lines` reads bytes and decodes each line on its own (Selected Design, Error state); the endpoint numbers parsed lines in file order; a torn last line has no number until it is complete - Confirmed: Yes
- [x] One event is one `write_all` of a JSON line ending in a newline, in append mode. An `Err`, or a crash, can leave part of the line written with no newline, and `append_line` adds nothing before the next line, so the next appended line joins that part and neither parses (`records.rs:227-235`). A temporary-file check reproduced it (finding I002) - Evidence: `crates/store/src/lib.rs:401-412` (`append_line`) - Design effect: `append_line` first ends an unterminated last line (Selected Design, Error state), so a line whose append returned `Ok` is always a parsed line, and the error state is advanced only after that - Confirmed: Yes
- [x] Now already reads all of `events.jsonl` on every 5 s poll (`fact_events` for its Times) - Evidence: `crates/daemon/src/overview.rs` (fact_events via `records::lines`) - Design effect: Now and Events derive the errors from that same read, with no new class of cost - Confirmed: Yes
- [x] Each launch's first line in `launches.jsonl` records its `item`, `issue`, `sprint` and `cli` - Evidence: `crates/daemon/src/records.rs` `LaunchStart` - Design effect: events that name only an `item` are attributed through it, old lines included - Confirmed: Yes
- [x] While the halt file exists, `tick` returns before the tracker and GitHub polls - Evidence: `crates/daemon/src/lib.rs:464-525` - Design effect: an error neither repeats nor clears while paused; notice delivery is placed before the halt check so waiting messages still go out - Confirmed: Yes
- [x] The daemon knows the dashboard's address - Evidence: `crates/daemon/src/lib.rs:131` `Paths.dashboard`; `crates/daemon/src/owner.rs:916` - Design effect: M-13 links to `<dashboard>/events` - Confirmed: Yes
- [x] The filter command's stderr is discarded today - Evidence: `crates/daemon/src/intake.rs` `judge` (`.stderr(Stdio::null())`) - Design effect: the UI Spec's start line "(<its output>)" needs stderr's first line captured (Selected Design, Filter output) - Confirmed: Yes
- [x] Slack keeps a message's `metadata` and returns it from `conversations.history` with `include_all_metadata=true` - Evidence: `slack.rs` `post_decision` (metadata `ovenspring_decision`), `decision_of` and `mark_resolved` (read it back with `include_all_metadata`); `crates/integrations/tests/slack_contract.rs:470-487,693` - Design effect: a notice's marker can be found after a lost answer (Selected Design, Delivery) - Confirmed: Yes
- [x] `post_notice` posts one root message and then replies only for text beyond `MARKDOWN_MAX` (12,000 characters); `chat.getPermalink`'s failure is ignored - Evidence: `slack.rs:28,218-235,605-626,829-834` - Design effect: an M-13/M-14 bounded far below that size is exactly one `chat.postMessage`, so no reply can fail after the root was posted - Confirmed: Yes
- [x] An agent that cannot be spawned fails inside `start_process` and returns `DaemonError::Launch`; no child exits, so `finished` never sees it. `launch_open` passes it to `settle` for an item of a recorded issue and to `settle_sprint` for a sprint item, and lets it escape to `settle_part("launching steps")` only for an item with neither - Evidence: `crates/daemon/src/lib.rs:342-357,529-550`, `stops.rs:44-138` - Design effect: a spawn failure is not an error and keeps the core rule (`docs/design/factory-core-design.md`, Exceptions, recovery and resilience): it is launched again on the next pass; failing on two passes in a row stops its issue or sprint and files `exception`, and pauses the factory with M-08 only when no issue or sprint owns it (Selected Design, Errors; findings I004, Conflict-002) - Confirmed: Yes
- [x] A restart sets `pause_announced` from the halt file's existence - Evidence: `crates/daemon/src/lib.rs:254-255`; `owner.rs:1012-1035` - Design effect: the M-08 of a CLI error is not left to `announce_pause`; it is a waiting notice derived from `events.jsonl` (Selected Design, Delivery) - Confirmed: Yes

#### Quality Assurance Mechanisms
- [x] `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace`, `cargo deny`, `cargo machete` — Enforces: format, lints, tests, dependencies — Config: `.github/workflows/ci.yml:34-40` — Covers: `crates/**` — Status: `adopted`
- [x] Generated types — Enforces: `types.gen.ts` matches the server types — Config: `crates/server/src/types.rs` (rewrite with `OVENSPRING_WRITE_TYPES=1 cargo test -p ovenspring-server`) — Covers: the events response, `NoticesConfig` — Status: `adopted`
- [x] `npm run check:all` — Enforces: biome, knip, tokens, tsc, build, vitest, Playwright fixture e2e — Config: `dashboard/package.json:24`, `dashboard/playwright.config.ts` (1280×800, UTC, `*.fixture.e2e.test.ts`) — Covers: `dashboard/**` — Status: `adopted`
- [x] Stories with axe in both themes — Enforces: WCAG 2.2 AA (AC-048) — Source: `docs/design/owner-interface-design.md` (Stories), `dashboard/.storybook/preview.tsx` — Covers: the new views — Status: `adopted`
- [x] Lint-suppression hook — Enforces: no new suppressions — Config: `.agents/hooks/lint-suppression.sh` — Covers: project-wide — Status: `adopted`

### Problem to Solve

The owner reads `events.jsonl` by hand to learn what the factory did. They hear about a failure only when it pauses the factory (M-08). Otherwise it is one event among many, repeated on every pass (an unreachable tracker is logged every minute).

### Current Challenges

- No API endpoint or page reads `events.jsonl` for the owner. Its ~75 kinds name what they concern inconsistently: `issue`, a `sprint` id, `sprint: true`, a sprint id in `issue`, or only an `item` (focus area `records.rs:event`).
- Start and clear are tracked only for unreachable services (`services-unanswered.json`, no event on change). Tracker refusal (`tracker_error`) and filter verdicts are held in memory only, so a restart forgets them.
- Nothing announces any error except M-08.

### Requirements

#### Functional Requirements
- AC-076: an Events page lists every recorded event newest first, with when, what, and the issue or sprint it concerns; filter drops (key, URL, reason) and failures (key, what failed) appear there.
- AC-077: while notices are on, the owner is told once when an error starts and once when it clears, however often it repeats in between. A filter failure is such an error. This design settles which other failures are errors (Selected Design, Errors) and where the notice goes: Slack only (UI Spec, AC-077 row).
- AC-078: an on/off switch in Settings, on by default. Turning it off stops only the notices.
- AC-072 (owner-facing part): drops are never posted to Slack, and failures reach Slack only as error notices.

#### Non-Functional Requirements
- **Freshness**: the page reflects a new event at its next 5 s poll — Source: selected in Design Convergence (the existing dashboard poll interval; the requirements set none)
- **Read cost**: one whole read of `events.jsonl` and `launches.jsonl` per poll; the response carries only the shown lines — Source: selected in Design Convergence (the lowest level that delivers; Risks)
- **Masking**: no held credential and no value in a common token format is written in a new event field, served by the endpoint, or posted to Slack (AC-047) — Source: PRD Non-Functional Requirements, Security; UI Spec Vocabulary, Events; `docs/design/factory-core-design.md` Security Considerations

## Acceptance Criteria (AC) - EARS Format

### Events page (AC-076, AC-072)

- [ ] **When** the owner opens Events, the system shall list the newest 15 matching lines of `events.jsonl`, newest first. Each line has its time, its kind's sentence and its subject. The subject is the issue key, "Sprint", or blank when the line concerns neither.
- [ ] **When** a line names only an `item`, the system shall attribute it to the issue or sprint that item's launch recorded in `launches.jsonl`. **When** a line's `issue` names a sprint the factory has a record of and no issue it has a record of, the system shall attribute it to that sprint.
- [ ] **When** the owner follows a sprint subject, S-03 shall open scrolled to that sprint's own place once its data has loaded.
- [ ] **When** the issue filter drops an issue, the Events list shall show a drop line with the key, the masked reason and a link named after the recorded URL's tracker. **When** the filter fails, it shall show an Error line with the key, the command, what failed and the first line of its stderr.
- [ ] **When** an error repeats on the same issue (or none) with the same cause, the system shall fold the repeat into the earlier line, raising its count and last time. **When** it repeats on another issue or with another cause, it shall list the repeat as its own Error repeat line.
- [ ] **While** the page is open, **when** a new line is recorded, the system shall add it at the top at the next poll and keep every line already shown.
- [ ] **When** the owner chooses "Show older events", the system shall add the next 50 older matching lines below and keep the lines already shown, new arrivals included, while the older ones load.
- [ ] **If** lines are recorded and none matches the filters, **then** the page shall show "No event matches this filter."; **if** no line is recorded, **then** it shall show "Nothing has been recorded yet."
- [ ] (ubiquitous) No held credential and no value in a common token format shall appear in a new event field, the endpoint's response or a Slack error notice, including when the 300-byte cut of the filter's stderr would fall inside a credential.

- [ ] **When** the factory merges a pull request, the system shall show its number and recorded base. **When** the factory completes a post-implementation review round, the system shall show its number and the count of findings it hands to correction.
- [ ] **When** Slack accepts a sprint proposal through the decision delivery path, the system shall show which sprint it concerned, how many issues it proposed and its purpose. **While** the decision is unsent, the system shall show no successful proposal post for it.

### Error notices (AC-077, AC-078)

- [ ] **While** notices are on, **when** an error starts, the system shall post one M-13. Repeats shall post nothing. **When** the error clears, the system shall post one M-14. A daemon restart while it lasts shall post nothing more.
- [ ] **If** Slack accepts a notice and its answer is lost, or the daemon stops before recording that it was told, **then** the next delivery shall find that message in the channel in force and record it as told without posting it again.
- [ ] **If** the daemon stops or an append fails between a transition and its record, **then** the next pass shall leave each start, clear and delivery recorded exactly once.
- [ ] **When** an error that pauses the factory starts (the agent CLI fails a run), the system shall post one M-08 that names that error's cause. Its start line shall read "You were told in Slack." once that M-08 is accepted. A restart before it is accepted shall still post it. **If** the daemon stops after that start line is recorded and before the factory is paused, **then** the restart shall pause the factory with the recorded reason before it launches anything.
- [ ] **When** the agent CLI cannot be spawned, the system shall launch the step again on the next pass and record no error line for it. **If** it fails on two passes in a row, **then** the system shall stop the issue or sprint that owns the item and file `exception`, or pause the factory with M-08 when no issue or sprint owns it, as today.
- [ ] **When** a GitHub operation keeps failing as unreachable while another way to GitHub, or another GitHub request, answers, the error shall stay open with no clear and no new start. It shall clear once a pass finds every way that failed answering and no GitHub operation failing as unreachable.
- [ ] **If** Slack does not accept a notice, **then** the system shall post it once Slack accepts again, in the order the notices were recorded. A waiting M-13 shall go before its M-14.
- [ ] **While** notices are off, the system shall post no M-13 or M-14 and drop the waiting ones, and the Events page shall still list every line. M-08 is unaffected.
- [ ] **When** the filter judges an issue freshly (admitted or dropped) while the filter's error is open, the system shall clear it once, naming that issue. A verdict reapplied from the cache shall not clear it.
- [ ] **When** the filter command is absent while the filter's error is open, whether the daemon sees it on reload or at start, the system shall clear the error once with "The issue filter was removed."
- [ ] **When** the daemon starts with a `services-unanswered.json` from the previous version, Now shall keep that service's "since" and reason, and the next confirmed answer shall clear it once.
- [ ] **When** the owner saves Error notices in Settings, the system shall store `[notices] errors` without checking any connection. The daemon shall apply it on its next pass and record "Settings changed: Error notices are now off/on."
- [ ] (ubiquitous) A configuration without `[notices]` shall have notices on.

### Look (binding prototype)

- [ ] **When** the Events golden states 9–14 and the open Error notices row (15) are rendered with the prototype's own data, their visible words, from the lost-contact line through the end of the list, and the positions of the compared boxes shall match the prototype's same situation at 1280px, with the UI Spec's stated wording overrides and the Issue options derived from available events. The Events list's words and layout shall also match at 600px, where the subject sits above the sentence (Verification Strategy, Visual comparison).

## Existing Codebase Analysis

### Implementation Path Mapping
| Type | Path | Description |
|------|------|-------------|
| Existing | `crates/daemon/src/records.rs` | `event`, `lines`; `Failing`/`unreachable`/`reached` over `services-unanswered.json` (removed; `Service` and `Failing` stay as types) |
| New | `crates/daemon/src/errors.rs` | Error kinds, `Errors` (state derived from lines), `failed`/`succeeded`/`deliver_error_notices`, import of the old file, M-08/M-13/M-14 text |
| New | `crates/daemon/src/events.rs` | Read side: numbering, attribution, folding, told state, filters, window, masking |
| Existing | `crates/daemon/src/stops.rs`, `pulls.rs`, `intake.rs`, `lib.rs`, `owner.rs`, `issues.rs`, `sprint.rs` | Emitters routed through `errors.rs`; `sprint` id instead of `true`; `delete_branch` names a sprint as `sprint` |
| Existing | `crates/core/src/flow.rs`, `crates/daemon/src/handoff.rs` | Signal a completed review round before filing its next action. Count the findings from the existing correction handoff. |
| Existing | `crates/integrations/src/github.rs` | Carry the pull request base ref from the existing response into the daemon's merge event. |
| Existing | `crates/daemon/src/overview.rs` | `status.unreachable` from `errors::Errors` over the lines it already reads |
| Existing | `crates/integrations/src/channel/mod.rs`, `slack.rs` | `NoticeKind::ErrorStarted`/`ErrorCleared`; `Notice.marker`; Slack finds a marked message before posting it |
| Existing | `crates/core/src/config.rs` | `NoticesConfig { errors: bool }`, default on |
| Existing | `crates/store/src/lib.rs` | `append_line` writes `\n` first when the file's last byte is not one |
| Existing | `crates/server/src/api.rs`, `settings.rs`, `types.rs` | `GET /events`; `notices` section |
| New | `dashboard/src/app/events/` | `EventsPage`, `EventsView`, `ErrorsNow`, `EventFilters`, `EventLine`, CSS, stories |
| Existing | `dashboard/src/app/App.tsx`, `app/Shell.tsx` (`area()`), `components/AppHeader` (`Area`, `LINKS`), `components/Contact`, `app/settings/sections.tsx`, `app/sprints/SprintsView.tsx` and `SprintsPage.tsx`, `lib/vocabulary.ts`, `lib/time.ts` | Route, current page, nav link, two-sentence lost-contact line, ErrorNoticesRow, sprint anchors and scroll, event wording, day headings |
| Existing | `crates/cli/tests/support/slack_stub.rs` | History that returns posted messages with their metadata, honoring `oldest` and paging by `cursor`; a mode that accepts a post and loses its answer |
| Existing | `dashboard/e2e/events.fixture.e2e.test.ts`, `dashboard/src/fixtures/events.ts` | Complete visual comparison in block 7 using the existing fixture transcription. |

### Integration Points
- **Integration Target**: the daemon's failure paths (`settle_for`, `poll_pull_requests`, `poll_tracker`, `finished`, `take_filter_run`, `log_verdict`) → `errors.rs`
- **Invocation Method**: direct calls on `Daemon` (`self.failed(...)`, `self.succeeded(...)`), in place of `records::reached` and the bare `self.event` for those kinds
- **Integration Target**: `tick` → `clear_removed_filter` and `deliver_error_notices` before the halt check; `recover` → `errors::Errors` built from `events.jsonl`, then `import_unanswered`; each pass, after taking in finished launches, the pause of each CLI start left without its `paused` line (`Errors.unpaused`)
- **Integration Target**: server router → `events::page(store, query, secrets)`; Settings save → `SettingsSection::Notices`

### Code Inspection Evidence

| File/Function | Relevance |
|---------------|-----------|
| `crates/daemon/src/records.rs:73 reached` | Existing start/clear for unreachable services; replaced by `errors.rs` |
| `crates/daemon/src/stops.rs:73 settle_for`, `:144 unreachable` | Emits `service-unreachable` per failing operation; tells API failures (`HttpError::Transient`) from `origin` failures (`git ls-remote`); `sprint: true` |
| `crates/daemon/src/pulls.rs:326 poll_pull_requests` | The only GitHub clear (`viewer` answers) |
| `crates/daemon/src/intake.rs:201 poll_tracker` | Tracker unreachable / refused / success |
| `crates/daemon/src/intake.rs:243 intake_pass` | Reapplies cached verdicts each pass; `configured == false` treats every issue as Admitted |
| `crates/daemon/src/intake.rs:401 take_filter_run` | Where a fresh judgment's verdict is inserted |
| `crates/daemon/src/lib.rs:342 start_process`, `:772 finished` | Spawn failure (a pass failure, unchanged); `launch-error` then `pause` |
| `crates/daemon/src/owner.rs:1012 announce_pause`; `lib.rs:254` | M-08 once per pause; `pause_announced` from the halt file at start |
| `crates/daemon/src/lib.rs:310 reload_config` | `config-reloaded`, `filter_changed`; runs only when the file's text changed |
| `crates/daemon/src/issues.rs:477 delete_branch` | `github-error` puts a sprint id in `issue` (callers `sprint.rs:637,676`) |
| `crates/daemon/src/download.rs:102 mask`; `api.rs:266 get_export` | Masking with held credentials |
| `crates/daemon/src/history.rs` `launch_rows`, `ended_sprints` | Activity list pattern; S-03's ended sprints |
| `crates/server/src/settings.rs:262 save` | Section save; `check_connection` only for general/tracker/slack |
| `crates/integrations/src/channel/slack.rs:605-652,816-834` | `post`, `reply_rest`, `post_decision` metadata, `post_notice` |
| `crates/daemon/src/pulls.rs` `merge`, `crates/integrations/src/github.rs` `RawPull` | The successful merge writer already has the PR number and reads the latest pull before merging. The adapter currently keeps the head SHA and discards the base ref. |
| `crates/core/src/flow.rs` `round_progress`, `crates/daemon/src/handoff.rs` `handed` | The flow owns the completed round boundary. The handoff owns which new and maintained findings go to correction. |
| `crates/daemon/src/sprint.rs` `post_sprint_decision`, `crates/daemon/src/owner.rs` `decide`, `send_decision`, `deliver_decisions` | The plan exists when the decision is made. Both initial and retried posts use the decision saved as Unsent. |
| `dashboard/src/app/activity/ActivityPage.tsx` | Filters in the URL query, `usePoll`, `Contact` |
| `dashboard/src/api/usePoll.ts` | A changed `key` drops the previous answer; `refresh()` asks again under the same key |
| `dashboard/src/app/sprints/SprintsPage.tsx`, `SprintsView.tsx:131-146` | S-03 renders after its poll answers; headings `this-sprint`, `ended-sprints` |

### Fact Disposition Table

Code analysis focus areas:

| Fact ID | Focus Area | Disposition | Rationale | Evidence |
|---------|------------|-------------|-----------|----------|
| crates/daemon/src/records.rs:event | The event log's shape: free-form JSON lines with no schema, mixed attribution fields, and lines from other writers | preserve | The log stays free-form and every existing field stays. The read side serves each line's kind and fields, so unknown kinds and other writers' lines (`replay-reset`) are listed with the fallback sentence. Attribution order: an `issue` string that names a known issue, or names no known sprint; else an `issue` string naming a known sprint (old `github-error` lines from `delete_branch`); else a `sprint` string; else `item` through `launches.jsonl`. The `sprint: true` emitter (`stops.rs`) and the `delete_branch` emitter for sprints are corrected to write the sprint id in `sprint`. Old `sprint: true` lines show a blank subject. New fields are additive. | crates/daemon/src/records.rs:216-223; event kinds emitted at the call sites listed in currentPath; live <state>/events.jsonl contains `replay-reset` (no emitter in crates/) written with different JSON spacing |
| crates/daemon/src/intake.rs:log_verdict | Filter drop and failure events, and what 'clears' means for the filter | transform | One command-level error `filter`. Its first `filter-failed` starts it, and each later failure on any issue is a repeat. It clears when `take_filter_run` inserts a fresh `Admitted` or `Dropped` verdict (the cleared line names that issue), and when a pass finds no filter configured while the error is open ("was removed"), at reload or at start. Verdicts reapplied in `intake_pass` never clear. Restarts re-judge and re-log failures as repeats, because the open error is derived from `events.jsonl`. `filter-failed` gains `command` and `output` (first stderr line, masked). Drops stay events only. | crates/daemon/src/intake.rs:262-285 (verdict handling), 323-345 (log once per Judged), 347-396 (one run at a time, skipped while the cached verdict's hash matches), 398-440 (take_filter_run), 179 filter_changed |
| crates/daemon/src/stops.rs:settle_for | Existing repeated-failure handling and the one existing announce-once mechanism | transform | `service-unreachable` goes through `errors.rs` (`github-unreachable`, with `via: api` or `via: git` from `unreachable()`), and the `failures` counter and stop/pause rules stay as they are. Operation failures that stop an issue or a sprint, and pass failures that pause, are not errors here (Selected Design, Errors). M-08 stays the one message for a pause. For a `cli-*` error, its M-08 is that error's start notice. | crates/daemon/src/stops.rs:73-145; crates/daemon/src/records.rs:73-102 (reached); crates/daemon/src/prerequisites.rs:100-106; crates/daemon/src/owner.rs:1012-1036 (announce_pause); crates/daemon/src/lib.rs:464-525 (tick) |
| crates/integrations/src/channel/mod.rs:Channel::post_notice | Slack as a notice destination, including the case where Slack itself is the error | transform | New `NoticeKind::ErrorStarted` and `ErrorCleared` with fixed templates, and `Notice.marker`. A marked notice is one message carrying the marker as metadata. Before posting it, Slack looks for it among the channel's latest messages. Slack's own failure is not an error (it could not carry its notice), and it stays a prerequisite on Now. A refused post keeps the notice waiting, and `channel-error` is logged once per waiting notice, not per attempt. | crates/integrations/src/channel/mod.rs:46-64 (NoticeKind, Notice), 141-163 (trait: post_notice, connected, trouble); crates/daemon/src/owner.rs:434-447 (tell_owner), 1012-1036; owner.rs and conversation.rs log channel-error on each failed post |
| crates/server/src/settings.rs:save | Where the error-notices switch can live in config and Settings | transform | New table `[notices]` with `errors = true` by default, saved by a new `SettingsSection::Notices` that runs no connection check. The dashboard shows it in Connections after Slack. The daemon applies it through `reload_config`. | crates/core/src/config.rs:17-46 (Config with deny_unknown_fields on every section), 274-279 (SlackConfig {channel, owners}); crates/server/src/settings.rs:143-153 (SettingsSection), 262-341 (save), 346-410 (check_connection runs for general, tracker, slack); crates/daemon/src/lib.rs:310-330 (reload_config); dashboard/src/app/settings/SettingsView.tsx:17-23 (pages) |
| crates/daemon/src/download.rs:download | Credentials in event text shown to the owner | preserve | The download is unchanged. `download::mask` with the held credentials is also applied at three new boundaries: every string of an error-family line before it is appended, every string the endpoint serves, and every notice text before posting (Security Considerations). | crates/daemon/src/download.rs:134-190 (mask applied to the whole download), crates/server/src/api.rs:270-300 (download reads held credentials); crates/daemon/src/intake.rs:418-421 (filter reason masked before logging); docs/design/factory-core-design.md:667 ('credentials never enter logs; every record is masked in the download') |

UI analysis focus areas:

| Fact ID | Focus Area | Disposition | Rationale | Evidence |
|---------|------------|-------------|-----------|----------|
| docs/ui-spec/oven-spring-ui-spec.md:Prototype Management | Which prototype governs, and how strongly | preserve | The events prototype is binding for S-08, the Error notices row and M-13/M-14 wording; the UI Spec decides behavior. Its control bar, notes and `#/slack` chrome are not built. A reproducible comparison proves the match (Verification Strategy, Visual comparison). | Observed: the UI Spec names one prototype, docs/ui-spec/assets/dashboard/index.html, binding for structure, layout, spacing, vocabulary and copy tone; the spec decides behavior, states, triggers and data (lines 20-35, 41). The owner chose binding for the new screens (sources/answers/GH-9-spec-13.md) and approved docs/ui-spec/assets/events/prototype.html (GH-9-prototype-15). |
| dashboard/src/components/AppHeader/AppHeader.tsx:LINKS | Navigation entry and route | transform | `Area` gains `'events'`; `LINKS` gains Events between Activity and Sprints; `App.tsx` gains route `events`; `Shell.tsx` `area()` maps `/events` to `'events'`, which is what makes the Events link current (`aria-current="page"`). The header fixture e2e checks the current link on `/events`. | Observed: AppHeader.tsx:11 `type Area = 'now' \| 'activity' \| 'sprints' \| 'settings'`, and LINKS at lines 13-17 are Now, Activity, Sprints, Settings. Shell.tsx area() maps /activity, /sprints and /settings; App.tsx has no events route. The prototype nav is Now · Activity · Events · Sprints · Settings, with Events marked aria-current on its page. The UI Spec component tree (line 140) and reuse map list four links. |
| crates/server/src/api.rs:routes | Data the Events page reads | transform | New `GET /api/v1/events` returning each line's kind and masked fields plus derived subject, error role and told state (Data Contracts). Wording stays in `vocabulary.ts`. | Observed: api.rs:21-39 has no events route. events.jsonl is read only by records::lines (records.rs:226, whole file, malformed lines skipped) for fact_events, last_activity and the export (download.rs:152). records::event (records.rs:217) adds `at` (RFC 3339) to free-form JSON. Kinds are a string field `event`. |
| crates/daemon (event kinds) | A sentence for each recorded kind | transform | `vocabulary.ts` gets a sentence per kind the UI Spec words and the fallback for others. Production event writers supply the merge, review round and posted proposal details (Selected Design, Event details). `config-reloaded` gains `changes` so "Settings changed" can be worded. | Observed: about 70 distinct `event` kinds are written in crates/daemon and crates/integrations (agreements-error … unreadable-output, including channel-error, github-error, tracker-error, service-unreachable, launch-error, push-failed, merge-refused, config-refused, paused, resumed, merged, pushed, sprint-begun, decision*, ignored-*). There are also `fact` lines (records.rs:165, FactEvent issue/fact/from/to) and the dynamic intake kinds filter-dropped {issue,url,reason} and filter-failed {issue,error} (intake.rs:270-283, via log_verdict). Dashboard convention: the server returns codes and dashboard/src/lib/vocabulary.ts maps each code to words ('the read layer uses sentences that name who acts'). |
| docs/ui-spec/assets/events/prototype.html:viewEvents | Events page structure and states | preserve | All six situations are built: Listed/All shown, Filtered to nothing (`recorded` tells it from Nothing recorded), Nothing recorded, Unreadable, Lost contact, Not read yet. Each is a story and is compared with the prototype. | Observed in the prototype (viewEvents, lines 538-568): |
| docs/ui-spec/assets/events/prototype.html:line | One event line | preserve | Grid 52px · 84px · sentence; below 620px clock · one column, subject above sentence. Error/Cleared words and told lines from the response's `error`. The drop link is named from the recorded URL. Compared at 1280px and 600px. | Observed: line() at prototype.html:525-537, CSS .ev2 at line 421. |
| crates/daemon/src/intake.rs:233 | Error episodes: start, repeats, clear, Errors now | transform | Occurrences are recorded as start/repeat/cleared lines with a `failure` annotation. Errors now, counts, last times and told states are derived from them (Selected Design, Error state). | Observed: a failure is written on every attempt (service-unreachable on each tracker poll, intake.rs:233; stops.rs:93-103 writes service-unreachable or operation-failed per failure). records::reached tracks only per-service reachability, for Now's Unreachable status (overview.rs:347-360). filter-failed is logged once per judged issue (log_verdict skips when already logged, intake.rs:330). |
| crates/daemon/src/overview.rs:Status | Overlap with Now's status | transform | `status.unreachable` is derived from the same open occurrences as Errors now, so both show one `since`. | Observed: Now's FactoryStatus already shows the states Prerequisites, Unreachable (service, since) and SettingsNotApplied (config_error) (overview.rs:49-93, 347-376). The UI Spec's guide list says 'A service that stops answering after it was connected shows on Now.' |
| dashboard/src/app/activity/ActivityPage.tsx:filters | Narrowing the list (Show, Issue) | transform | Filters stay in the URL query as Activity does; the layout is the prototype's `.filters` flex-wrap (UI Spec EventFilters), not Activity's grid. Issue titles come from the response's `issues`. | Observed in the prototype (lines 563-564): 'Show' select with 'Everything' / 'Errors and when they cleared' / 'The issue filter's drops and failures', and 'Issue' select with 'All issues', then each key present in the events plus its title, sorted. In the code, Activity keeps its filters in URL search params (ActivityPage.tsx: useSearchParams) and draws them with the Activity.module.css .filters grid (label display:contents, select column minmax(0,48ch)). The prototype's .filters CSS is flex-wrap. |
| dashboard/src/lib/time.ts | Times and day headings | transform | `time.ts` gains the day heading ("Today, Nov 27", "Yesterday, Nov 26", else the date) and the cross-day "last at" rule the UI Spec Time section gives, with `time.test.ts` cases. | Observed: time.ts has clock() ('14:10', en-GB), dayAndClock() and when(). No helper produces 'Today, Nov 27' / 'Yesterday, Nov 26', and the src tree has no 'Today'/'Yesterday' string. Activity groups by hour ('14:00'), not by day. The UI Spec Time section defines clock and date formats only. |
| dashboard/src/components/Contact/Contact.tsx | Lost contact and unreadable states | transform | `Contact`'s lost-contact line becomes the two sentences the UI Spec (Component: Contact) and the prototype give, "The dashboard lost contact with the factory. This page is as it was at <clock>.", on every page that uses it; its not-yet-read line is unchanged. EventsPage reuses `Contact` and `usePoll` as Activity does. A server that answers but cannot read `events.jsonl` is not Contact's case: the response's `unreadable` gives the page's own "The events could not be read…" state (Data Contracts; UI Error State Design). | Observed: Contact renders 'The dashboard lost contact with the factory; this page is as it was at HH:MM.' (role=status), or '<What> could not be read: the dashboard cannot reach the factory…' (role=alert). usePoll polls every 5 s and keeps the last answer. The prototype's lost note reads 'The dashboard lost contact with the factory. This page is as it was at 14:20.' (two sentences). Its 'could not be read' state is a server-side read failure with its own text. |
| dashboard/src/app/settings/sections.tsx:Connections | Error notices row in Settings | transform | `Connections()` renders a new `ErrorNoticesRow` after `SlackRow` and before `OwnersRow`, built from `SettingRow` and `Choices` (RadioGroup, visible legend) as Who merges is. Label, what-line, summary with the Events link, the two option texts in the owner's words (`sources/answers/GH-9-prototype-14.md`), Save and Cancel, and "Applies at once." are the prototype's. Save calls `PUT /settings/notices`; a refusal shows its reason under the radios. `lib/settings.ts` `PAGE_OF` maps the `notices` section to Connections. Compared with the prototype as golden state 15. | Observed in the prototype (viewSettings, lines 570-590): the row sits in Connections after Slack and before 'Who the factory answers'. Label 'Error notices' with what-line 'A message in Slack when an error starts and when it clears.' Summary '<b>On</b>' or '<b>Off</b>. Errors still show on Events' (link). Change opens a fieldset with a visible legend 'Error notices' and radios. Under 'On (default)': 'One message when an error starts and one when it clears, however often it repeats in between.' Under 'Off': 'No messages about errors. The Events page still shows every error.' (the owner's words, GH-9-prototype-14). Buttons are Save (not 'Check and save') and Cancel, with 'Applies at once.' In code, Connections() renders GitHubRow, TrackerRow, SlackRow, OwnersRow (sections.tsx:350-358). SettingRow's props cover name/what/summary/applies/checks/onSave. RadioGroup takes legend, legendHidden and per-option hint (Choices.tsx). The 'Who merges' row is the representative radio row. |
| crates/server/src/settings.rs:SlackSection | Where the notices setting is saved | preserve | `SlackSection` and its save stay as they are, connection check included. The switch is not a `[slack]` key: it is `[notices] errors`, saved by its own `SettingsSection::Notices`, which runs no connection check, so the owner can turn notices off while Slack fails (Selected Design, Added design surface; Notices section contract). | Observed: config has no notices key (SlackConfig is channel and owners only, config.rs:276-280). SlackSection has deny_unknown_fields (settings.rs:198-207). save() for the Slack section always runs check_connection against Slack (settings.rs:385-398), even for OwnersRow, which says 'Applies at once.' lib/settings.ts PAGE_OF maps a server section to its Settings page. The knowledge file config-key-in-issue-is-storage.md and AC-078 say it is set in the dashboard like other settings. |
| docs/ui-spec/assets/events/prototype.html:events notices-off | Notices off, as shown on Events | transform | The response's `notices` drives the Errors now line "Error notices are off, so Slack tells you nothing about these. Turn them on in Settings". Each start and clear line records its notice state in `failure.notice` (`waiting`, `pause`, `off`), and `error-notice` lines record told or dropped, so a line reads "Not told: error notices are off." only when no message was sent (Events read, told state). The switch is recorded by `config-reloaded.changes` and worded "Settings changed: Error notices are now off/on." (Selected Design, Settings changed). | Observed: when NOTICES is false, Errors now shows a small line, 'Error notices are off, so Slack tells you nothing about these. Turn them on in Settings' (link to /settings/connections). Start and clear lines after the switch read 'Not told: error notices are off.' The switch itself appears as an event, 'Settings changed: Error notices are now off.' |
| docs/ui-spec/assets/events/prototype.html:viewSlack | Slack error notices | transform | M-13 and M-14 are new `NoticeKind::ErrorStarted` and `ErrorCleared` notices with the prototype's `#/slack` wording, posted once per transition by delivery with a marker (Selected Design, Delivery and Notice text). M-14 drops "Issues it could not judge meanwhile are judged now." as the UI Spec says. "See it on Events" links to `<Paths.dashboard>/events` and is left out when the dashboard address is not known. A `cli-*` error is told by M-08 alone. The prototype page's chrome is not built. | Observed (lines 592-601): one message per notice, with no button, sent only while notices are on. Start: '**Error: <name>.** <what failed, with issue key and cause>' / '<what the factory does meanwhile>. You will hear once more when it clears.' / 'See it on Events' (link). Clear: '**Cleared: <name> <again-phrase>.** It failed N times from HH:MM.', plus an effect sentence when one applies ('Issues it could not judge meanwhile are judged now.'). The UI Spec's Slack Messages table has M-01..M-12 and no error notice. M-08 Paused already announces a self-pause. |
| dashboard/src/app/activity/ActivityView.tsx:intro | Boundary with Activity | preserve | Activity (S-04) is unchanged: it stays the page for agent runs. Events lists `events.jsonl` only and never `launches.jsonl` lines; `launches.jsonl` is read only to attribute an `item` to its issue or sprint (Events read, step 2). | Observed: Activity's intro is 'Every agent run in this sprint, newest first.' The UI Spec S-04 says 'Only agent runs are listed.' The prototype's why-note says 'Activity stays the page for agent runs; this page holds everything else the factory records.' |
| docs/ui-spec/oven-spring-ui-spec.md:Accessibility Requirements | Accessibility | preserve | The new views follow the UI Spec's keyboard, screen-reader and contrast rows as written there (the Events filters, "Show older events" and its focus rule, the Error notices radios in a fieldset with a visible legend), with the prototype's sections labelled by their headings, plain lists, Error/Cleared words that do not rely on colour, and "(opens in a new tab)" on external links. Stories run axe in both themes (Quality Assurance Mechanisms; AC-048). | Observed in the prototype: the groups are sections with aria-labelledby; day groups are sections labelled by their h3; lists are ul.plainlist; the Error and Cleared words carry the meaning, with colour added; external links carry '(opens in a new tab)'; focus-visible outline uses the accent; no live region on the list. The UI Spec's keyboard and screen-reader tables have rows per component, and the contrast table already covers color-ok, color-fail and muted text. |
| dashboard/e2e + stories | Verification surface | transform | Stories for golden states 9–15 (two errors going on, nothing wrong now, notices off, nothing recorded, unreadable, lost contact, the Error notices row open) beside `ActivityView.stories.tsx`, with fixtures in `dashboard/src/fixtures/`. Fixture e2e: `events.fixture.e2e.test.ts` (drops and failures listed, filters, polls, older lines, sprint links, and the block 7 prototype comparison), the header and settings fixture e2e (Events current; the switch saves and shows Off). Server contract in `crates/cli/tests/dashboard_int.rs` (Test Boundaries, Integration Verification Points). | Observed: the visual verification environment is Storybook stories per golden state (UI Spec External Resources Used). Peers: app/activity/ActivityView.stories.tsx, app/settings/SettingsView.stories.tsx, fixtures in dashboard/src/fixtures/*.ts. E2E fixture tests: dashboard/e2e/activity.fixture.e2e.test.ts and settings.fixture.e2e.test.ts. Server contract tests: crates/cli/tests/dashboard_int.rs. |

## Design

### Change Impact Map

```yaml
Change Target: error tracking and notices (daemon), events read API (server), Events page and Error notices row (dashboard)
Direct Impact:
  - crates/daemon/src/errors.rs (new), events.rs (new)
  - records.rs (reached/unreachable over services-unanswered.json removed), overview.rs (status.unreachable from errors::Errors)
  - stops.rs, pulls.rs, intake.rs (poll_tracker, take_filter_run, log_verdict, judge), lib.rs (recover, finished, tick: the pause of each CLI start in `unpaused`; applying a CLI start and that pause set `pause_announced`), owner.rs (announce_pause unchanged), issues.rs (delete_branch), sprint.rs (delete_branch callers)
  - crates/integrations/src/channel/mod.rs (NoticeKind, Notice.marker), slack.rs (marked post_notice)
  - crates/core/src/config.rs (NoticesConfig)
  - crates/core/src/flow.rs and crates/daemon/src/handoff.rs / issues.rs (review round completion and findings count)
  - crates/integrations/src/github.rs and crates/daemon/src/pulls.rs (merge number and base)
  - crates/daemon/src/sprint.rs and owner.rs (proposal snapshot in DecisionRecord and decision event at delivery)
  - crates/store/src/lib.rs (append_line ends an unterminated last line first); crates/daemon/src/records.rs (lines decodes each line on its own)
  - crates/integrations/src/http.rs (HttpError::Transient carries the answer's status)
  - crates/server/src/api.rs (GET /events), settings.rs (Notices section), types.rs (roots)
  - dashboard: App.tsx, Shell.tsx area(), AppHeader, Contact, events/*, settings sections and lib/settings.ts PAGE_OF, SprintsView anchors, SprintsPage scroll, vocabulary.ts, time.ts, types.gen.ts
Indirect Impact:
  - events.jsonl: added fields (failure, cli, tracker, why, command, output, changes) and kinds (error-cleared, error-notice, review-round-ended); merged and decision details extend existing fields
  - services-unanswered.json imported once at daemon start, then deleted
  - Design documents updated with this design so they agree: docs/design/distribution-design.md (Configuration: optional [notices] errors); docs/design/issue-filter-design.md (stderr's first line, masking of a failure at the take, Slack only through these notices); docs/design/factory-core-design.md (Store and Exceptions, recovery and resilience: reachability from events.jsonl, the GitHub clear rule here, Slack as a prerequisite); docs/design/owner-interface-design.md (GET /events, the /events route, the notices settings section, status.unreachable's source)
  - Every Notice constructor sets marker: None
No Ripple Effect:
  - download/export selection, time order, JSON Lines format and masking (a newly recorded scoped line, such as `filter-failed` with `command`, `output` and `failure`, is exported whole with its added fields, as the existing contract exports every whole record; lines recorded before the change export unchanged), launches.jsonl, the board, decision buttons and answer processing, M-01..M-12 other than M-08 for a CLI error, agent prompts and factory/ files
```

### Interface Change Matrix

| Existing | New | Conversion Required | Compatibility Method |
|----------|-----|--------------------|--------------------|
| `records::reached(store, Service, Result)` | `Daemon::failed(ErrorKey, Failure)` / `Daemon::succeeded(ErrorKey, Clear)` | Yes | All callers moved in this change |
| `records::unreachable(store) -> BTreeMap<Service, Failing>` | `Errors::unreachable(&self)` with the same map type | Yes | `overview` builds `Errors` from the lines it reads; `pulls` uses the daemon's `Errors` |
| `services-unanswered.json` | start lines in `events.jsonl` | Yes | Imported at daemon start with the same `since` and `why` (Migration Strategy) |
| `Notice {kind, issue, text, preview}` | + `marker: Option<String>` | Yes | Existing constructors pass `None`; behavior unchanged for them |
| `{"event":"config-reloaded"}` | `{"event":"config-reloaded","changes":[…]}` | No | Additive field |
| `stops.rs` `sprint: true` | `sprint: "<id>"` | No | Readers treat a non-string `sprint` as absent |
| `delete_branch` `github-error {issue: <sprint id>}` for sprints | `github-error {sprint: <id>}` | No | Old lines attributed by the sprint-record rule |
| `merged {sha, merge_commit}` with subject | Adds `pr` and `base` | No | Old lines use the existing merge sentence |
| `fact` with issue and stages | Adds a separate `review-round-ended` event | No | Keep the original facts for Times and export |
| `decision {id, kind, issue}` at request | Same kind on successful delivery with `posted: true`, optional `sprint` and proposal summary | No | Legacy lines describe preparation. Saved old decisions can still be delivered. |
| — | `GET /api/v1/events` | — | New |
| `SettingsSection` (9) | + `Notices` | — | New section |

### Architecture Overview

The daemon owns writes. Each failure of an error kind goes through `errors.rs`, which appends the failure's event with a `failure` annotation. Clears append `error-cleared`, and delivered or dropped notices append `error-notice`. `errors::Errors` is the state of the errors going on and of the waiting notices. It is one fold over those lines (`Errors::apply(&line)`). The daemon builds it from `events.jsonl` at start and applies each line after the line is appended. The server and Now build it from the lines they already read. The server reads `events.jsonl`, `launches.jsonl`, the issue and sprint records and the config. It derives the page's lines in `events.rs`, and writes nothing.

### Data Flow

```
failure site ──Daemon::failed(key, Failure)──► mask ► append events.jsonl  <kind fields> + failure{key,since,cause,via?,start?,notice?,reason?}
                                                 └─ appended ► Errors.apply (start | repeat)
success site ──Daemon::succeeded(key, Clear)──► mask ► append error-cleared{failure{key,since,notice},count,issue?,removed?,tracker?,cli?}
                                                 └─ appended ► Errors.apply (removes open, adds waiting clear)
tick ─► clear_removed_filter ─► deliver_error_notices (before take_channel_events and the halt check):
          notices off → append error-notice dropped for each waiting M-13/M-14
          each waiting notice in order → Slack: find marker in latest messages | else post with marker
              accepted/found → append error-notice told{at} ► Errors.apply ; refused → post nothing more this pass (drops still run)
tick end ─► github_clear still pending (probes answered, no GitHub unreachable failure since) → succeeded(github-unreachable)

GET /events?show&issue&from ─► events::page: lines(events.jsonl) numbered → Errors fold → attribute (issue|sprint|item→launch)
   → fold repeats per occurrence(key,since) × (issue, cause) → told state
   → drop error-notice lines → filter show/issue → window by from → mask → EventsPage
GET /overview ─► fact_events lines → Errors fold → status.unreachable
```

### Integration Points List

| Integration Point | Location | Old Implementation | New Implementation | Switching Method | Verification Method |
|-------------------|----------|-------------------|-------------------|------------------|-------------------|
| GitHub unreachable | `stops.rs settle_for`, `pulls.rs poll_pull_requests` | `records::reached` + `service-unreachable` event; `viewer` clears | `failed(GithubUnreachable)` with `via`; `poll_pull_requests` probes each `via` of the occurrence (`viewer` for `api`, `stops::origin_answers` through the [authenticated Git origin capability](github-git-identity-design.md#integration-and-state-transitions) for `git`); all answer → clear pending; the end of the pass → `succeeded` unless a GitHub unreachable failure came after the probes | Direct call | resilience_int: repeated failures → one M-13; the authenticated Git endpoint still unreachable while `viewer` answers → no clear; a pull read keeps answering 502 while `viewer` answers → no clear and no second start; all answer → one M-14 |
| Tracker unreachable / refused | `intake.rs poll_tracker` | `reached`; `tracker_error`; `service-unreachable`/`tracker-error` | `failed(TrackerUnreachable\|TrackerRefused)`; Ok → `succeeded` both | Direct call | resilience_int with tracker stub |
| CLI fails a run | `lib.rs finished` | `launch-error` + `pause` | `failed(Cli(cli))` with the pause reason, then `pause`; each pass pauses each CLI start with no `paused` line after it; a CLI start and that pause set `pause_announced`, so `announce_pause` does not post for it; a launch of that CLI begun after the error's latest failure (the `(since, count)` it saw going on at launch is still the occurrence's) ending without Transient/Fatal → `succeeded`; a launch begun before that failure clears nothing | Direct call | resilience_int: one M-08 naming the cause, then M-14; interrupted before the pause → restart pauses, one M-08; the `paused` append fails after the halt file is written → one M-08 and nothing launched; a launch of that CLI begun before the failure ends well → the error stays open, then a launch after `!resume` → M-14 |
| Filter | `intake.rs take_filter_run`, `log_verdict`, `tick` | `filter-failed` via `log_verdict` | `failed(Filter)` inside `log_verdict`; fresh `Admitted`/`Dropped` in `take_filter_run` → `succeeded(issue)`; `clear_removed_filter` each pass → `succeeded(removed)` | Direct call | intake_int |
| Notice delivery | `lib.rs tick` | — | `deliver_error_notices` after `reload_config`, before `take_channel_events` | Direct call | resilience_int with Slack stub (refuse; accept and lose the answer; restart) |
| Old reachability record | `lib.rs recover` | read by `records::unreachable` | `import_unanswered` | Direct call | resilience_int and dashboard_int upgrade case |
| Event details | `pulls.rs merge`, `flow.rs round_progress` → `issues.rs`, `sprint.rs post_sprint_decision` → `owner.rs send_decision` | Merge SHAs, issue facts, decision request | Merge details on success, round summary on completed checks, decision details on confirmed post | Existing daemon writes | Drive normal writers into the real Events API before block 7 |
| Events read | `api.rs routes` | — | `GET /events` → `events::page` | Route | dashboard_int |
| Notices switch | `settings.rs save`, `reload_config` | — | `Notices` section; `config-reloaded.changes` | Section enum | settings unit test, daemon reload test |
| Events in the header | `App.tsx`, `Shell.tsx area()`, `AppHeader` | — | route `events`, area `events`, link | Route | header fixture e2e |
| Sprint anchor | `SprintsView.tsx`, `SprintsPage.tsx` | — | `id="sprint-<id>"`; scroll once, on the first render that holds the target | Fragment | events fixture e2e: an ended sprint with a delayed sprints answer; the current sprint with the overview answer delayed after the sprints answer |

### Main Components

#### errors.rs (daemon)
- **Responsibility**: the one owner of "an error": the error kinds, the start/repeat/clear transitions, the derived state of errors going on and of waiting notices, delivery, the import of the old record, and the M-08 (for CLI errors), M-13 and M-14 text.
- **Interface**: `ErrorKey`; `Errors::from_lines(&[Value])`, `Errors::apply(&Value)`, `Errors::open()`, `Errors::waiting()`, `Errors::unreachable()`; `Daemon::failed`, `Daemon::succeeded`, `Daemon.github_clear` (the pass's pending GitHub clear), `Daemon::deliver_error_notices`, `Daemon::clear_removed_filter`, `Daemon::import_unanswered`.
- **Dependencies**: `Store`, `records::event`, `records::lines`, `download::mask`, `Channel::post_notice`, `Config.notices`, `Paths.dashboard`.

#### events.rs (daemon crate, read side, used by the server)
- **Responsibility**: turn `events.jsonl` into the page's lines: numbering, attribution, folding, told state, filters, window, masking.
- **Interface**: `events::page(store, &EventsQuery, secrets) -> Result<EventsPage, DaemonError>`.
- **Dependencies**: `records::lines`, `records::issues`, `records::launches` (starts), `history::ended_sprints`, the sprint record, `errors::Errors`, `download::mask`, `Config` (notices).

#### Events page (dashboard)
- **Responsibility**: S-08 as the UI Spec defines it; wording through `vocabulary.ts`.
- **Interface**: route `/events`; query `show`, `issue`.
- **Dependencies**: `GET /api/v1/events`, `usePoll`, `Contact`, `ExternalLink`, Activity's select styling.

### Selected Design

#### Errors

An error is one of the operations below that failed. The four kinds are the ones the UI Spec's Errors table words. They are the failures that repeat on their own, pass after pass, without stopping anything, or that pause the factory until a CLI is fixed. Without a notice the owner would never hear of the first kind. Each other failure is excluded for a stated reason:
- An operation failure that stops an issue or a sprint after two passes (`operation-failed`) already ends in that issue's or sprint's stop, which the owner sees.
- A pass failure that pauses already posts M-08. An agent CLI that cannot be spawned (`start_process`, `DaemonError::Launch`) is one of these two cases: it stops the issue or sprint that owns the item, or, with no owner, it is a pass failure.
- Slack's own failure cannot be told in Slack, and Now shows it as a prerequisite.
- Per-issue tracker write errors (`tracker-error` with `issue`, `issues.rs:411,455,561`) and `agreements-error` stay plain events. They are bound to one issue's or one pull request's operation, not to a service the owner must fix.

| `ErrorKey` (serialized) | Starts (failure) | Repeats | Clears (success) | Event of a failure |
|---|---|---|---|---|
| `github-unreachable` | `settle_for` sees an unreachable error (`via: api` for `HttpError::Transient`, `via: git` for a Git remote failure when the authenticated Git endpoint is unreachable) | Each such failure and each clear probe that detects continued unreachability | A pass in which every recorded `via` answers its probe in `poll_pull_requests` and no later GitHub operation fails as unreachable. The clear is appended at the end of the pass. | `service-unreachable {operation, error, why, issue?/sprint?/pass?}`. A probe that detects continued unreachability writes one with `operation: "checking that GitHub answers"` (`api`) or `"checking that origin answers"` (`git`). |
| `tracker-unreachable` | `poll_tracker` gets `HttpError::Transient` | Each poll | `poll_tracker` reads the tracker, or the tracker refuses (it answered) | `service-unreachable {operation: "reading the tracker", error, why, tracker}` |
| `tracker-refused` | `poll_tracker` gets any other error | Each poll | `poll_tracker` reads the tracker | `tracker-error {error, tracker}` |
| `cli-<claude\|codex\|opencode>` | `finished` reaches the `launch-error` branch, which pauses the factory | Another such failure of that CLI while open | A later launch of that CLI ends with no Transient or Fatal failure | `launch-error {item, error, cli}`; the start's `failure` also carries `reason`, the pause reason |
| `filter` | `log_verdict` logs `filter-failed` | Each later `filter-failed`, any issue | `take_filter_run` inserts a fresh `Admitted` or `Dropped` verdict (the cleared line names that issue); `clear_removed_filter` finds `tracker.filter` absent while the error is open (`removed`) | `filter-failed {issue, error, command, output}` |

The clear is tied to the same operation as the start:
- **GitHub**: `viewer` counts any result other than `Transient` as an API answer. For Git, `stops::origin_answers` probes `HEAD` on the same authenticated HTTPS endpoint as publication. A recognized authentication or authorization denial counts as an answer and uses the normal operation failure path on the issuing operation. A probe that fails before reaching the endpoint because of local configuration leaves the incident open without a repeat. A probe tells only that a way to GitHub answers again: `viewer` can answer while the pull read that failed still gets 502 (`pulls.rs:320-339`, `github.rs:343-359`), and the Git endpoint can still be unreachable while the API answers. So the probes only make the clear pending (`Daemon.github_clear`, set in `poll_pull_requests` and reset at the start of each pass). Every `failed(GithubUnreachable)` resets it, and the pull reads that follow the probes in `poll_pull_requests`, and the board repairs and launches after them in `tick`, run first. `tick` appends the clear only after its last part, when it is still pending. An operation that still fails as unreachable in that pass is a repeat of the open occurrence, not a clear and a new start (finding I005). A pass that ends early on a store failure leaves it pending for nobody, since the next pass resets it.
- **Filter**: a verdict `intake_pass` reapplies from its cache is an earlier judgment, not a new success, so only `take_filter_run` clears. `clear_removed_filter` runs every pass before the halt check. It checks the configuration in force, not a change of it, so a restart with the command already gone clears the error once.
- **CLI spawn**: a program that cannot be spawned is not an error. It keeps the core rule for a failed launch: the item goes back to `open`, the next pass launches it again, and a spawn failure that has passed by then costs nothing. Failing on two passes in a row stops the issue or sprint that owns the item (`settle`, `settle_sprint`) and files `exception`; an item with no owner is a pass failure of "launching steps", and two in a row pause the factory and `announce_pause` posts M-08, as today. Making it an error would pause at its first failure, removing that retry (finding I004).

The `cause` that tells repeats apart (and appears in the sentence) is:
- for `*-unreachable`: "HTTP <status>" for an answer with an error status (`ServerError`, `RateLimited` on 429), as the binding prototype's start line and M-13 show ("HTTP 502", `docs/ui-spec/assets/events/prototype.html`, `EV` and `#/slack`); "rate limited" for GitHub's 403 rate limit; "no answer" for `NoConnection` and for an unreachable authenticated Git endpoint; together with `operation`. `HttpError::Transient` carries the answer's status (`status: Option<u16>`, set by `http::check`), so the cause comes from the answer, not from parsing the message (finding I011);
- for `tracker-refused`: `HTTP <status>` for `HttpError::Status`, else the error text;
- for `cli-*`: the error as the CLI wrote it;
- for `filter`: `<error>` and, when stderr printed a line, ` (<output>)`.

Every `cause`, `error`, `command` and `output` is masked, then cut to 300 bytes at a character boundary, before it is written.

#### Error state (derived from `events.jsonl`)

There is no second record. `Errors` is a fold over the lines in order:

```rust
struct Errors { open: BTreeMap<ErrorKey, Occurrence>, waiting: Vec<Waiting>,
                unpaused: Vec<String> } // pause reasons of CLI starts with no `paused` line after them
struct Occurrence {
    since: String,               // failure.since of the start line: the occurrence's identity with the key
    last: String, count: u32,    // from each failure line of (key, since)
    first_issue: Option<String>, // the start line's issue
    cause: String,               // the latest failure line's cause
    why: Option<Unanswered>,     // the latest *-unreachable line's why (Now's reason)
    via: BTreeSet<Via>,          // github-unreachable: the ways recorded failing (api, git); each must answer its probe
    command: Option<String>,     // filter: the latest filter-failed line's command, as recorded
    tracker: Option<TrackerKind>, cli: Option<Cli>,
    start_seq: usize,
}
struct Waiting { key: ErrorKey, since: String, phase: Phase /* started | cleared */, line: Value }
```

`apply(line)`:
- A line with `failure.start` opens `(key, since)`. When its `notice` is `waiting` (M-13) or `pause` (M-08), it adds a `Waiting started`.
- A line with `failure` for the open `(key, since)` raises `count`, sets `last`, `cause`, `why`, `command`, and adds each of its `via`. A `failure` line whose key is not open is ignored. That happens only when its start was never appended, and then the daemon's own fold, built the same way, also treats the next failure as a start.
- A `cli-*` start adds its `failure.reason` to `unpaused`. A `paused` line removes that `reason` from `unpaused`. Neither depends on the occurrence still being open: the start commits its pause, and a later success of that CLI does not withdraw it.
- `error-cleared` removes `(key, since)`. When its `notice` is `waiting`, it adds a `Waiting cleared`.
- `error-notice {failure: {key, since}, phase, outcome}` removes that `Waiting`.

Each line is appended by `Store::append_line`, which this change makes start on a line of its own: it opens the file for reading and appending, and when the file is not empty and its last byte is not `\n`, it writes `\n` before the line, in the same `write_all`. A part left by an interrupted write then stays one unparsed line of its own, and the next appended line parses. This is the store's rule for every JSON Lines file (`events.jsonl`, `launches.jsonl`), so no caller handles it. The reader holds the other half of the same rule: `records::lines` reads the file as bytes, splits it on `\n`, and parses each line with `serde_json::from_slice`, so a line that is not valid UTF-8 (a write cut inside a multibyte character) is one skipped line like any other torn line, not a failure of the whole file (finding I002). `Ok` from `append_line` therefore means the line is a parsed line of the file, which is what lets the fold advance on it.

Transitions:
- `failed(key, failure)`: the daemon's `Errors` decides start or repeat. A start writes `failure: {key, since: now, cause, via?, start: true, notice, reason?}`, where `notice` is `pause` for `cli-*`, `waiting` when notices are on, and `off` otherwise. A repeat writes `failure: {key, since, cause, via?}`. Every string in the line is masked with the held credentials. The line is appended, and only then is `Errors.apply` called on it. When the append fails, the error propagates as a store failure of that operation, `Errors` is unchanged, and the next failure starts again. So no start is lost and no repeat is ever written without a start.
- `succeeded(key, clear)`: a key that is not open does nothing. An open key appends `error-cleared {failure: {key, since, notice: "waiting"|"off"}, count, issue?, removed?, tracker?, cli?}`, then applies it. When the append fails, the key stays open and the next success clears it.
- A CLI start is appended first, with `failure.reason` set to the pause reason `finished` builds today. Then `pause(reason)` writes the halt file and the `paused` line. Its M-08 is that start's waiting notice, posted by delivery. The start line is the commit: it decides both that the factory pauses and that this pause's M-08 is the start's marked notice. Two rules follow from the start line, not from whether the occurrence is still open or a step succeeded, so one M-08 goes out and nothing launches under it (finding I004):
  - **The pause follows the start.** At the start of every pass, after the finished launches are taken in and before `announce_pause` and the halt check, and so also before the first pass after a restart, `pause(reason)` runs for each reason in `unpaused`. This repairs an interruption, or a failed halt-file write or `paused` append, after the start line. It runs whether or not the occurrence is still open, because the waiting M-08 says the factory paused. `pause` rewriting an existing halt file is harmless. An owner's `!resume` comes after the `paused` line, so the reason is no longer in `unpaused` and is not paused again.
  - **One announcement per pause.** Applying a `cli-*` start in the daemon sets `pause_announced`, and so does each `pause` the first rule runs. So `announce_pause` never posts for a pause a CLI start committed; that pause's M-08 is the start's marked notice, and delivery posts it whether or not the occurrence has cleared since. A restart sets `pause_announced` from the halt file's existence, as today. Every other pause is announced as today.
- `Errors::unreachable()` maps open `github-unreachable` and `tracker-unreachable` to `BTreeMap<Service, Failing { since, why }>`, so `overview::status` and `poll_pull_requests` keep their logic. The Now page's "since" and the Events page's "since" come from the same `since`.

#### Delivery

`deliver_error_notices` runs each pass after `reload_config` and `clear_removed_filter`, before `take_channel_events` and the halt check:
- While notices are off, each waiting M-13/M-14 gets `error-notice {failure: {key, since}, phase, outcome: "dropped"}`. A waiting M-08 is not dropped: M-08 does not depend on the switch.
- Otherwise each waiting notice, in order, is built from its line (masked again) as a `Notice` with `marker: Some("<key>@<since>/<phase>")`, and posted. For `kind` `Paused` the text is M-08, otherwise `ErrorStarted` or `ErrorCleared` with M-13/M-14 text. The marker identifies the transition, not the attempt. Slack's `post_notice` for a marked notice:
  1. reads the channel's messages since the transition line's time (`conversations.history` with `oldest` = that time less one hour, as a Slack `ts`, `include_all_metadata=true`, `limit=200`), following `response_metadata.next_cursor` until Slack returns none, and returns the message whose metadata is `{event_type: "ovenspring_notice", event_payload: {id: <marker>}}`, when there is one. Slack may return fewer messages than `limit` together with a cursor, so the read ends only when no cursor is returned, not when a page is short ([pagination](https://docs.slack.dev/reference/methods/conversations.history/#pagination)). A message for this transition is posted only after its line is appended, so it is not older than the line by this computer's clock. The hour is an assumed margin for a difference between this computer's clock and Slack's (named as an assumed constant in code); it bounds the read without missing the message. The read is in the channel in force: when the owner moves the factory to another channel between Slack accepting a notice and its `told` line, the notice is posted once in the new channel, where the owner now reads the factory, and the abandoned channel keeps its copy. The owner chose this outcome and asked that no mechanism be added for it (owner's answer, `sources/answers/GH-9-spec-16.md`: "A. A duplicate in the old channel is fine. This case is rare, so add no mechanism for it."; finding I001; Design Boundaries);
  2. otherwise posts one `chat.postMessage` with that metadata. The notice texts are bounded (each variable part at most 300 bytes), so `pieces` gives one piece and no reply follows.
- `Ok(message)` appends `error-notice {…, outcome: "told", at: <message ts as RFC 3339>}` and applies it. `Err` stops this pass's posting; the drops of the bullet above still run for the notices after it, so after a pass while off only a waiting M-08 is left to post. The first `Err` for a waiting notice is logged once as `channel-error` (masked). "Once" is tracked in memory, so a restart may log it once more. The next pass starts again from step 1. So a message Slack accepted, whose answer was lost or whose `told` line was never appended (crash, failed append), is found and recorded, not posted twice.

#### Notice text (M-08 for CLI errors, M-13, M-14)

Times are the daemon's local clock, or "Nov 26, 23:58" when not today. The prototype's `#/slack` page is binding wording for M-13 and M-14:
```
*Error: <name, lower-cased after the colon>.* <what failed, with the issue key and the cause>
<what the factory does meanwhile>. You will hear once more when it clears.
<{dashboard}/events|See it on Events>        (this line only when Paths.dashboard is known)
```
- `github-unreachable` / `tracker-unreachable`: "*Error: can't reach <Service>.* It failed while <operation>: <cause>." The operation's "its" becomes "<KEY>'s" when the failure concerns an issue. Then "The factory tries again<interval>.", where the interval is " every minute" for the tracker and empty for GitHub (UI Spec, Errors). An imported start has no operation: "It has not answered since <since>: <cause>."
- `tracker-refused`: "*Error: <Tracker> refuses the connection.* <Tracker> refused the connection (<cause>)." / "No new issue or change is read from it until it is fixed."
- `filter`: "*Error: the issue filter fails.* `<command>` <error> on <KEY> (`<output>`)." / "Issues go on as without a filter until it works."
- `cli-*` start: M-08 as `announce_pause` words it, with the reason the pause recorded for that failure ("Oven Spring paused because the agent on <KEY> (<step>) stopped with an error: <cause>.", from the start line's `failure.reason`). Each CLI occurrence's M-08 carries its own cause, so marking it told concerns only that occurrence.

M-14: "*Cleared: <clear sentence, lower-cased after the colon>.* It failed <n> times from <time>." The clear sentences come from the UI Spec Errors table. A CLI's M-14 is "*Cleared: <CLI> starts again.* …". All text is masked with the held credentials before posting.

#### Event details

**Merged pull request.** In `pulls.rs::merge`, retain the result of the last existing `gh.pull(subject.pr)` read, which already checks the head. Extend `github.rs` `RawPull` and `Pull` to carry the optional `base.ref` returned by that read. The factory uses the configured branch for the existing merge gate. The event names the PR target observed in the last read. On `MergeOutcome::Merged` write `merged {pr: subject.pr, base?, sha, merge_commit}` with the same issue or sprint from `Subject::named`. An absent base leaves the field absent and does not prevent a merge. The GitHub merge response confirms success and returns the commit SHA. The preceding pull read supplies the base, with no extra GitHub call. The repository fixture in `crates/integrations/tests/github_contract.rs` already returns `base.ref`; assert that the adapter carries it through. [GitHub REST API for pull requests](https://docs.github.com/en/rest/pulls/pulls#get-a-pull-request)

**Review round.** `flow::round_progress` signals completion once when the pending reviews, queued units and rerun check have all finished and the round advances to `ReviewResponse` or converges to pushing. Capture `Round.number` before convergence resets it. The stuck and escalation exits keep their existing failure facts. Use an internal `Action::RecordReviewRound {number}` before the next filing or push. The daemon handles it after saving the new flow state and the existing `FactEvent`. Where the round is handing findings to `ReviewResponse`, obtain the count from `handoff::handed` for `Origin::Round`, the same output keys and entries the correction receives. Count one per entry, including each maintained applied finding returned as still open. Equal IDs from different reviews are separate entries. For a round that converged directly, record zero. Append `review-round-ended {issue, round, findings}` once for that completion. Declining the findings later, replaying an already consumed completion, or starting a recheck adds no second end for that round. The next completed recheck has the incremented number. Existing issue facts continue to describe each transition.

**Sprint proposal and decision delivery.** At `sprint.rs::post_sprint_decision`, for `DecisionKind::Sprint` with a plan, copy `plan.issues.len()` and the masked `plan.purpose` into `DecisionRecord.proposal` before saving the existing Unsent decision. Use its existing `sprint` ID. The optional proposal summary is immutable for that decision, and absent when loading older decisions. It contains the count and purpose of the plan used for that post, so delivery after a restart or retry uses the very same summary. Mask purpose with the held credentials before storing the summary. Keep the original Slack message, buttons and retry path.

Move the `decision` event from `decide` to the common success branch of `send_decision`. Write it after `Channel::post_decision` returns its message and after saving Pending with that message, at the existing successful delivery boundary. Keep the existing best-effort details and attachment follow-ups running even if the event append fails, then return the append error through the existing store-error path. A refusal stays Unsent, logs its existing `channel-error`, and emits no successful `decision` line. `deliver_decisions` calls the same function on Unsent records, so an accepted retry writes the same event once. The existing adapter returns success after posting the root and any initial text continuation. As with other decision writes, a failure after Slack accepts but before the local status save can retry the post. An append failure preserves Pending and uses existing store-failure handling. These records use the existing event append and decision delivery semantics.

**Wording from recorded fields.** Keep attribution and recursive masking in `events::page` and English in `vocabulary.ts`. The following uses the existing code and quote segments.

| Record | Wording |
|--------|---------|
| `merged` with PR 86 and base `main` | “Merged pull request #86 into `main`.” With only the number, “Merged pull request #86.” With neither, retain “Merged its pull request.” or the existing working-agreements sentence for a sprint subject. |
| `review-round-ended` with round 1 and count 3 | “Review round 1 ended with 3 findings.” Use “1 finding” for one and “0 findings” for zero. Historical `fact` lines retain their current wording. |
| `decision` with `posted: true`, kind `sprint`, and proposal summary | “Proposed the sprint in Slack: 3 issues toward “<purpose>”” using the prototype's quote treatment. Use “1 issue” for one. When the summary is absent, use “Proposed the sprint in Slack.” Other posted decisions use their existing kind-specific sentences. |
| Legacy `decision` without `posted: true` | “Prepared a sprint proposal.” for a sprint proposal and “Prepared a decision for you.” for other decisions. The recorded decision kind and issue remain available. |

#### Events read (`events::page`)

1. Read `events.jsonl` with `records::lines`; `seq` is the 0-based index of a parsed line. Build `Errors` from the same lines.
2. Attribute:
   - an `issue` string that names an issue record, or names no sprint record, gives the issue;
   - otherwise an `issue` string naming a sprint record (the current sprint or `sprints/<id>.json`) gives that sprint (old `delete_branch` lines);
   - otherwise a `sprint` string gives the sprint;
   - otherwise an `item` gives the issue or sprint of that item's `LaunchStart` (first match in `launches.jsonl`);
   - otherwise neither.
3. Occurrences: a line with `failure` belongs to occurrence `(failure.key, failure.since)`. The start line is the one with `failure.start`. A non-start line with the same `(issue, cause)` as an earlier listed line of the occurrence folds into it: it raises `times` and sets `last`. Otherwise it is an Error repeat line that refers to the start line's time. An occurrence is open when `Errors.open` holds `(key, since)`.
4. Told state of a start/cleared line:
   - `told {at}` when an `error-notice` with `outcome: "told"` exists for `(key, since, phase)`;
   - `off` when the line's `notice` is `"off"` or a `dropped` notice exists;
   - `waiting` otherwise (`notice` `waiting` or `pause`).
   `error-notice` lines are not listed; they only supply this state.
5. Filters: `show=errors` keeps lines with `failure` and `error-cleared` lines. `show=filter` keeps `filter-dropped` and the lines of key `filter`. `issue=<KEY>` keeps lines attributed to it.
6. Window: without `from`, the newest 15 matching lines; with `from`, every matching line with `seq ≥ from`. `older` is the `from` that also includes the next 50 older matching lines, or none when no older line matches. `recorded` is true when any listed line exists before filtering.
7. Masking: every string of every served value (`fields` recursively, causes, commands, titles, purposes) is masked with `download::mask` and the held credentials. That covers lines written before this change.

An unreadable answer has empty `lines`, `errorsNow` and `issues`. The Issue control displays only “All issues,” because its options come from successfully read event data (UI Spec, EventFilters).

The client keeps its window anchored: after the first answer it sends `from` = the smallest `seq` it shows, and "Show older events" sets `from` to `older`. Every poll then returns the lines it shows plus new arrivals. `usePoll`'s key is only `show` and `issue`, and the load reads `from` from a ref, so a changed `from` asks again (`refresh()`) without dropping the answer on screen. A changed filter changes the key, which resets `from`, as the UI Spec requires.

#### Sprints anchor

`SprintsView` gives the "This sprint" group and each `SprintRow` `id="sprint-<id>"`. The two kinds of target render from two requests: the ended sprints from `SprintsPage`'s own poll, and "This sprint" only once `Shell`'s separate overview poll has answered (`Shell.tsx:29-34`, `SprintsPage.tsx:8-26`). Either can answer first. So `SprintsPage` keeps, in a ref, the hash it has already scrolled to. After each render (an effect on the hash, its poll's data and `overview.data`), when the hash is not that one and `document.getElementById` finds its element, it scrolls the element into view and records the hash. A target that is not rendered yet is tried again on the next render, so the scroll happens on the first render that holds it, whichever request brings it (finding I009). A target that never renders (a sprint S-03 does not list) is never scrolled to. Once scrolled, later polls do not move the page; a new hash scrolls again.

#### Settings changed

`reload_config` compares the old and new `Config` as JSON and adds `changes: [{setting: "<table>.<key>", value?}]` (leaf paths; arrays are compared whole) to `config-reloaded`. `value` is recorded only when the new leaf is a boolean or a number. A string or array leaf (a filter command, a role's `append` prompt, a model, a channel, a list of skills) is recorded by its path alone, because its text is the owner's free text and can hold a credential, which never enters the log (PRD Non-Functional Requirements, Security; finding I007). Masking cannot replace this: `download::mask` removes held credentials and common token formats, not every secret an owner can type. It applies the notices switch from the next pass. The vocabulary words `notices.errors` as "Error notices are now on/off.", another key with a value as "<label> is now <value>." (the prototype's "Issues at once is now 3."), and a key without one as "<label> was changed." It uses the Settings row labels and falls back to the dotted key.

#### Filter output

`judge` pipes stderr and reads its first line, up to 4 KiB, on a thread, as it does stdout, then drains the rest. `Outcome::Failed` carries `{error, output}`. `take_filter_run`, which already masks a `Dropped` reason there with `run.secrets` (`docs/design/issue-filter-design.md`, Taking a run; Credentials for masking), masks a `Failed` outcome's `error`, `output` and the `[tracker] filter` command in force (a changed setting voids the run, so it is the command that ran) with the same secrets, then cuts each to 300 bytes, and stores only that in the verdict. A credential the cut would split is already replaced. So the verdict that waits for publication, for example while the sprint is Proposing, holds no credential the daemon held during the run, even one replaced before `log_verdict` writes it; `log_verdict` writes the stored values into `filter-failed`, through `failed(Filter)`, which masks again with the credentials held then (finding Conflict-001).

#### Added design surface

- **Addition**: event kinds `error-cleared` and `error-notice`, and the `failure` annotation, as the one record of errors
  - **Current evidence**: AC-077 "told once … however often it repeats", across restarts; UI Spec Delivery (kept until Slack accepts); UI Spec "same record" for Now and S-08; Cleared and told lines
  - **Lower-surface insufficiency**: in-memory state re-announces after a restart (analysis invalidation). A separate state file needs a second write per transition, and an interruption between the two writes loses or duplicates transitions (finding I002). No existing event marks a success.
  - **Subtraction result**: AC-077 fails on restart or on a Slack outage, and the Cleared and told lines cannot be shown
- **Addition**: `Notice.marker` and the Slack lookup before a marked post
  - **Current evidence**: AC-077 and UI Spec Delivery ("posts it once Slack accepts messages again, once"); Slack can accept a post whose answer the daemon never receives, and the daemon can stop between the post and its `told` line
  - **Lower-surface insufficiency**: Slack's `chat.postMessage` takes no idempotency key. Without a marker, a retry cannot tell an accepted message from a refused one. The metadata and history reads already exist for decisions (`post_decision`, `mark_resolved`).
  - **Subtraction result**: a lost answer or a crash during delivery posts the same notice twice
- **Addition**: `GET /api/v1/events` and `events.rs`
  - **Current evidence**: AC-076; no endpoint reads events
  - **Lower-surface insufficiency**: `download` is scoped to one issue or sprint and returns JSONL for a file, not the page's lines
  - **Subtraction result**: AC-076 is unmet
- **Addition**: merge fields, review-round summary event, and posted decision fields with an optional proposal snapshot
  - **Current evidence**: AC-076 and UI Spec Events wording refer to the binding prototype's merge, review and proposal sentences. The production writer currently omits these facts or writes before the post.
  - **Lower-surface insufficiency**: merge SHAs carry neither number nor target, a fact with stages carries neither the round identity nor the actionable handoff count, and a decision request proves no post. Reconstructing them at read time would use mutable PR, review or sprint state. The writers already own those facts when they happen. The existing Unsent decision is the only input to retry and can carry its own summary.
  - **Subtraction result**: one or more approved sentences becomes unrenderable from production records, or the page claims a Slack post before it succeeds.
- **Addition**: `recorded` in the response
  - **Current evidence**: UI Spec EventsPage "Filtered to nothing" and "Nothing recorded" are different states
  - **Lower-surface insufficiency**: an empty window and an empty `issues` list do not tell them apart when every line is factory-wide
  - **Subtraction result**: a nonempty log with no match shows "Nothing has been recorded yet."
- **Addition**: `[notices] errors` and `SettingsSection::Notices`
  - **Current evidence**: AC-078; UI Spec ErrorNoticesRow "Saving checks no connection"
  - **Lower-surface insufficiency**: a key in `[slack]` is saved with the Slack connection check (analysis invalidation)
  - **Subtraction result**: AC-078 is unmet, or the owner cannot turn notices off while Slack fails
- **Addition**: `NoticeKind::ErrorStarted`, `ErrorCleared`
  - **Current evidence**: M-13, M-14
  - **Lower-surface insufficiency**: existing kinds carry other meanings (Paused = M-08)
  - **Subtraction result**: M-13 and M-14 cannot be posted
- **Addition**: `config-reloaded.changes`
  - **Current evidence**: UI Spec "Settings changed: …" lines and "Turning notices off or on is recorded as an event"
  - **Lower-surface insufficiency**: the existing event names no setting
  - **Subtraction result**: those lines cannot be worded
- **Addition**: stderr first line in `filter-failed.output`
  - **Current evidence**: UI Spec Errors table "(<its output>)" and the binding prototype
  - **Lower-surface insufficiency**: stderr is discarded today
  - **Subtraction result**: the approved start line cannot be shown

### Data Contracts

#### GET /api/v1/events

```yaml
Contract: GET /api/v1/events?show=all|errors|filter&issue=<KEY>&from=<seq>
Input:
  Preconditions: all parameters optional; show defaults to all
  Validation: unknown show or non-numeric from → 400 ApiErrors {show|from: message}
Output:
  Type: EventsPage (ts-rs)
    lines: EventLine[]          # newest first
    older: number | null        # `from` that adds the next 50 older matching lines
    recorded: boolean           # some listed line exists before filtering
    unreadable: boolean         # events.jsonl exists but could not be read; lines, errorsNow and issues empty
    errorsNow: ErrorNow[]       # every open error, oldest start first, not narrowed by filters
    notices: boolean            # current [notices] errors
    issues: {key, title|null}[] # every issue key in any line, sorted
  EventLine:
    seq: number; at: string; kind: string   # kind = the line's `event`
    fields: Record<string, JsonValue>       # the line's other fields, masked
    issue: {key, known: boolean} | null     # known: the factory has an issue record
    sprint: {id, purpose|null, listed: boolean} | null  # listed: S-03 lists it
    error: null
      | {role: started, key, cause, times, last, open: boolean, told: Told}
      | {role: repeat,  key, cause, times, last, startedAt}
      | {role: cleared, key, startedAt, times, removed: boolean, told: Told}
    Told: {state: told, at} | {state: waiting} | {state: off}
  ErrorNow: {key, since, last, times, firstIssue: {key, known}|null, cause,
             tracker: TrackerKind|null, cli: Cli|null, command: string|null}
  Guarantees: every string masked; a line of unknown kind is still listed; error-notice lines never listed
  On Error: a read failure of events.jsonl → 200 with unreadable: true; other failures → 500 ApiErrors
Invariants:
  - seq of a line never changes (append-only file)
  - errorsNow, the open flags and Now's unreachable come from one Errors fold over events.jsonl
```

Sources: `started.cause` is the start line's cause; `times`/`last` count the lines folded into it. `repeat` is the same for its line, and `startedAt` is the start line's `at`. `cleared.times` is the cleared line's `count`, `startedAt` is the occurrence's `since`, and `removed` is its `removed` flag. The cleared line's subject is its `issue` (the issue whose fresh judgment cleared the filter). `ErrorNow` is `Errors.open`: `cause` and `command` from the latest failure line, `firstIssue` and `tracker` from the start line. So after the command changes in Settings, Errors now still names the command that failed.

`listed` holds when the sprint is in `history::ended_sprints` or is the current sprint shown as "This sprint" on S-03 (the same condition `SprintsView` uses). `purpose` comes from the sprint's record.

#### Notices section

```yaml
Contract: PUT /api/v1/settings/notices
Input: {errors: boolean}  (deny_unknown_fields)
Output: Settings (as every section); config.toml [notices] errors written
Invariants: no connection check; other tables unchanged
```

#### Marked notice (daemon → Slack)

```yaml
Contract: Channel::post_notice(&Notice { marker: Some(id), .. })
Input: id = "<error key>@<since>/<started|cleared>"; text one message (≤ MARKDOWN_MAX)
Output: MessageRef of the message carrying metadata {event_type: "ovenspring_notice", event_payload: {id}},
        found among every channel message from an hour before the transition line's time (all history pages) or newly posted
On Error: ChannelError when any history page or the post fails; nothing is recorded as told
Invariants: at most one chat.postMessage per call; never a reply
```

### Field Propagation Map

| Field | Boundary | Status | Serialized Format | Consumer Parse Rule | Detail |
|-------|----------|--------|-------------------|---------------------|--------|
| `failure` | daemon → events.jsonl → `Errors` (daemon at start, overview, events.rs) | preserved | `{"key":"github-unreachable","since":"2026-11-27T04:41:00Z","cause":"HTTP 502","via":["api"],"start":true,"notice":"waiting"}`; `start`/`notice` only on the start line; `reason` only on a `cli-*` start (the pause reason); `imported:true` and `via: ["api","git"]` on an imported start | `ErrorKey::from_str` on `key`; `since` string compared exactly; `via` an array of `api`\|`git` | Occurrence identity is `(key, since)` |
| `error-cleared` | daemon → events.jsonl → `Errors`, events.rs | preserved | `{"event":"error-cleared","failure":{"key":"filter","since":"…","notice":"waiting"},"count":4,"issue":"ACME-12"}`; `removed:true` for a removed filter | as `failure` | `issue` only when the success concerned an issue |
| `error-notice` | daemon → events.jsonl → `Errors`, events.rs | preserved | `{"event":"error-notice","failure":{"key":"…","since":"…"},"phase":"started","outcome":"told","at":"…"}` | `outcome` `told`\|`dropped` | Never listed |
| notice marker | daemon → Slack metadata → Slack history → daemon | preserved | `metadata: {"event_type":"ovenspring_notice","event_payload":{"id":"filter@2026-11-27T04:41:00Z/started"}}` | exact match of `event_type` and `id` among the messages from an hour before the line's time, every history page read | `decision_id` reads `ovenspring_decision` only, so decisions ignore it |
| `ErrorKey` | events, API | preserved | `github-unreachable`, `tracker-unreachable`, `tracker-refused`, `cli-claude`/`cli-codex`/`cli-opencode`, `filter` | Unknown key → line listed as Plain | One `Display`/`FromStr` pair in `errors.rs`; TS union generated |
| `notices.errors` | config.toml → Config → daemon / server | preserved | `[notices]\nerrors = false`; absent = true | serde default true | `distribution-design.md` Configuration lists it |
| `changes` | daemon → events.jsonl → vocabulary | preserved | `[{"setting":"notices.errors","value":false},{"setting":"tracker.filter"}]` | Known setting → its label; else the dotted key; no `value` → "was changed" | `value` only for boolean and number leaves; no string is written |
| `pr`, `base` | GitHub → daemon → `merged` line → API `fields` → vocabulary | transformed | GitHub `base.ref` string → `"base":"main"`; `"pr":86`; existing `sha` and `merge_commit` retained | Positive safe integer PR number, nonempty string base, historical fields may be absent | `base` optional at decode; masked before append and at read |
| `round`, `findings` | flow and correction handoff → `review-round-ended` line → API `fields` → vocabulary | transformed | `{"event":"review-round-ended","issue":"ACME-142","round":1,"findings":3}` plus `at` | Positive safe integer round, nonnegative safe integer count; an incomplete line uses “Review round ended.” | Count entries passed to correction; keep existing `fact` rows |
| `proposal` | sprint plan → isolated `decisions/<id>.json` → posted `decision` line → API `fields` → vocabulary | preserved | Optional object `"proposal":{"issues":3,"purpose":"Shoppers can find a product by what it does, not only by its name."}` | Serde defaults absent object to none; dashboard accepts a nonnegative safe integer count and nonempty string purpose | Counts plan entries at decision creation. Purpose is a masked string. |
| `posted`, `sprint` | decision delivery → `decision` line → API and vocabulary | preserved | `{"event":"decision","id":"sprint-20261127-090200-sprint-1","kind":"sprint","issue":null,"sprint":"20261127-090200","posted":true,"proposal":{"issues":3,"purpose":"Improve product search."}}` plus `at`; omit absent `sprint` and `proposal` | `posted === true` selects sent wording; a nonempty string `sprint` supplies the existing attributed subject | Keep `id`, `kind` and `issue`; existing `sprint` ID comes from the saved decision |
| `output`, `command` | judge → `take_filter_run` (verdict) → filter-failed → page, M-13 | transformed | strings, masked with the run's secrets in `take_filter_run`, then cut to 300 bytes, before the verdict is stored | Shown in the mono face | stderr's first line read up to 4 KiB |
| `sprint` on `service-unreachable`/`operation-failed`/`github-error` | stops.rs, issues.rs → events | transformed | sprint id string (was `true`, or the id in `issue`) | Non-string `sprint` ignored; `issue` naming only a sprint record → sprint | Old `sprint: true` lines unattributed |
| `from`, `older` | dashboard ↔ server | preserved | decimal `seq` in the query | `u64` parse; invalid → 400 | The client anchors `from` at its oldest shown line; a filter change resets it |

### State Transitions and Invariants

```yaml
State Definition:
  - Per ErrorKey: Clear (no open occurrence) | Open{since, count, via}
  - Per transition notice: Waiting | Told | Dropped | Off (never queued)
  - All of it is the fold of events.jsonl; the daemon's copy is rebuilt at start
State Transitions:
  Clear → failure (start line appended) → Open (count 1; Waiting M-13 when on, Waiting M-08 for cli-*, else Off)
  Open → failure (repeat line appended) → Open (count+1; nothing waits)
  Open → success of the failed way | filter absent (error-cleared appended) → Clear (Waiting M-14 when on, else Off)
  Decision Unsent → confirmed post and Pending saved → append posted decision event (initial send or retry)
  Waiting → Slack holds the marked message (found or posted) → Told (error-notice told appended)
  Waiting M-13/M-14 → notices off → Dropped (error-notice dropped appended)
  Any → append fails → unchanged (the next failure/success/pass retries)
System Invariants:
  - Each transition is exactly one appended line; there is no second store to disagree with it
  - At most one Slack message per (key, since, phase): the marker is checked before every post
  - Notices leave in the order their lines were appended; a refusal blocks the ones behind it
  - A cleared or repeat line never exists for an occurrence without its start line in the fold
```

### UI Error State Design

| Component / Screen | Loading | Empty | Error | Partial |
|-------------------|---------|-------|-------|---------|
| EventsPage | Contact "not yet read" | `recorded` false → "Nothing has been recorded yet."; `recorded` true and no line → "No event matches this filter." | `unreadable` → "The events could not be read…" | Lost contact → page as last read with Contact's line |
| ErrorsNow | — | "Nothing is wrong now." | `unreadable` → "Not known while the events cannot be read." | — |
| ErrorNoticesRow | — | — | Save refused → reason under the radios | — |

### Client State Design

| State Category | State | Management Method | Sync Strategy | Reset/Clear Behavior |
|---------------|-------|-------------------|---------------|----------------------|
| Server state | EventsPage | `usePoll` keyed by `show` and `issue` only | 5 s poll; `refresh()` after `from` changes | A filter change (new key) starts empty, as `usePoll` does |
| URL state | `show`, `issue` | URL query (Activity's pattern) | — | — |
| Local UI state | `from` (anchor) | `useRef` read by the load, mirrored in `useState` for render | Set to the oldest shown `seq` after the first answer; set to `older` on "Show older events" | Cleared when the key changes (first 15 again) |
| Temporary state | Notices radio choice | SettingRow form | Save | Cancel restores |

### UI Action - API Contract Mapping

| UI Action | API Endpoint | Request | Response | Error Contract |
|-----------|-------------|---------|----------|----------------|
| Open Events / poll | `GET /api/v1/events` | `show`, `issue`, `from` (anchor once set) | `EventsPage` | Fetch failure → Contact; `unreadable` → Unreadable state |
| Show older events | same | `from = older` | `EventsPage` | as above; the shown lines stay while it loads |
| Save Error notices | `PUT /api/v1/settings/notices` | `{errors}` | `Settings` | 400/409 → reason under the radios |

### Error Handling

| Error Category | Example | Detection | Recovery Strategy | User Impact |
|---------------|---------|-----------|-------------------|-------------|
| External | Slack refuses a sprint proposal | `post_decision` Err | Keep the same Unsent decision and summary for `deliver_decisions` | No posted proposal line until successful delivery |
| External | Slack refuses M-13 | `post_notice` Err | Keep waiting, retry each pass with the marker lookup first, log `channel-error` once | Told line "Not told yet…" |
| External | Slack accepted, answer lost | `post_notice` Err after Slack stored it | Next pass finds the marked message → `told` | One message; told line shows when it was accepted |
| Infrastructure | Append to `events.jsonl` fails during `failed`/`succeeded`/delivery | `append_event` Err | `Errors` unchanged; the error propagates through `settle_part` as any store failure; the next pass retries the transition | Pass failure handling (pauses after two) |
| Infrastructure | `events.jsonl` unreadable at daemon start | `records::lines` Err | `recover` fails as for any unreadable record | The daemon does not start, as today for a broken record |
| Infrastructure | `events.jsonl` unreadable for the page | `records::lines` Err | `unreadable: true` | Unreadable state |
| Validation | `?from=abc` | Query parse | 400 ApiErrors | Not reachable from the UI |

## Implementation Plan

### Implementation Approach

**Selected Approach**: Vertical slices, with the error state and its delivery first.
**Selection Reason**: the riskiest premise is that one notice per transition reaches Slack exactly once, through restarts, refusals and a lost answer. The first slice proves it end to end on the one kind that already has start/clear state (unreachability). The page and the other kinds build on that state.

### Technical Dependencies and Implementation Order

1. **Error state and notices on unreachable services** (`append_line` terminating a torn line, `config.rs` NoticesConfig, `errors.rs`, `NoticeKind`, `Notice.marker` and the Slack lookup, `slack_stub` history and lost-answer mode, callers in `stops.rs`/`pulls.rs`/`intake.rs` for unreachability with `via`, `overview.rs`, `tick` delivery, `import_unanswered`)
   - Technical Reason: proves AC-077's once-semantics on the existing transitions, and replaces `services-unanswered.json` before anything else reads it
   - Prerequisites / Dependent Elements: none / every later slice
2. **Events writers, endpoint and page** (Event details writers, `events.rs`, `GET /events`, types and writer-to-API proof before dashboard vocabulary, fixture and block 7 changes)
   - Technical Reason: AC-076 requires the writers to supply the approved sentences and their subjects. Exercise the real writers with the Events endpoint before completing the fixture comparison. ErrorsNow uses slice 1's state.
   - Prerequisites: slice 1 (`failure` annotation, `Errors`)
3. **The other errors** (tracker refused; CLI from `finished` with its M-08 as the start notice; filter with stderr capture, fresh-judgment clear and `clear_removed_filter`; `stops.rs` sprint id; `delete_branch` subject)
   - Prerequisites: slices 1–2
4. **Switch in Settings and settings-changed lines** (`SettingsSection::Notices`, ErrorNoticesRow, `config-reloaded.changes`, vocabulary)
   - Prerequisites: slice 1 (the daemon already reads `notices.errors`)

### Migration Strategy

At start, after building `Errors` from `events.jsonl`, `import_unanswered` reads a leftover `services-unanswered.json`. For each service in it that has no open occurrence, it appends a start line `service-unreachable {why, failure: {key, since: <the file's since>, cause: <why's words>, via: ["api", "git"] for GitHub, start: true, notice, imported: true}}` and applies it. The old file was written for API failures and for git commands when `origin` did not answer (`stops.rs:98-101,144-152`), without saying which, so the imported GitHub occurrence names both ways and clears only when both probes answer (finding I006). Then it deletes the file. The occurrence keeps the outage's first-failure time and reason, so Now shows the same "since" and reason before any poll. The next confirmed answer clears it once. An interruption before the delete re-runs the import, which skips services already open, so no second start is written. The imported start waits for its M-13 like any start: the owner was never told of the outage. Its sentence has no "while <operation>", since the old file did not record one.

Old merge and decision events render from the fields they hold, using Event details' historical wording. A pre-upgrade Unsent decision can still post with the existing ID and message, and without a proposal summary. Neither history nor current records are backfilled.

Old event lines lack `failure` and are listed as Plain lines with their sentences. Old `sprint: true` lines show a blank subject. Old item-less lines show a blank subject where `launches.jsonl` cannot attribute them. Old `delete_branch` lines with a sprint id in `issue` show that sprint. An older binary refuses a `config.toml` that has `[notices]` (deny_unknown_fields), as with every added key.

## Security Considerations

- **Authentication & Authorization**: the endpoint is served on the same local listener and under the same rules as every `/api/v1` endpoint (ADR-0002); it adds no write.
- **Input Validation**: `show`, `issue` and `from` are parsed and validated; `issue` is compared as a string only.
- **Sensitive Data Handling**: the masking boundary is `download::mask` with the held credentials (the run's secrets for the filter), applied:
  1. to the new free-text event details (the merge base before append and the sprint purpose before saving its decision summary), and to every string of every line `errors.rs` appends (`failure.cause`, `failure.reason`, `error`, `command`, `output`, `channel-error`) before the append. The filter's `error`, `output` and `command` are masked earlier too, with the run's secrets when `take_filter_run` keeps the verdict and before the 300-byte cut, so a cut never leaves part of a credential and a credential replaced before the verdict is logged is still masked (Selected Design, Filter output);
  2. to every string the endpoint serves, old lines included;
  3. to every notice text before posting.
  No notice text is persisted; it is built from the masked line at delivery.
- **Settings values**: `config-reloaded.changes` writes no string value at all, only the setting's path, so a credential typed into a filter command or a role prompt never reaches `events.jsonl` (Selected Design, Settings changed).

## Test Boundaries

### Mock Boundary Decisions

| Component/Dependency | Mock? | Rationale |
|---------------------|-------|-----------|
| Slack | Yes | `slack_stub` (`crates/cli/tests/support`) runs the real `Slack` adapter over a fake transport: it keeps posted messages with metadata, answers `conversations.history` with them, can refuse a method, and gains a mode that stores a post and then fails the transport (a lost answer). `RecordingChannel` stays for tests that only count notices |
| GitHub / tracker HTTP | Yes | Existing stubs produce transient, refused and successful answers |
| Git origin capability | Yes | Make the endpoint unreachable and then reachable through the existing flow fixture. The [Git identity design](github-git-identity-design.md#test-boundaries-and-verification-strategy) verifies the actual HTTPS transport separately. |
| Filter command | No | Real `sh -c` scripts, as intake_int does |
| Store, events.jsonl | No | Real files in a temporary state directory |

### Data Layer Testing Strategy

- **Schema dependencies**: `events.jsonl` (free-form, `records.rs`; error lines `errors.rs`), `launches.jsonl` (`LaunchStart`), `services-unanswered.json` (import only, `records.rs` `Failing`), `config.toml` (`Config`)
- **Test data approach**: seeded state directories (dashboard_int pattern), and daemon ticks that write real records
- **Mock limitations acknowledged**: Slack rendering and metadata are proven at `slack.rs`'s payload level against the stub; Slack's own retention of metadata is the repository's existing assumption for decisions

### Integration Verification Points

- `crates/cli/tests/resilience_int.rs`:
  - GitHub fails on several passes → exactly one `ErrorStarted` notice; it answers → exactly one `ErrorCleared`;
  - a restart between failures posts no second start;
  - Slack refuses, then accepts → M-13 then M-14, in order;
  - Slack stores the M-13 and the answer is lost, then the daemon restarts → the stub holds exactly one M-13 message, and an `error-notice told` line exists;
  - an `events.jsonl` that ends with a start line and no `told` line, while the stub already holds its marked message → no post, one `told` line;
  - with notices off, no M-13/M-14 is posted and the lines are still in `events.jsonl`;
  - the authenticated Git endpoint is unreachable while `viewer` answers → no clear and no second start; the same endpoint answers → one M-14;
  - a CLI launch error → one M-08 naming its cause, `told` for that start only; the halt file kept and M-08 refused, restart, Slack accepts → M-08 posted once; a later good launch → M-14;
  - an `events.jsonl` ending with a CLI start line and no `paused` line, and no halt file (interrupted before the pause), then the daemon starts → the halt file exists before any launch, one M-08 naming the recorded cause;
  - a launch of one CLI failing while another of that CLI, begun before the failure, runs and then ends well → no `error-cleared` while paused; after `!resume` a launch begun after the failure → exactly one M-08, one `error-cleared`;
  - with notices off, a refused M-08 waiting before an ordinary notice → that notice dropped in the same pass, and only the M-08 posted once Slack accepts;
  - a CLI program that does not exist, for an issue's item → no error line; the item is launched again on the next pass; a second failed pass stops that issue and files `exception`, and the factory is not paused; for an item no issue or sprint owns → the second failed pass pauses with one M-08 from `announce_pause`, as today;
  - upgrade: a seeded `services-unanswered.json` with GitHub failing since T → `GET /overview` `status.unreachable` shows GitHub since T with its reason before any poll; a restart before the delete writes no second start; the authenticated Git endpoint is unreachable while `viewer` answers → no clear; the endpoint answers too → one M-14.
- `crates/cli/tests/intake_int.rs`:
  - a failing filter on two issues → one start and one repeat;
  - A admitted, B fails, the next intake passes reapply A's cached verdict → the error stays open;
  - a fresh admitted judgment → one clear naming that issue;
  - the filter removed while the error is open → one clear with `removed`; the same with the command already absent at daemon start;
  - a planted held credential in stderr, placed so the 300-byte cut falls inside it → absent from `events.jsonl`, the `GET /events` response and the Slack payload (no substring of 8 or more of its characters);
  - the same credential printed by a failing command while the sprint is Proposing, then replaced in the credentials file before the proposal ends → the `filter-failed` line written after the proposal does not contain it.
- `crates/cli/tests/dashboard_int.rs`: seeded lines → `GET /events` (attribution by issue, sprint, item, and a `github-error` with a sprint id in `issue`; folding; Error repeat; role shapes; told states; filters; window, `from` anchoring and `older`; `recorded` with a nonempty log and no match; masking; unknown kind; `unreadable`).
- `crates/store` unit test: a file ending in a partial line with no newline, then `append_line` → the appended line parses with `records::lines`, and the partial line stays one skipped line (I002). A `records` unit test does the same with the partial line cut inside a multibyte character: every other line is returned (I002).
- `crates/server/src/settings.rs` unit tests:
  - the `notices` section saves without a connection check;
  - `Config` without `[notices]` parses with `errors = true`.
- `crates/daemon` unit test of the `changes` diff: `notices.errors` false and `limits.issues_at_once` 3 carry their values; a changed `tracker.filter` holding a planted credential carries no `value`, and the serialized line does not contain the credential.
- `crates/integrations/tests/slack_contract.rs`: a marked notice reads history first, with `oldest` an hour before the line's time, and posts with `ovenspring_notice` metadata; a found marker posts nothing; history answered as a short first page with a `next_cursor` and the marked message on the second page → the second page is read and nothing is posted (I001).
- Real writer-to-API coverage, using the temporary repository, daemon, Slack stub and the real server from the existing CLI integration harness:
  - Extend `issue_flow_int.rs` with the merge through the checked-head gate. Read the resulting `merged` line through `GET /api/v1/events?issue=<KEY>`, and compare `pr`, `base`, both existing SHAs and the subject to independent expected values. Use a non-default fixture base. A refused or deferred merge produces no success line.
  - Extend the review correction flow in `issue_flow_int.rs`. While a required check remains outstanding there is no round-end line. Complete a round with three findings across more than one review (including an ID reused across two reviews) and assert `round: 1, findings: 3` at the endpoint. When its recheck completes cleanly, assert `round: 2, findings: 0` and one summary for each completion. Check that the existing facts still appear. Exercise the maintained-applied count in the existing handoff test when that path is not covered by the flow.
  - Extend `owner_channel_int.rs::unsent_decision_is_delivered_after_restart_and_resolved_by_one_answer`. With a known three-issue plan, deny Slack's post and assert no posted decision event for that ID. Restart with Slack accepting. Assert one posted decision line in the real API with its ID, kind, sprint attribution, count and purpose, still one after another pass, and the existing answer resolves it. A planted credential in the purpose is masked in the saved summary and served line. Where the existing whole-sprint path already posts on the first try, assert the same payload and subject there.
  - Keep the seeded endpoint fixture for read-side cases. Keep legacy wording cases in the vocabulary tests (merge lacking details, generic fact, decision lacking confirmed post), and use literal independent expectations for the new field shapes.
- Dashboard:
  - `EventsView` stories (UI Spec golden states 9–15, axe in both themes);
  - `dashboard/e2e/events.fixture.e2e.test.ts`: open; filters; a poll that brings a new line keeps the first 15 shown; "Show older events" keeps the shown lines while the older answer is delayed; "No event matches this filter."; the link to S-02; the sprint link lands at `#sprint-<id>` for an ended sprint whose sprints answer is delayed, and for the current sprint opened directly while the overview answer is delayed until after the sprints answer;
  - `dashboard/e2e/header.fixture.e2e.test.ts`: on `/events`, the Events link has `aria-current="page"`;
  - `dashboard/e2e/events.fixture.e2e.test.ts` block 7 completes the visual comparison below;
  - the settings fixture e2e for the Error notices row;
  - vitest for the vocabulary's fallback, the settings-change sentences and `time.ts` day headings.

## Verification Strategy

### Correctness Proof Method

- **Correctness definition**:
  - For each error occurrence, Slack holds at most one start and one clear message (none while off; M-08 instead of the start for `cli-*`), whatever restarts, refusals or lost answers happen in between, and each held one is recorded as told.
  - Each start, clear and delivery is recorded exactly once in `events.jsonl`.
  - The Events response lists every parsed line except `error-notice` lines and folded repeats, with the attribution and told state above.
  - The Events page and the Error notices row match the prototype's words and layout.
- **Verification method**: daemon integration tests count messages in the Slack stub, and `error-notice` lines, over scripted failure/success/restart sequences. Server integration tests compare `GET /events` with literal expectations after the real merge, review and decision writers run, and use a seeded file for read-side cases. Block 7 compares the production page and the prototype with matching fixture data.
- **Verification timing**: slice 1 proves delivery before the page is built; each later slice adds proof at its observable boundary. In slice 2, pass writer-to-API verification before the dashboard comparison. A missing production field or premature Slack success returns to its writer before comparison proceeds.

### Early Verification Point

- **First verification target**: slice 1's resilience test on the real Slack adapter. GitHub is unreachable for three passes. Slack stores the M-13 but the answer is lost. The daemon restarts, GitHub fails once more, then it answers.
- **Success criteria**: the stub holds exactly one M-13 and one M-14 message, each with its marker. `events.jsonl` has one start line, one `error-cleared` and one `told` line per phase. Now's `unreachable` showed the first failure's `since` throughout.
- **Failure response**: if a marked message cannot be found after a lost answer, revisit the delivery identity in this document before building the page on it. Example: history does not return the metadata the stub models.

### Visual comparison (binding prototype)

- **Reference**: `docs/ui-spec/assets/events/prototype.html`, opened in the same Playwright browser from its repository path, its situation select set to `open`, `clear`, `off`, `new`, `unreadable`, `lost` and its theme select to the golden state's theme.
- **Subject**: the production bundle under the fixture API (`playwright.config.ts`). The fixture answers carry the prototype's own data for each situation, transcribed once into the existing `dashboard/src/fixtures/events.ts` with the fields the real writers emit. The browser clock is fixed to the prototype's day (2026-11-27), so "Today" and clocks match.
- **Viewports**: 1280×800 for golden states 9–15; 600×800 for state 9, below the 620px breakpoint. The other Events states (All shown, Filtered to nothing and Not read yet) use their UI Spec behavior checks.
- **Observations**, per situation:
  - the visible text of the page content, from the lost-contact line when the situation shows one (the prototype puts it above the title, `lostNote()`) through the end of the list, equals the prototype's, whitespace normalized, without the prototype's "Why this is here" notes, after the UI Spec's stated overrides of the prototype's words are applied to the prototype's text. The overrides are a literal list in `events.fixture.e2e.test.ts` block 7, each pair naming its UI Spec passage, and the unreadable Issue options follow the separate EventFilters rule below (finding I011):
    - Vocabulary › Errors, <interval>: GitHub's "The factory tries again every 30 seconds." → "The factory tries again." in the start line and in Errors now's what-to-do line;
    - Vocabulary › Errors, tracker refuses: "No new issue is read and no sprint is proposed until it is fixed." → "No new issue or change is read from it until it is fixed.";
  - in Unreadable, compare the whole page content and layout. The reference Issue select exposes only “All issues” for the comparison, per UI Spec EventFilters. Separately compare the labels, selected values and option text of both controls in every situation. Keep the remaining unreadable content and layout in the comparison.
  - Include the list sentences, subject text, day headings, told lines, and the pagination or end text wherever they are present. Run the text comparison at 600px as well.
  - for each EventLine, the left edge and width of the clock, subject and sentence boxes equal the prototype's within 1px; at 600px, the subject's box is above the sentence's;
  - the filter gaps (10px row gap when wrapped and 16px column gap); compare the position and dimensions of the filter and empty-state layouts in states with no EventLine boxes;
  - for the open Error notices row, its label, what-line, legend, both option texts and buttons, and their boxes.
- **Pass**: every observation equal; any difference names the element and both values. An override pair whose prototype text is not found fails the test, so the list cannot go stale silently.

### Output Comparison

- **Comparison input**: the same seeded state directory before and after the change
- **Expected output fields**:
  - `GET /api/v1/overview` `status.unreachable` (`service`, `since`, `why`) while GitHub fails;
  - the `download` output for an issue and a sprint scope, including preserved merge SHAs, issue facts and decision identifiers; the new decision event time comes from delivery.
- **Diff method**:
  - The existing overview test keeps its expected JSON. Its seed of `services-unanswered.json` is kept as the upgrade case: the daemon starts on it (import), and the response is compared with the same expected JSON. A second case seeds the new start lines directly.
  - The download tests pass unchanged on their seeded records. A new case seeds a `filter-failed` line with `command`, `output` and `failure` for one issue: the issue's export holds that whole record, masked, in its time order.
- **Transformation pipeline coverage**:
  - intake `filter-dropped` (masked reason) stays byte-equal, and the existing intake_int credential tests pass;
  - `download`'s selection, merge, sort and mask are unchanged (it is not shared with `events.rs` beyond `mask`).

## Design Boundaries

- **Intentional limitations**:
  - Errors are the five keys above; other failures stay plain events (Selected Design, Errors).
  - An error does not repeat or clear while the factory is paused, because the polls do not run (`tick`).
  - Disconnecting the tracker or GitHub does not clear their error; it clears at their next successful read.
  - Old event lines are not rewritten.
  - A notice whose acceptance was not yet recorded when the owner changes the Slack channel is posted again in the new channel, and the old channel keeps its copy. The owner decided this: "A. A duplicate in the old channel is fine. This case is rare, so add no mechanism for it." (`sources/answers/GH-9-spec-16.md`, the factory's state directory; finding I001). No per-attempt channel record or deletion of the old copy is built.
- **Existing extension points used**:
  - `Channel::post_notice` (current consumers: `announce_pause`, ready-to-merge, preparation, reminder, waiting-for-issues);
  - Slack message metadata and history reads (current: `post_decision`, `decision_of`, `mark_resolved`, `catch_up`);
  - `SettingsSection` (current: 9 sections);
  - `records::lines` (current: overview, download, `last_activity`);
  - `download::mask` (current: `get_export`, intake).

## Risks and Mitigation

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| `events.jsonl` grows without rotation (724 lines / 105 KB in ~2 days), so a whole read per poll, and at daemon start, slows | Medium | Medium | The response carries only the shown window, and Now already reads the file per poll. Measure on a large seeded file in dashboard_int. Add a tail read only if it is measured slow. |
| The PR base changes outside the factory between the existing final pull read and successful merge, or the daemon is interrupted between the Pending decision save and the decision event append | Low | Low | Record the observed base and retain existing merge and delivery failure handling. These event details add no external writes or replay guarantees. |
| A CLI's M-08 still waits when the owner resumes | Low | Low | Delivery runs before `take_channel_events` in each pass, and `!resume` arrives only over a working Slack. |

## References

- `docs/prd/oven-spring-prd.md` AC-047, AC-048, AC-050, AC-072, AC-076–AC-078
- `docs/ui-spec/oven-spring-ui-spec.md` S-08, Errors, Slack Messages M-08, M-13, M-14, Delivery, Visual Acceptance
- `docs/design/issue-filter-design.md` (filter events), `docs/design/owner-interface-design.md` (dashboard and Slack builders), `docs/design/distribution-design.md` (Configuration)
- `sources/answers/GH-9-prd-10.md`, `GH-9-spec-13.md`, `GH-9-prototype-14.md`, `GH-9-prototype-15.md`
- Slack `chat.postMessage` (`metadata`), `conversations.history` (`include_all_metadata`): https://docs.slack.dev/reference/methods/chat.postMessage/, https://docs.slack.dev/reference/methods/conversations.history/
