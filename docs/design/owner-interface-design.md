# Owner Interface Design Document

## Overview

The owner reaches the factory through Slack, where the owner acts (every decision that is theirs arrives and is answered there, the lead talks with them, and they pause and resume the factory), and through a local dashboard, where the owner looks and configures (setup, the goal, the factory's state, settings and logs). Each function lives in one of the two. This document designs the channel abstraction and its Slack implementation, how decisions and approvals are recorded, the dashboard's HTTP API and frontend, and the download of records.

### Referenced UI Spec

- UI Spec path: `docs/ui-spec/oven-spring-ui-spec.md`
- Component structure, states, Slack message shapes and tokens are inherited from the UI Spec.

## Background and Context

### Prerequisite ADRs

- `docs/adr/ADR-0002-embedded-web-dashboard.md`: React SPA embedded in the binary, served on 127.0.0.1.
- `docs/adr/ADR-0004-slack-socket-mode-channel.md`: Socket Mode behind the `Channel` trait.
- `docs/adr/ADR-0005-file-board-and-state-outside-repository.md`: decisions, approvals and logs are files in the state directory.
- `docs/adr/ADR-0007-daemon-owns-external-writes.md`: only the daemon posts to Slack.

### External Resources Used

| Resource (project-tier label) | Feature-specific identifier | Notes |
|-------------------------------|-----------------------------|-------|
| Slack | Socket Mode (`apps.connections.open`, app-level token with `connections:write`); Web API `auth.test`, `chat.postMessage`, `chat.update`, `chat.postEphemeral`, `chat.getPermalink`, `conversations.history`, `conversations.replies`, `files.getUploadURLExternal`, `files.completeUploadExternal`, `users.info` | Bot scopes: `chat:write`, `channels:history`, `groups:history`, `app_mentions:read`, `files:write`, `users:read` |
| Design origin | `docs/ui-spec/assets/dashboard/index.html` | Binding for structure, layout, spacing and vocabulary (UI Spec, Prototype Management) |
| Fonts | Hanken Grotesk, IBM Plex Mono (SIL OFL) | Bundled; no network request |

### Requirement Convergence

- **Outcome**: N/A — covered by PRD `docs/prd/oven-spring-prd.md`
- **Non-Goals**: N/A — covered by PRD `docs/prd/oven-spring-prd.md`
- **Open questions**: None

### Standards and Assumptions

#### Applicable Standards

- [x] WCAG 2.2 AA `[explicit]` - Source: PRD Accessibility
- [x] Design tokens in DTCG format 2025.10 `[explicit]` - Source: UI Spec Design Tokens; https://www.designtokens.org/tr/2025.10/
- [x] TypeScript strict mode; Biome for lint and format; knip for unused code; pnpm with a seven-day release quarantine `[explicit]` - Source: the owner

#### Assumed Behaviors

- [x] Socket Mode delivers `block_actions` for message buttons and requires an acknowledgement within 3 seconds - Evidence: https://docs.slack.dev/apis/events-api/using-socket-mode/; runs with real services - Design effect: buttons are the only way to approve - Confirmed: Yes
- [x] Slack refreshes the WebSocket URL and drops connections - Evidence: runs with real services - Design effect: the channel module, written directly on a WebSocket client, reopens with back-off up to 60 s and reads the history missed while disconnected - Confirmed: Yes

#### Quality Assurance Mechanisms

- [x] `tsc` (application and Node configurations), `biome check --error-on-warnings`, `knip` — Enforces: types, lint, format, unused files, exports and dependencies — Config: `dashboard/tsconfig*.json`, `biome.json`, `knip.json` — Covers: `dashboard/**` — Status: `adopted`
- [x] Vitest — Enforces: unit tests of logic (token build) and the story projects below — Covers: `dashboard/**` — Status: `adopted`
- [x] Storybook stories run by Vitest (`@storybook/addon-vitest`) in Chromium with axe (`@storybook/addon-a11y`, `test: 'error'`), one project per theme — Enforces: every story renders, its play function passes, no WCAG 2.2 AA violation in light and in dark — Covers: `dashboard/src/**/*.stories.tsx` — Status: `adopted`
- [x] Token build check — Enforces: generated CSS variables match `tokens/tokens.json`; contrast pairs in the UI Spec meet their targets — Covers: `dashboard/tokens/**` — Status: `adopted`

### Problem to Solve

Give the owner one place to act (Slack) and one place to look (the dashboard), with the goal always in view, and make every approval attributable.

### Current Challenges

None open. The split between Slack and the dashboard follows the owner's journey: Slack is where the owner acts, the dashboard where the owner looks and configures.

### Requirements

#### Functional Requirements

PRD R6, R9, AC-002, AC-007 to AC-012, AC-053, AC-055 to AC-060.

#### Non-Functional Requirements

- **Freshness**: dashboard data no older than 5 seconds - Source: PRD Performance (one poll interval of the daemon)
- **Acknowledgement**: Slack interactions acknowledged within 3 seconds - Source: Slack Socket Mode
- **Accessibility**: WCAG 2.2 AA in both themes - Source: AC-048

## Acceptance Criteria (AC) - EARS Format

### Channel

- [x] **When** the core requests an owner decision, the system shall post its message (UI Spec M-01 to M-07) and record a pending decision with the message's link (AC-029)
- [x] **While** any delivered decision or ready-to-merge pull request waits on the owner, the system shall post one reminder (M-12) listing only items waiting at least an hour since delivery or readiness, with their links, at most once an hour; the working agreements' pull request is not listed, since the review thread links it (AC-068)
- [x] **When** an owner presses Approve, the system shall record the approval with the Slack user, the time, the message link and, for a merge, the commit sha shown, and update the message to show who approved (AC-032)
- [x] **If** someone who is not an owner presses a button, **then** the system shall record nothing and tell that person privately why (AC-032)
- [x] **When** an owner replies in a decision's thread, the system shall pass the reply to the waiting step as the answer; a merge request is answered only with its buttons (AC-029, AC-017)
- [x] **When** an owner mentions the bot outside a decision thread with anything but a command, the system shall start or continue a `conversation` with the lead in that thread (AC-033)
- [x] **When** an owner mentions the bot with `!pause [reason]`, `!resume` or `!dashboard` right after the mention, the system shall pause, resume or give the dashboard's address, and confirm in the command's thread; any other command gets no reply (AC-034)
- [x] **When** the factory pauses itself, the system shall post why and how to resume (UI Spec M-08) (AC-034, AC-037)
- [x] **When** the core requests a recovery, the system shall post M-09 with the procedure's text as copied to the isolated directory; **when** an owner presses Approve, it shall record the approval and hand the core the decision id, from which the core runs that copy; **when** an owner presses Resolved, it shall record the answer and hand the core the same id (AC-064)
- [x] **When** an owner presses Cancel issue on a question, a recovery or a stop about an issue, the system shall record the answer, end the issue as a cancel from the dashboard does, and mark the message canceled with who did it (AC-067)
- [x] A message that does not mention the bot shall never be taken as a command (AC-034)
- [x] Messages and presses from users not in `slack.owners` shall be ignored, apart from the private notice for a press (AC-012)
- [x] **When** Preview is pressed on a merge request, the system shall start the issue's app and reply in the thread with its local address (AC-031); a merge request offers Preview only while `[app]` has commands, not when it records nothing to run (AC-066)
- [x] **When** a prototype is ready, its approval message shall link to the issue's prototype page in the dashboard and state its reference strength and why; the page shows the lead's HTML in a sandbox without the dashboard's origin (AC-031)
- [x] The core shall depend only on the `Channel` trait (AC-035)

### Dashboard

- [x] **When** a request names a host other than this machine, or a write carries a body other than JSON, the system shall refuse it and serve no data (AC-055)
- [x] **While** setup is not finished, the dashboard shall open on its first step not done: Connect while there is no configuration, Run the product while `[app]` is not settled, Set the goal while there is no goal (AC-002, AC-066)
- [x] **When** the owner continues from Connect, the system shall check GitHub, the tracker and Slack, then write `.ovenspring/config.toml` with every role on the agent the owner chose and store the credentials outside the repository (AC-002, AC-013, AC-065)
- [x] **When** the owner keeps or changes the product's commands, the system shall start the product once with them and write `[app]` only when it answered; **when** the owner records that there is nothing to run, the system shall write that (AC-066)
- [x] **While** a prerequisite is missing (the GitHub, tracker or Slack connection), the dashboard shall name it and how to fix it, and the factory shall start no work that needs it (AC-060)
- [x] The goal band shall show the current goal on every screen, and **when** the owner saves a new goal on the goal page, the system shall store it and record the change (AC-056)
- [x] The overview shall return the factory's one state in the UI Spec's FactoryStatus order, the sprint with its done count, every pending decision with its kind, and each issue of the sprint with its section (waiting for you, in progress, waiting to start, done) and, by section, its phase, activity and when it entered it, its wait reason, or its outcome and when (AC-043)
- [x] The issue detail shall return the issue's size with the evidence it was judged from and its path: each phase's state (done, now, ahead, skipped), its span and its runs (AC-044)
- [x] **While** a run is under way or after it ended, the system shall return the agent's messages read from its output, in order (AC-045)
- [x] **When** the owner saves settings, the system shall write `.ovenspring/config.toml` and the next launch shall use them (AC-007 to AC-012, AC-052, AC-058)
- [x] **When** the owner requests a download, the system shall produce the records of the chosen scope in one form, with secrets masked (AC-046, AC-047)

## Existing Codebase Analysis

### Implementation Path Mapping

| Type | Path | Description |
|------|------|-------------|
| Existing | `crates/integrations/src/channel/` | `Channel` trait, Slack implementation, message builders |
| Existing | `crates/daemon/src/owner.rs`, `overview.rs`, `history.rs` | Decisions, approvals, reminders, commands; the overview and the issue detail the API returns |
| Existing | `crates/server/` | HTTP server, API handlers, request check, download |
| Existing | `dashboard/` | React app, tokens, Storybook |

### Integration Points

- **Integration Target**: factory core; **Invocation Method**: `Channel` trait calls from the daemon's outward-action handlers; the daemon reads channel events and applies the matching core facts.
- **Integration Target**: browser; **Invocation Method**: HTTP on 127.0.0.1.

### Code Inspection Evidence

| File/Function | Relevance |
|---------------|-----------|
| `docs/ui-spec/assets/dashboard/index.html` | Binding prototype |
| `crates/daemon/src/overview.rs`, `history.rs` | The API returns codes; the dashboard words them (`dashboard/src/lib/vocabulary.ts`) |
| `crates/integrations/src/channel/slack.rs` | Socket Mode, commands through a mention, message builders |

### Fact Disposition Table

| Fact | Disposition |
|------|-------------|
| The goal and the factory's status live in the dashboard, not in Slack commands | Keep |
| A re-posted decision carries the same id and resolves once | Keep |
| The API returns phase, activity, wait reason and outcome as codes; the dashboard owns the words (UI Spec, Vocabulary) | Keep |

## Design

### Change Impact Map

```yaml
Change Target: owner interface
Direct Impact:
  - crates/integrations/channel, crates/server, dashboard/
Indirect Impact:
  - core facts the daemon applies from the owner's answers: OwnerAnswered, OwnerReplied, OwnerHeld, Canceled, and the sprint's Approved and Replied
No Ripple Effect:
  - step flow and gate rules (they consume the facts above)
```

### Interface Change Matrix

N/A — no existing interfaces.

### Architecture Overview

The channel and the server are inside the daemon. The channel turns Slack events into channel events, which the daemon applies as core facts, and the daemon's outward actions into Slack messages. The server reads the state directory on each request; it writes the setup, the goal, the settings and credentials, and the owner's cancel requests (isolated directory).

### Data Flow

```
core action AskOwner | RequestApproval ─▶ decisions/<id>.json (unsent) ─▶ Channel::post_decision (id in metadata and buttons) ─▶ decisions/<id>.json (pending, permalink)
Slack block_action ─▶ ack (<3 s) ─▶ owner? ─▶ approvals/<id>.json ─▶ fact OwnerAnswered | SprintFact::Approved ─▶ core; events.jsonl gains decision-answered
Slack thread reply ─▶ owner? ─▶ decision thread? ─▶ fact OwnerReplied | SprintFact::Replied | lead conversation
Slack app_mention ─▶ `!command` right after the mention? ─▶ command | (decision thread? reply : lead conversation)
Slack block_action Cancel issue ─▶ owner? ─▶ decision answered `cancel` ─▶ fact Canceled
Slack block_action Preview ─▶ owner? ─▶ the issue's app started from its worktree (off the loop) ─▶ reply in the pressed message's thread with its address
Slack block_action Hold (merge request) ─▶ owner? ─▶ decision answered `hold` ─▶ issue held for the sprint review
ConversationMessage ─▶ conversations/<thread ts>.json ─▶ one `conversation` item for the lead at a time ─▶ its `reply` in the thread; messages that arrived meanwhile file the next turn
browser ─▶ /api/v1/* (Host check) ─▶ state directory ─▶ JSON ─▶ React views (poll 5 s)
```

### Integration Points List

| Integration Point | Location | Old Implementation | New Implementation | Switching Method | Verification Method |
|-------------------|----------|-------------------|-------------------|------------------|-------------------|
| Owner channel | `channel::Channel` | — | `SlackChannel` | constructed from configuration | Fake channel in core tests; the end-to-end check with real services |
| Dashboard API | `server::api` | — | axum handlers | — | API tests against a temporary store |

### Main Components

#### Channel (`crates/integrations/src/channel`)

- **Responsibility**: the factory's conversation with the owner, in the factory's terms.
- **Interface**:
  ```rust
  trait Channel: Send + Sync {
      fn post_decision(&self, d: &DecisionRequest) -> Result<MessageRef, ChannelError>;
      fn post_notice(&self, n: &Notice) -> Result<MessageRef, ChannelError>;
      fn reply(&self, thread: &MessageRef, text: &str) -> Result<MessageRef, ChannelError>;
      fn attach(&self, thread: &MessageRef, name: &str, content: &str) -> Result<(), ChannelError>;
      fn tell(&self, user: &str, text: &str) -> Result<(), ChannelError>; // privately, to one user
      fn mark_resolved(&self, m: &MessageRef, resolution: &str) -> Result<(), ChannelError>;
      fn events(&self) -> Vec<ChannelEvent>; // Action, Preview, Said, Reply, Command
      fn connected(&self) -> bool;           // defaults: true, None, None
      fn trouble(&self) -> Option<String>;
      fn mention(&self) -> Option<String>;   // the bot's name as Slack shows it
  }
  ```
- **Receiving**: Socket Mode delivers a message that mentions the bot twice, as `app_mention` and as `message`. Every message that mentions the bot is read from `app_mention` only; `message` events are read only for thread replies that do not mention it. A command is the bot's mention at the start of the text followed by `!name`; the bot's user id comes from `auth.test` with the bot token each time the Socket Mode connection opens, so no event is read before it is known. A command in a thread is answered in that thread (its root, as Slack asks).
- **Dependencies**: Socket Mode written on a WebSocket client (`tungstenite`) and the Web API over `ureq`; the module reconnects itself with back-off and reads the history missed while disconnected; credentials.

#### Decisions and approvals

- **Responsibility**: the record of what the owner was asked and what they answered.
- `<isolated>/decisions/<id>.json`: `{id, kind: sprint|spec|design|prototype|prd|merge|review|recovery|stopped, issue?, sprint? (for a decision with no issue), sha?, categories, title, text, status: unsent|pending|answered|superseded, message?: {channel, ts, permalink}, answer?: {kind: approve|hold|resolved|reply|cancel, text?, user, at}, procedure?, asked_by?}`. `procedure` names the copied file under `<isolated>/recoveries/` for a `recovery`; a `stopped` decision is M-09 without a procedure, for a stop the lead could not be launched to take, and offers Resolved. A question, a recovery or a stop about an issue also offers Cancel issue, which ends the issue (AC-067). Decisions and approvals live in the isolated directory, which no launch is given (factory core design, Store).
- A decision is written `unsent`, with its content, before anything is posted. Each Slack message carries the decision id in its message metadata and in every button's value. Posting succeeds → `pending` with the message reference. Posting fails → it stays `unsent` and is retried on every loop until delivered. If a post reached Slack but its reference was not saved, the decision is posted again; both messages carry the same id, and an action or reply resolves a decision by that id, so the answer counts once.
- `<isolated>/approvals/<id>.json`: `{decision, kind, issue?, sha?, categories, user, at, permalink}`; append-only; `events.jsonl` gains `decision-answered` with the decision id.
- A decision is `superseded` when its basis changes (a new push for a merge request); its message is updated to say so and a new request is posted.

#### Slack message builders

Each UI Spec message is one builder producing Block Kit: a section with the bold title, a `markdown` block with the conclusion and points, an `actions` block with buttons where the UI Spec gives actions, a context line saying how to answer, and the details, from the first heading on, as a reply in the thread, also in a `markdown` block. Slack takes at most 12,000 characters of `markdown` text in one message, so text longer than that continues in further replies in the thread. A PRD (M-11) or design (M-03) approval has the document it asks about attached in its thread (`files.getUploadURLExternal`, the content sent to the address it gives, `files.completeUploadExternal` with the thread), read from the issue's worktree when the request is sent, so the owner reads what they approve; a failed upload is logged and the request stands. Content written by the lead comes from its validated output (`message` fields); the factory's own notices are fixed English templates. Approve, Hold, Resolved and Cancel issue carry the decision id as plain text, which names one sha for a merge request and one copied procedure for a recovery (the sha is read from the decision, not the button); no button carries a command. Preview carries the issue key, since the ready-to-merge notice has no decision.

#### Dashboard server (`crates/server`)

- **Responsibility**: serve the embedded assets and the JSON API on `127.0.0.1`, on the port the repository's dashboard last used while it is free (otherwise any free port), recorded in `<isolated>/dashboard.json` (`{pid, url}`, kept after the daemon stops so the next start serves at the same address) so that `ovenspring start` can open it again. The daemon serves the dashboard before it has a configuration and starts the factory once a valid one exists. The header names the repository, since each repository has its own dashboard, and the version its daemon runs, with the newer one to move to (distribution design, Version and updates).
- **Request check**: every request must name `127.0.0.1` or `localhost` (any port, so an SSH port forward works) as its `Host`, which refuses a page reaching the port under another name (DNS rebinding); writes take only JSON bodies, which another site's page cannot send without a preflight the server never grants. No token: it would not stop an agent, which runs as the owner, and these two checks stop the browser threat.
- **Interface** (all under `/api/v1`, JSON):

| Method and path | Returns or does |
|-----------------|-----------------|
| `GET /overview` | `Overview`: `{goal, status: {state, reason, prerequisites[], unreachable: {service, since, why}, configError, reviewPart}, sprint: {id, purpose, startedAt, done, total}, decisions[{issue, ask, title, link, delivered}], issues[{key, title, now: Position}], tracker, openIssues, lastSprintEndedAt, toldWaiting: {at, link}, agreements, mention}`; `Position` is `{section, phase, activity, task: {number, of}, problem, feedback, onHold, since, wait: {reason: after, keys} \| {reason: place, heldBy}, ended, endedAt}`. `feedback` lists the review steps whose findings the lead is addressing while the issue is in progress at an `unsettled` exception, derived from the round state, and is `[]` otherwise (`docs/design/review-feedback-progress-design.md`, Data Contracts). `state` is the first that holds of `paused`, `prerequisites`, `unreachable`, `settings-not-applied`, `proposing`, `waiting-for-issues`, `proposal-with-owner`, `sprint-problem`, `sprint-with-owner`, `reviewing`, `review-with-owner`, `applying-agreements`, `working` (the dashboard adds lost contact itself). `section`, `phase`, `activity`, `ask`, `problem` (the exception's cause, `unsettled` included), `ended` (`merged`, `closed`, `dropped`, `canceled`, `held`) are codes the dashboard words. A `merge.by = owner` issue ready to merge is a decision `merge-on-github` linking its pull request, as is the working agreements' pull request while it waits to merge. `agreements`: that pull request's address while the sprint waits for it. `unreachable`: GitHub or the tracker failing to answer since the first failure, and why (`no-connection`, `server-error`, `rate-limited`), derived from the open `github-unreachable` and `tracker-unreachable` errors in `events.jsonl` (`docs/design/events-and-error-notices-design.md`, Error state; a `services-unanswered.json` left by an earlier version is imported once at daemon start). Slack failing to connect is a prerequisite, not `unreachable`. `configError`: `.ovenspring/config.toml` failing to load in the server while the daemon runs on the settings it last read. `mention`: the bot's name as Slack shows it, which the owner types after `@` (`mention.json`, written by the daemon when Slack says it; the app manifest's name before that) |
| `PUT /goal` | `{text}`; stores, logs, answers `{goal}` |
| `POST /issues/{key}/cancel` | `{words}`; for an issue that has not ended, leaves `<isolated>/cancels/<key>.json` for the daemon, which ends the issue on its next pass (AC-067), and answers 202; 409 for an unknown, merged or dropped issue (a held issue can be canceled). The isolated directory keeps agents from asking for a cancel |
| `GET /issues/{key}` | `IssueDetail`: `{key, title, tracker, pullRequest, prototype, now: Position (as in `GET /overview`, `feedback` included), decision, canceled, scale, scaleEvidence[], outcome, nonGoals[], path[{phase, state, startedAt, endedAt}], findings[{source: {kind: review, review, round} | {kind: comment, author}, text, disposition, answer}], launches, costUsd, uncosted}`; `canceled` is the owner's words when they canceled it. `scaleEvidence` is the `requirements` output's `costEvidence.drivers[].fact`; `path` takes `done`, `now`, `ahead` or `skipped` from the issue's phase, its scale (Small: Design skipped) and how it ended, and its spans from the launches' recorded phase; `uncosted` counts ended launches with no cost; 404 for an unknown key |
| `GET /sprints` | Ended sprints, newest first: `[{id, purpose, startedAt, endedAt, keyResults[{text, met, evidence}], agreements[], merged[], dropped[], held[], launches, costUsd, uncosted}]` |
| `GET /launches?sprint=&issue=` | `[{id, startedAt, endedAt, issue, role, step, phase, cli, result, costUsd, detail}]`, running first, then by when they ended, newest first; `detail` is what the step found or did (round, findings applied and declined, scale, tasks, documents); `phase` as recorded when the launch started; `result` one of `running`, `done`, `pass`, `findings`, `failed`, `retried` |
| `GET /launches/{id}` | The row with `{model, effort, failure, messages[]}`: the agent's messages read from its output stream in order, while it runs and after; `/launches/{id}/log` serves the raw output |
| `GET /setup` | `{step, repository, origin, error, slackManifest, run}`: `step` is `connect`, `run`, `goal` or `done`, from what is saved; `run` (while `step` is `run`) is `{said[], stopped, mention, found, checking, check}`: the lead's messages in the `run-commands` launch, the pause reason when its agent stopped, the bot's name to mention in Slack to resume, its answer `{runs: commands \| nothing \| unknown, setup, start, ready, reason}`, whether the factory is starting the product, and how that ended `{started: {port, seconds}} \| {failed: {output}}` |
| `POST /setup/check` | One block, by `section`: `github {token, baseBranch}`, `tracker {tracker, githubToken}`, `slack {botToken, appToken, channel, owner}`; answers `{found}` or 400 `{errors: {<section>: why}}`. Writes nothing |
| `PUT /setup` | `{cli, tracker, slack, repo, github}`: checks the three blocks again, writes the configuration (every role on `cli`, `[app]` unsettled) and the credentials, answers the setup state; 400 with nothing written, or 409 once configured |
| `PUT /setup/run` | `{runs: commands, setup, start, ready}` or `{runs: nothing}`: commands other than the lead's that already started are started once in the base branch's worktree first; `[app]` is written only when the product answered; 400 `{errors: {check: <what failed and the last output>}}` otherwise |
| `GET /settings` / `PUT /settings/{section}` | `{config, problem, alwaysDenied, held, mcp, repository, githubAccount}`: the configuration in the file's own shape, read as it is (`problem` says why the factory cannot use it, so a save can correct it; only a file that does not parse is refused), the commands no agent runs whatever the owner sets, which credentials are held, each by its last four characters so the owner can tell which one it is (values are write-only), and the MCP servers found with where each is defined and which factory services it reaches. A save takes one section (`general`, `agents`, `roles`, `tracker`, `slack`, `limits`, `app`, `guard`, `review`, `notices`): the four with credentials or a role as their own request types, `app` as `PUT /setup/run` takes it, the others in the file's shape; it is checked as the configuration is checked and, for a connection or the product's commands, as setup checks them, then the file is written whole. `notices` (`{errors: boolean}`) runs no connection check, so error notices can be turned off while Slack fails (`docs/design/events-and-error-notices-design.md`, Notices section) |
| `GET /issues/{key}/prototype` | The lead's prototype page as HTML, with `Content-Security-Policy: sandbox allow-scripts` and `nosniff`, so it runs in an opaque origin; 404 when the issue has none |
| `GET /skills` | `{claude, codex, opencode}`, each `{dirs, skills}`: the repository's own skills in the folders that CLI reads (`.claude/skills` for Claude Code, `.agents/skills` for Codex, both for OpenCode), with descriptions |
| `GET /export?scope=sprint:<id>|issue:<key>` | The records as one attachment |
| `GET /events?show=&issue=&from=` | `EventsPage`: the lines of `events.jsonl` for the Events page (S-08), newest first, with their subject, error role and told state, the errors going on and the issues the lines name, every string masked with the held credentials; contract in `docs/design/events-and-error-notices-design.md` (Data Contracts, GET /api/v1/events). A `show` other than `all`, `errors` or `filter`, or a `from` that is not a line number, is refused with 400 naming that parameter; an `events.jsonl` that exists and cannot be read answers 200 with `unreadable: true` and no lines |
| `GET /version` | `VersionState`: `{running, update: null \| {kind: restart \| available, version}}`: the version the daemon runs, and an installed version other than it (`restart`) or else a newer release (`available`); distribution design, Version and updates |

Every call a handler refuses or fails answers `{errors}`: why, by field or section, or by `request` for the request as a whole. A request naming another host gets a plain-text 403, a body or path the extractor rejects gets its plain-text 400, 415 or 422, and an unknown `/api` path gets 404. Field names are camelCase, except the configuration, which keeps the file's names. The dashboard's TypeScript types for every body above are generated from these Rust types (`crates/server/src/types.rs`, a dev-time test that fails while `dashboard/src/api/types.gen.ts` is out of date); the dashboard does not check the shape again.

#### Dashboard app (`dashboard/`)

- **Structure**: `src/app/` (routes: `/setup`, `/`, `/issues/:key`, `/issues/:key/prototype`, `/activity`, `/events`, `/sprints`, `/goal`, `/settings/:page`; the pages are `connections`, `agents`, `merging`, `safety` and `product`, each saving one or more API sections; `/settings` opens `connections` and an unknown path opens `/`), `src/components/` (one directory per UI Spec component, each with its stories), `src/api/` (typed client for the endpoints above, its types generated from the Rust response types), `src/lib/vocabulary.ts` (the UI Spec's words for every code the API returns), `tokens/tokens.json` (DTCG), `tokens/build.ts` (generates `tokens.css` with light and dark sets).
- **Effort choices**: Claude Code's levels (`low`, `medium`, `high`, `xhigh`, `max`) and the levels every Codex model offers (the same five) are constants of the dashboard; OpenCode's is free text. The API keeps `effort` a string.
- **Data**: each view polls its endpoint every 5 seconds while visible; the status headline region is `aria-live="polite"`.
- **Theme**: `light | dark | auto` stored in `localStorage`; `auto` follows `prefers-color-scheme`.
- **Stories**: one per golden state and per main component state in the UI Spec matrices; the Storybook test runner runs axe on each in both themes.

### Data Representation Decision

| Criterion | Assessment | Reason |
|-----------|-----------|--------|
| Semantic Fit | Yes | A decision is the owner's question; an approval is their answer with identity |
| Responsibility Fit | Yes | The channel records, the core consumes |
| Lifecycle Fit | No (differs) | Approvals are permanent audit records; decisions end when answered or superseded |
| Boundary/Interop Cost | Low | Files in the state directory |

**Decision**: new — two records, `decisions` and `approvals`, because their lifecycles differ.

### Selected Design

- **Addition**: `Channel` trait — **Current evidence**: AC-035 — **Lower-surface insufficiency**: calling Slack from the core would tie the core to Slack — **Subtraction result**: AC-035 unmet.
- **Addition**: `decisions` and `approvals` records — **Current evidence**: AC-030, AC-032, AC-046 audit — **Lower-surface insufficiency**: the event log alone cannot answer "what is pending" without replaying — **Subtraction result**: AC-032 attribution and AC-030 per-decision waiting unmet.
- **Addition**: the `Host` check — **Current evidence**: AC-055 — **Lower-surface insufficiency**: binding to 127.0.0.1 alone lets a page reached through DNS rebinding read and change settings — **Subtraction result**: AC-055 unmet.
- **Addition**: DTCG token file and generator — **Current evidence**: AC-048 (contrast in light and dark themes) — **Lower-surface insufficiency**: a hand-kept CSS file cannot be checked for contrast pairs — **Subtraction result**: contrast check of AC-048 unmet.

#### Download

One JSON Lines file per scope (a sprint or an issue), every record in time order: launches (start and end, with usage and cost), state changes, outward actions, decisions and their answers, approvals (user, time, link, sha), each finished item's result and output, commits and merges with their shas. JSON Lines keeps each record whole for an agent to read and in order for an audit; it is built from `events.jsonl`, `launches.jsonl`, decisions, approvals, archived items and outputs.

Masking runs on every downloaded byte: exact values of every configured credential, and patterns for common token formats (`xoxb-`, `xoxp-`, `xapp-`, `ghp_`, `github_pat_`, `gho_`, `lin_api_`, `sk-`, AWS access keys, PEM private key blocks) are replaced with `[masked]`.

### Data Contracts

#### Approval

```yaml
Contract: approvals/<id>.json
Input:
  Type: Slack block_action from Socket Mode
  Preconditions: action's user is in slack.owners; decision is pending; a superseded decision counts nothing, so a merge approval holds only for the sha its decision names
  Validation: decision id from the button value; user from the payload Slack signs over the socket
Output:
  Type: {decision, kind, issue?, sha?, categories, user, at, permalink}
  Guarantees: written once; an event ApprovalRecorded follows
  On Error: no record; ephemeral explanation to the presser
Invariants:
  - An approval never changes after it is written.
```

#### Goal

```yaml
Contract: PUT /api/v1/goal
Input:
  Type: {text}
  Validation: trimmed, non-empty
Output:
  Type: goal.md replaced; events.jsonl gains {event: goal-set, at, old, new}
  Guarantees: next sprint-plan and every later launch read the new text
```

### Field Propagation Map

| Field | Boundary | Status | Serialized Format | Consumer Parse Rule | Detail |
|-------|----------|--------|-------------------|---------------------|--------|
| decision id | core → Slack button → channel | preserved | button `value` JSON `{"d":"<id>","sha":"<40-hex>"}` | JSON; id must exist; sha exact | |
| theme | browser only | preserved | `localStorage["ovenspring-theme"]` = `light|dark|auto` | enum; default `auto` | |

### State Transitions and Invariants

```yaml
Decision: unsent → pending → answered (approve | hold | resolved | reply) | superseded
System Invariants:
  - At most one pending decision per issue and kind.
  - A merge approval is valid only for its sha.
  - A recovery approval runs only the procedure file its decision names, once.
```

### UI Error State Design

| Component / Screen | Loading | Empty | Error | Partial |
|-------------------|---------|-------|-------|---------|
| Now | Last data kept; first load shows the goal band and a "Loading" title | No sprint: the factory's state and "No sprint running."; empty sections as the UI Spec says | FactoryStatus "lost contact" with the time of the last data; the rest kept | — |
| Issue | Header only | No findings: "No findings so far." | As above | — |
| Setup | Values kept; a block being checked says so | First start: Connect with GitHub and GitHub Issues checked, the rest empty | A block's failed check under it with what the service answered; Continue lists what is missing; a failed start of the product shows its last output; the lead's failed launch shows the agent's error | — |
| Activity | Rows kept while refreshing | "Nothing is running for this filter." / "Nothing has happened yet." | As above; a run that cannot be read says so in its panel | — |
| Settings | Values kept | No skills: UI Spec state | Save failed under the field | — |

### Client State Design

| State Category | State | Management Method | Sync Strategy | Reset/Clear Behavior |
|---------------|-------|-------------------|---------------|----------------------|
| Server state | overview, issue, launches, settings, version | fetch hooks with a 5 s poll while visible | polling | — |
| Local UI state | expanded rows, download panel, edit forms | component state | — | reset on navigation |
| Temporary state | goal draft (goal page), settings drafts (one row at a time) | component state | saved on Save | discarded on Cancel or on leaving, with a notice |
| Preference | theme | `localStorage` | — | — |

### UI Action - API Contract Mapping

| UI Action | API Endpoint | Request | Response | Error Contract |
|-----------|-------------|---------|----------|----------------|
| Save goal | `PUT /goal` | `{text}` | goal | 400 empty → field message; 5xx → Save failed state |
| Check a setup block | `POST /setup/check` | the block's values | `{found}` | 400 → ✗ and the reason under the block |
| Continue from Connect | `PUT /setup` | connections and the agent | setup state | 400 with field or block errors |
| Continue from Run the product | `PUT /setup/run` | commands or nothing | setup state | 400 `check` → Did not start with its output |
| Start (Set the goal) | `PUT /goal` | `{text}` | goal | 400 empty → field message |
| Save settings | `PUT /settings/{section}` | section object | settings | 400 with field errors |
| Cancel this issue | `POST /issues/{key}/cancel` | `{words}` | 202 | 409 or 5xx → Canceling failed |
| Download | `GET /export` | query | file | 500 → Failed state |

### Error Handling

| Error Category | Example | Detection | Recovery Strategy | User Impact |
|---------------|---------|-----------|-------------------|-------------|
| External | Slack socket closed | Close frame or missed pings | Reconnect with backoff; read history since the last seen ts | Messages delayed |
| External | Post failed | Web API error | The decision stays `unsent` and is retried every loop; the overview lists it as "not in Slack yet" | Owner sees what is waiting even while Slack is unreachable |
| Validation | Approval from someone who is not an owner | User check | Ephemeral notice | Presser informed |

## Implementation Plan

### Implementation Approach

**Selected Approach**: Vertical slices from the owner's first need: setup and the goal band over the API, then the overview, then issue, logs, settings and the download.
**Selection Reason**: the owner can watch the walking skeleton of the core from the first slice.

### Technical Dependencies and Implementation Order

1. **Socket Mode buttons and reconnect**, confirmed in the end-to-end check with real services — can change the approval fallback.
2. **Tokens and Storybook** — every component depends on them.
3. **Server, request check, setup, goal and overview** — Prerequisites: core store.
4. **Channel with M-01, M-05 and approvals** — Prerequisites: core flow emitting decisions.
5. **Issue, logs, settings, download** — Prerequisites: 3.

### Migration Strategy

N/A — new product.

## Security Considerations

- **Authentication & Authorization**: the dashboard answers only requests naming this machine, and writes only JSON bodies (AC-055); Slack approvals and messages only from owners, identity from the socket payload (AC-012, AC-032); a recovery runs only the procedure its decision names, on an owner's Approve (AC-064).
- **Input Validation**: setup, goal and settings validated server-side; Slack text treated as data passed to steps, never as factory instructions.
- **Sensitive Data Handling**: credentials write-only in the API, named only by their last four characters; masking on every download.

## Test Boundaries

### Mock Boundary Decisions

| Component/Dependency | Mock? | Rationale |
|---------------------|-------|-----------|
| Slack | Yes (recorded Socket Mode envelopes and Web API responses) | The channel module is the boundary owned here; P3 covers the real service |
| Store | No (temporary directory) | Real files are cheap |
| Browser | Storybook and Vitest with DOM; one Playwright pass over the golden states | Visual acceptance from the UI Spec |

### Data Layer Testing Strategy

- **Schema dependencies**: decisions and approvals JSON shapes above.
- **Test data approach**: fixtures written into temporary state directories.
- **Mock limitations acknowledged**: Slack's real delivery and reconnect are proven only by P3.

### Integration Verification Points

- A merge decision: posted, a press by someone who is not an owner refused, an owner's press recorded with sha, superseded by a new push, re-requested.
- Restart while a post fails: the decision stays `unsent`, is delivered after restart, and the owner's one answer resolves it.
- Dashboard: first start opens setup; saving it writes only `.ovenspring/` in the repository; a request naming another host refused; goal saved and shown on every screen.
- Download: the records of a fixture sprint, with every credential the factory holds and every listed token format masked.

## Verification Strategy

### Correctness Proof Method

- **Correctness definition**: every owner action becomes exactly one recorded fact attributed to the right person; every screen state in the UI Spec renders from API data.
- **Verification method**: channel tests over recorded envelopes; API tests; stories per state with axe.
- **Verification timing**: with each slice.

### Early Verification Point

- **First verification target**: P3 on a test workspace: an approval button pressed by an owner and by another user, and a forced reconnect.
- **Success criteria**: the owner's press is recorded with the correct user within 3 seconds; the other is refused; messages sent during the disconnect are read after reconnect.
- **Failure response**: use thread-reply approvals (see Assumed Behaviors) and record the limitation in ADR-0004.

### Output Comparison

N/A — new behavior.

## Design Boundaries

- **Intentional limitations**: one Slack channel per factory; no dashboard access from other machines except through the owner's own forwarding; English text only.
- **Existing extension points used**: None.

## Risks and Mitigation

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| Approval spoofing by a local process through the dashboard | Medium | Low | Approvals exist only in the channel; the dashboard has no approval endpoint |
| Slack rate limits during a burst of messages | Low | Low | One message per decision; details as one file |

## References

- PRD, UI Spec, ADR-0002, ADR-0004, ADR-0005, ADR-0007
- Slack Socket Mode: https://docs.slack.dev/apis/events-api/using-socket-mode/
- DTCG Format 2025.10: https://www.designtokens.org/tr/2025.10/

## Update History

| Date | Version | Changes | Author |
|------|---------|---------|--------|
| 2026-11-21 | 1.0 | Initial version | Oven Spring team |
| 2026-11-25 | 1.1 | `GET /version` and the header's version (AC-071) | Oven Spring team |
| 2026-11-25 | 1.2 | Reconciled with the implementation: the channel trait and events, decisions with Cancel issue, the overview and issue detail shapes, the request check, the API routes | Oven Spring team |
