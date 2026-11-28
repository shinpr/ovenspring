# Review Feedback in Progress Design Document

## Overview

When a review loop ends without settling its findings, the issue goes to the lead as an exception. Today that exception shows as a failure in red on the issue page and on the overview list, even though the lead is just correcting the work and retrying. This design records in the flow why the issue went to the lead: either reviews still had findings, or something could not run. The daemon serves that cause in `Position.problem` as it does today, and serves which reviews hold the findings in `Position.feedback`, derived from the round state the flow already keeps. One dashboard rule decides whether a cause is a failure. Unsettled reviews show as ordinary progress naming the review ("The lead is addressing design review feedback"), and every other cause shows as a problem, as it does today.

### Referenced UI Spec

- UI Spec path: `docs/ui-spec/oven-spring-ui-spec.md`
- This design changes three rows: the Status vocabulary "Any" row (§ Status vocabulary, the table holding "▲ Problem: <cause>"), the GlanceRow "In progress" row and the IssueNow "In progress" row. Each one gains the addressing-feedback wording, and its failure wording stays as it is. The GlanceRow interaction "Activity with a running run → D-01" applies to the addressing-feedback status too.

## Background and Context

### Prerequisite ADRs

- `docs/adr/ADR-0005-file-board-and-state-outside-repository.md`: issue records and `events.jsonl` live in the state directory. The new `Cause` variant is written there, so records saved before the change must still load (see Migration Strategy).
- `docs/adr/ADR-0002-embedded-web-dashboard.md`: the dashboard reads the daemon's JSON API through the generated `dashboard/src/api/types.gen.ts`.

### External Resources Used

N/A: the change does not depend on any resource outside the repository.

### Requirement Convergence

- **Outcome**: While the lead is addressing review feedback that the review loop could not settle, the dashboard shows it as normal work in progress in plain words, such as "The lead is addressing design review feedback." A review that failed to run still shows as a failure, and a state that needs the owner still shows as waiting for them. (Source: `outputs/20261128-144006-requirements-GH-38.json`, convergence.outcome, and the GH-38 acceptance criteria.)
- **Non-Goals**: The review and retry workflow itself. The issue says: "The requested change is to how the dashboard presents these states."
- **Left open by the owner**: None.

### Standards and Assumptions

#### Applicable Standards
- [x] Exhaustive matches on closed enums, without wildcard arms `[implicit]`. Evidence: `crates/core/src/phase.rs:233-241`, `crates/core/src/flow.rs:1537-1577`, `crates/daemon/src/overview.rs:593-620`. Confirmed: Yes.
- [x] Rust types that the API serves are exported to TS through ts-rs, and a test fails when `types.gen.ts` drifts `[explicit]`. Source: `crates/server/src/types.rs:1-3`.
- [x] Dashboard words live in `dashboard/src/lib/vocabulary.ts`, and renderers do not hold their own wording `[implicit]`. Evidence: `vocabulary.ts:1-2`, `IssueView.tsx`, `Glance.tsx`. Confirmed: Yes.
- [x] Browser behavior across the production bundle and the API is proven in the fixture-e2e lane (`dashboard/e2e/*.fixture.e2e.test.ts`, `dashboard/playwright.config.ts:5-9`) `[explicit]`.

#### Assumed Behaviors
- [x] serde reads a JSON object tagged `"cause":"step"` as `Cause::Step` after a new variant is added, so issue records and `events.jsonl` lines saved before the change load unchanged. Evidence: `crates/core/src/flow.rs:198-212` (internally tagged enum `#[serde(tag = "cause")]`; existing variants keep their tags). Design effect: Selected Design remains valid; the test in Verification proves it. Confirmed: Yes.
- [x] The lead's retry after an unsettled review runs the same review again, and the dashboard already shows a running review by its activity (for example "Reviewing the design"). Evidence: `crates/core/src/flow.rs:1538-1577`, `crates/core/src/phase.rs:132`, GH-9 `state/events.jsonl` lines 836-837. Confirmed: Yes.
- [x] The round's retry does not depend on the recorded step when the round state names the reviews to recheck. At the round limit `round.correcting` holds the owning reviews (`flow.rs:1249-1251`, `recheck_round` 1319-1326 checks the limit before `file_rechecks` takes them). At kept-twice `round.maintained` is non-empty (`round_progress` 1407-1415), and `retry` takes it when `correcting` is empty (1550-1551). The step-filtered fallback (1553-1562) serves only snapshots saved before maintained reviews were recorded. Confirmed: Yes.
- [x] While the issue is in a round exception, the round state still names the reviews with outstanding findings, by the same rule `retry` reads: `round.correcting` when non-empty, otherwise `round.maintained`. At the round limit `recheck_round` fails before `file_rechecks` takes `correcting` (1319-1326). At kept-twice `correcting` is empty, because `file_rechecks` took it when the rechecks were filed (1329-1331), and `round_progress` fails with `maintained` non-empty (1407-1415). `exception()`/`to_lead()` change only the stage (611-618). Design effect: the review names need no stored copy. Confirmed: Yes (flow tests in Verification prove it on each exit).
- [x] Owner-input states already map to the "Waiting for you" section. Evidence: `crates/daemon/src/overview.rs:593-620`. Confirmed: Yes.

#### Quality Assurance Mechanisms
- [ ] `cargo test` (core, daemon; server type generation). Enforces: flow classification, Position serialization, TS contract drift. Config: `Cargo.toml`, `crates/server/src/types.rs`. Covers: `crates/**`. Status: `adopted`.
- [ ] clippy / rustfmt. Enforces: Rust lint and format. Config: `clippy.toml`, `rustfmt.toml`, `lefthook.yml`. Covers: `crates/**`. Status: `adopted`.
- [ ] `npm run check:all` in `dashboard/` (biome, knip, tsc, vitest including Storybook stories, playwright fixture-e2e). Enforces: exhaustive `switch` on `Cause`, unused exports, vocabulary tests, browser behavior on the production bundle. Config: `dashboard/package.json`. Covers: `dashboard/**`. Status: `adopted`.
- [ ] PRD AC-036. Enforces: a failed, blocked or round-limited step reaches the lead as an exception, and the owner hears only when the way out is theirs. Source: `docs/prd/oven-spring-prd.md:183`. Covers: flow routing. Status: `adopted` (routing is unchanged).

### Problem to Solve

In GH-9 the design review ended with an applied finding maintained twice (`flow.rs:1141-1145`). The issue went to the lead, and the issue page showed "The lead is handling a problem: Design review stopped" in red. The lead was already correcting the design and retried the review without the owner. The owner read a correction under way as a failure.

### Current Challenges

- `Cause::Step { step }` (`flow.rs:198-212`) carries only the step. All 14 `step_failed` callers record the same value, so nothing downstream can tell "the review still has findings" from "the review could not run". Both code/security round exits record `code-review` even when only the security review holds the outstanding finding (`flow.rs:1323`, `1413`).
- Both renderers draw any `Position.problem` as a failure: `IssueView.tsx:56-62` in `color-fail`, and `Glance.tsx:95-100` as "▲ Problem:". The overview's follow button depends on `agentAtWork(p.activity)` (`Glance.tsx:103`, `vocabulary.ts:83-85`), and activity is null during an exception (`phase.rs:115-116`).

### Requirements

#### Functional Requirements

- FR1: While the lead handles a review that ended with unsettled findings (an applied finding maintained twice, or past the owner's review round limit), the issue page and the overview row show normal in-progress styling and the words "addressing <review name> feedback", naming the reviews that hold the outstanding findings. This covers every review with a round limit: the PRD, UI Spec, decision record, design (including design check), plan, test, code and security reviews. Design comparison is not among them: its author's dispositions end the check and go to commit (`flow.rs:after_judgment`, `check_cleared` → `CommitPoint::DesignSync`), so it never re-reviews and cannot end unsettled. As for any running activity, the overview row and the issue page follow the lead's run, and the issue page names the lead. While that run is not running, neither view follows or names another run: not the review that just finished, a next review already running, or an earlier lead run on the same issue.
- FR2: When the next review starts, the status shows that review in progress, as it does today.
- FR3: An exception that is not an unsettled review shows as a failure, exactly as today. This includes a review that was blocked or stuck, a design comparison that fails to run, and every cause that is not a review, such as the CI fix limit.
- FR4: A state that needs the owner still shows as "Waiting for you".

#### Non-Functional Requirements

- **Compatibility**: issue records and `events.jsonl` lines saved before the change load, and keep their current failure meaning. Source: ADR-0005 state persistence; selected in Design Convergence as the lowest level that keeps existing state usable.

## Acceptance Criteria (AC) - EARS Format

### Addressing feedback (FR1)

- [ ] **When** an applied finding of a document or test check is maintained through two corrections (GH-9's path), the flow shall record `Cause::Unsettled { step: <that check's step> }`.
- [ ] **When** a document check's correction re-review exceeds `limits.review_rounds`, the flow shall record `Cause::Unsettled { step: <that check's step> }`.
- [ ] **When** a code/security review round exceeds `limits.review_rounds`, or its maintained findings are kept twice, the flow shall record `Cause::Unsettled { step: code-review }`, keeping the round state as today.
- [ ] **While** an issue is in progress with `Stage::Exception { cause: Unsettled { step }, .. }`, its Position shall have `problem` set to that cause (as for every exception) and `feedback` set to the reviews with outstanding findings: `[step]` for a document or test check; for a round, the distinct steps of `round.correcting`, or of `round.maintained` when `correcting` is empty, with code review before security review. The JSON shall read `"problem":{"cause":"unsettled","step":"<step>"},"feedback":[…]`. A security-only round gives `"feedback":["security-review"]`, and a round with both gives `["code-review","security-review"]`. In every other Position, `feedback` is `[]`.
- [ ] **While** `problem` is `unsettled` with `feedback: ["design-review"]` and the lead's exception run is running, the issue page shall show "**Lead** is addressing design review feedback · for <duration>" with "follow", without `color-fail`. The overview row shall show "Addressing design review feedback" as the follow button that opens the lead's run, without "▲ Problem". With `feedback: ["security-review"]` both read "security review feedback".
- [ ] **While** `problem` is `unsettled` and the issue has no running exception launch (the exception is saved and the lead's current run has not started yet, including after the owner answers a question the previous lead run asked), following the overview row shall open no run. In particular, it shall open neither the review that just finished nor the earlier, ended exception run.
- [ ] **While** the issue page shows an `unsettled` problem and its launches response holds a running launch that is not an exception launch (the lead retried and the next review started between the two reads), the page shall show "Addressing design review feedback · for <duration>" without naming the reviewer and without "follow".
- [ ] The wording names each review as it reads in running text: "design review", "design check", "PRD review", "UI Spec review", "decision review", "plan review", "test review", "code review", "security review". Two reviews read "code and security review", so the overview shows "Addressing code and security review feedback".

### Next review (FR2)

- [ ] **When** the lead's exception output is `retry` for `Cause::Unsettled { step, .. }`, the flow shall take the same transition, from the same issue state, that it takes for `Cause::Step { step }` (`flow.rs:retry`). A security-only round therefore files the security review's recheck. The issue then shows the review's activity, for example "Reviewing the design".

### Failures and owner input (FR3, FR4)

- [ ] **If** a check result is `blocked`, a review of the round is stuck, a launch is stuck, or any non-review step fails (CI fix limit included), **then** the flow shall record `Cause::Step { step }`. The issue page shall then show "The lead is handling a problem: <Step name> stopped" in `color-fail`, and the overview "▲ Problem: <Step name> stopped", as today.
- [ ] **If** a stored record or event line holds `{"cause":"step","step":"design-review"}` from before the change, **then** it shall load as `Cause::Step { step: DesignReview }`.
- [ ] **While** the lead asks the owner after an unsettled review (`Stage::LeadAsking { cause: Unsettled, .. }`), the issue shall be in the "Waiting for you" section.

## Existing Codebase Analysis

### Implementation Path Mapping
| Type | Path | Description |
|------|------|-------------|
| Existing | `crates/core/src/flow.rs` | `Cause`, `step_failed` and its callers, `retry`, Display |
| Existing | `crates/core/src/phase.rs` | `stopped_phase` matches `Cause`; `IssueState::feedback()` (below) |
| Existing | `crates/daemon/src/issues.rs` | `exception_request` matches `Cause` for the lead's request text |
| Existing | `crates/daemon/src/overview.rs` | `Position` gains `feedback`; `position()` serves `flow.feedback()` while in progress, and `problem` stays the exception cause |
| Existing (generated) | `dashboard/src/api/types.gen.ts` | `Cause`, `Position` |
| Existing | `dashboard/src/lib/vocabulary.ts` | `activityWords`, `problemWords`, `agentAtWork`, `STEP_NAME`; new `failure` and `statusRun` |
| Existing | `dashboard/src/app/issue/IssueView.tsx`, `dashboard/src/components/Glance/Glance.tsx` | The problem guard reads `failure(p)`; Glance's follow reads `agentAtWork(p)`; IssueView's role and follow come from `statusRun` |
| Existing | `dashboard/src/app/now/NowPage.tsx` | `onFollow` opens `statusRun(p, rows)` |
| Existing | `dashboard/src/fixtures/issue.ts`, `dashboard/src/fixtures/overview.ts` | Gain issue states for addressing feedback, running review, failed review and owner wait |
| Existing | `dashboard/e2e/now.fixture.e2e.test.ts`, `dashboard/src/lib/vocabulary.test.ts` | Proof of the renderings and wording |
| Existing | `docs/ui-spec/oven-spring-ui-spec.md` | The three display rows |

### Integration Points
- **Integration Target**: the flow's exception recording → the daemon's Position (unchanged) → the dashboard's failure rule → normal in-progress rendering.
- **Invocation Method**: `step_failed` callers in `flow.rs` choose the cause. `overview.rs:position()` serves it as `problem`. `failure(p)` decides the renderers' problem branch; `activityWords(p)` words an unsettled cause; `agentAtWork(p)` enables the overview's follow.

### Code Inspection Evidence

| File/Function | Relevance |
|---------------|-----------|
| `crates/core/src/flow.rs:checked` (1126-1150) | MaintainedApply twice → unsettled (GH-9 path); Blocked → failure |
| `crates/core/src/flow.rs:recheck` (1187-1195) | Document review round limit → unsettled |
| `crates/core/src/flow.rs:recheck_round` (1319-1326), `round_progress` (1397-1418), `judged` (1249-1251) | Code/security round limit and kept-twice → unsettled, with `round.correcting` / `round.maintained` still naming the reviews during the exception; stuck round review → failure |
| `crates/core/src/flow.rs:retry` (1538-1577) | Retry semantics that `Unsettled` must share |
| `crates/core/src/flow.rs` Display for `Cause` (215-225), `crates/daemon/src/issues.rs:1398-1410` | The lead's request text, which stays byte-identical |
| `crates/daemon/src/overview.rs:position` (549-590), `Times::since` (518-535) | `problem` already carries the cause; `since` already gives the exception launch time |
| `dashboard/src/app/issue/IssueView.tsx:inProgress` (54-97), `dashboard/src/components/Glance/Glance.tsx:progressStatus` (90-110) | One problem guard each, followed by the normal branch with duration, follow and on-hold note |
| `dashboard/src/app/now/NowPage.tsx:20-29`, `dashboard/src/app/issue/IssuePage.tsx` | The overview's follow opens the issue's running launch, or else its newest launch (`rows[0]`, "a run that ended just now is the one its activity named"). The issue page reads the issue and then its launches in two requests (`IssuePage.tsx:12-15`), and names and offers "follow" for the first running launch (`IssueView.tsx:72-91, 368-372`) |
| `crates/daemon/src/issues.rs:apply` (314-335), `crates/daemon/src/lib.rs` tick (520-521, 555-576) | The exception is saved before its item is filed, and open items launch later in the tick. An overview poll in between shows the exception while the newest launch is still the review that just finished |
| `dashboard/e2e/support.ts:63-78` | `serve(page, given)` answers the production bundle from overridable fixtures |

### Fact Disposition Table

| Fact ID | Focus Area | Disposition | Rationale | Evidence |
|---------|------------|-------------|-----------|----------|
| crates/core/src/flow.rs:step_failed callers | Classification of the 14 step_failed call sites | transform | 1144, 1193, 1323 and 1413 record `Cause::Unsettled` with the same `step` as today. 1132, 1402, 864 and the non-review callers (838, 906, 912, 985, 1011, 1073, 1600) keep `Cause::Step` | crates/core/src/flow.rs:838,864,906,912,985,1011,1073,1132,1144,1193,1323,1402,1413,1600 |
| GH-9 incident path | Which path produced the reported false alarm | transform | The MaintainedApply path (1144) is one of the four unsettled callers, so GH-9's case shows as progress | state/events.jsonl:834 (running design-review --Done--> exception cause step design-review); state/outputs/20261127-124001-design-review-GH-9.json verdict needs_revision with prior_feedback_reconciliation containing 'maintained'; exception output 20261127-124336 action 'retry' |
| crates/core/src/flow.rs:Cause | Persisted and shared shape of Cause | transform | A new variant `Unsettled { step }`. Old lines parse unchanged. Every Rust `Cause` match handles it through or-patterns that share `Step`'s arm on `step` | crates/core/src/flow.rs:198-212; crates/daemon/src/records.rs:124-132 (FactEvent { from: Stage, to: Stage }); dashboard/src/api/types.gen.ts:62 |
| crates/core/src/phase.rs:IssueState::activity | Activity is None during an exception | preserve | Activity stays None in exception. The wording comes from the unsettled cause in `problem` | crates/core/src/phase.rs:115-116, 246-270; crates/daemon/src/overview.rs:566 |
| crates/daemon/src/overview.rs:position | `problem` is the exception's cause | preserve | `problem` keeps meaning the exception's cause (`docs/design/owner-interface-design.md` § Dashboard server, `GET /overview`). The new `feedback` field is served beside it | crates/daemon/src/overview.rs:571-577 |
| dashboard/src/app/issue/IssueView.tsx:inProgress, Glance.tsx:progressStatus | Exception rendering differs from normal progress | transform | Each guard reads `failure(p)` instead of `p.problem`. An unsettled cause falls to the existing normal branch, and failures still take the problem branch | dashboard/src/app/issue/IssueView.tsx:56-62 vs 73-96; dashboard/src/components/Glance/Glance.tsx:95-108 |
| dashboard/src/lib/vocabulary.ts:agentAtWork | Overview follow interaction (UI Spec GlanceRow: activity with a running run → D-01) | transform | `agentAtWork` takes the Position and is also true for an unsettled cause, so the row's addressing-feedback words are the follow button | dashboard/src/components/Glance/Glance.tsx:103-108; dashboard/src/lib/vocabulary.ts:83-85; docs/ui-spec/oven-spring-ui-spec.md:263 |
| dashboard/src/app/now/NowPage.tsx:onFollow | Which run the overview's follow opens | transform | For an unsettled problem, the run the status names is the lead's running exception launch. Follow opens `statusRun(p, rows)`: the launch with `result === 'running'` and `step === 'exception'`, and otherwise none. For every other status, the current choice (running launch, else newest) is kept | dashboard/src/app/now/NowPage.tsx:20-29; crates/daemon/src/issues.rs:314-335; crates/daemon/src/lib.rs:555-576 |
| dashboard/src/app/issue/IssueView.tsx:NowLine follow | Which run the issue page names and follows | transform | `follow.running` is `statusRun(issue.now, runs)` when running. For every status but an unsettled one it equals today's first running launch. For an unsettled problem only a running exception launch supplies the role and follow; otherwise the line has neither | dashboard/src/app/issue/IssuePage.tsx:12-15; dashboard/src/app/issue/IssueView.tsx:73-92, 368-372 |
| docs/ui-spec/oven-spring-ui-spec.md | Spec wording fixed for every exception | transform | The three rows add the addressing-feedback wording and keep the failure wording | docs/ui-spec/oven-spring-ui-spec.md:130 (Any: ▲ Problem: <cause>), 263 (In progress status ▲ Problem in color-fail), 273 (IssueNow: 'The lead is handling a problem: <cause>' color-fail) |
| crates/daemon/src/overview.rs:section | Owner-input states | preserve | `section()` is unchanged, so LeadAsking/Stopped/AwaitingRecovery with either cause map to You | crates/daemon/src/overview.rs:593-620 |

## Design

### Change Impact Map

```yaml
Change Target: why an issue went to the lead, and how the dashboard words it
Direct Impact:
  - crates/core/src/flow.rs: new Cause::Unsettled { step }, recorded by the four unsettled callers through exception(); retry and Display or-patterns on step
  - crates/core/src/phase.rs: stopped_phase or-pattern; new IssueState::feedback()
  - crates/daemon/src/issues.rs: exception_request or-pattern (request text unchanged)
  - crates/daemon/src/overview.rs: Position gains feedback, from flow.feedback() while in progress
  - dashboard/src/api/types.gen.ts: regenerated
  - dashboard/src/lib/vocabulary.ts: failure(p); statusRun(p, runs); activityWords words an unsettled cause from p.feedback; agentAtWork(p); problemWords takes only failure causes
  - dashboard/src/app/issue/IssueView.tsx, dashboard/src/components/Glance/Glance.tsx: guard reads failure(p); Glance passes p to agentAtWork; IssueView's follow.running is statusRun(p, runs) when running
  - dashboard/src/app/now/NowPage.tsx: onFollow opens statusRun(p, rows)
  - dashboard fixtures (Position literals gain feedback), fixture-e2e test and vocabulary tests
  - docs/ui-spec/oven-spring-ui-spec.md: three rows
  - docs/design/owner-interface-design.md: Dashboard server `GET /overview` Position lists `feedback`; `GET /issues/{key}` names the same Position
Indirect Impact:
  - Issue records and events.jsonl gain a new tag value "unsettled" for new exceptions
No Ripple Effect:
  - Flow transitions, round state, exception routing (AC-036), the lead's exception request text and step files, overview.rs section(), Activity
```

### Interface Change Matrix

| Existing | New | Conversion Required | Compatibility Method |
|----------|-----|--------------------|--------------------|
| `Cause` = Step / DependencyLost / PullClosed / Push / Operation | adds `Unsettled { step: Step }`, serialized `{"cause":"unsettled","step":"<step>"}` | No | Additive variant; stored values keep their tags |
| `step_failed(s, step)` used by 14 callers | `step_failed` keeps 10 callers; the 4 unsettled callers call `exception(s, Cause::Unsettled { step })` | No | Internal to `flow.rs` |
| — | `IssueState::feedback(&self) -> Vec<Step>` | No | New read-only method |
| `Position.problem: Option<Cause>` | unchanged; may now hold `Unsettled` | No | Same field and meaning (the exception's cause); the TS `Cause` union gains a member |
| `Position` without review names | adds `feedback: Vec<Step>` (TS `feedback: Array<Step>`) | No | Derived per request; `[]` except for an unsettled exception in progress |
| `agentAtWork(activity: Activity \| null)` | `agentAtWork(p: Position)` | No | Its only caller is `Glance.tsx` |
| `problemWords(cause: Cause)` | `problemWords(cause: Failure)`, `Failure = Exclude<Cause, {cause: 'unsettled'}>` | No | Callers pass `failure(p)` |

### Data Flow

```
flow.rs callers 1144/1193/1323/1413 → exception(Unsettled{step}) → Stage::Exception { cause: Unsettled{step}, resume: None }  (round state kept as today)
other callers                       → step_failed               → Stage::Exception { cause: Step{step},      resume: None }
        ↓ (issue record, events.jsonl)
overview.rs position(): in progress and Exception{cause} → problem: Some(cause)   (unchanged)
                        in progress                      → feedback: flow.feedback()  (round state → review steps)
        ↓ JSON (camelCase)
dashboard: failure(p) !== null → existing failure branch (problemWords)
           otherwise           → normal branch; activityWords(p) = "Addressing <names of p.feedback> feedback" when problem is unsettled;
                                 Glance follow when agentAtWork(p) → NowPage onFollow(key) → statusRun(p, launches)
                                 IssuePage (issue + launches) → IssueView: role and follow from statusRun(p, runs) when it is running
           statusRun: unsettled → the running exception launch, else none; any other status → the running launch, else the newest
```

### Main Components

#### Flow (`crates/core/src/flow.rs`, `crates/core/src/phase.rs`)
- **Responsibility**: records why the issue went to the lead, and reads from its own round state which reviews hold outstanding findings. It stays the only place that classifies the reason.
- **Interface**:
  - `Cause::Unsettled { step }`, with doc comment "Reviews still had findings when their loop ended: an applied finding maintained twice, or the owner's round limit." `step` is the step that stopped, the same value `step_failed` records today. Display is the same as `Step`'s (the step's name).
  - `IssueState::feedback(&self) -> Vec<Step>`, beside `activity()` in `phase.rs`: when the stage is `Exception { cause: Unsettled { step }, .. }`, the reviews whose findings remain. For a round step (`code-review`), the distinct `Review::step()` of `round.correcting`, or of `round.maintained` when `correcting` is empty, the same rule `retry` reads (`flow.rs:1550-1551`). Both sets are `BTreeSet<Review>`, so the order is code review before security review. For any other step, `[step]`. Any other stage gives `[]`.
- **Dependencies**: `retry` treats `Unsettled { step }` exactly as `Step { step }`. Both arms use or-patterns (`Cause::Step { step: … } | Cause::Unsettled { step: … }`), including the round-review arm, so the same round state files the same rechecks.

#### Overview (`crates/daemon/src/overview.rs`)
- `Position.feedback: Vec<Step>`, doc comment "The reviews whose findings the lead is addressing." `position()` sets it to `flow.feedback()` when in progress, and to `[]` otherwise. `problem` keeps serving the exception's cause, `Unsettled` included.

#### Vocabulary (`dashboard/src/lib/vocabulary.ts`)
- **Responsibility**: the words for Position, and the one rule for whether a cause is a failure.
- **Interface**:
  - `failure(p: Position): Failure | null` returns `p.problem` unless its `cause` is `'unsettled'`. Both renderers' problem guards call it.
  - `activityWords(p)` returns `Addressing ${reviewNames(p.feedback)} feedback` when `p.problem?.cause === 'unsettled'`. Each name is `STEP_NAME[step]` with its first letter lowercased, unless its first word is "PRD" or "UI". With several names, every name but the last drops its trailing " review", joined with " and " ("code and security review"). `reviewNames` stays private to the module.
  - `agentAtWork(p: Position)` is true for an unsettled cause, or for an activity outside `FACTORYS_OWN`. The lead's exception run is an agent at work.
  - `statusRun(p: Position, runs: readonly LaunchRow[]): LaunchRow | undefined`, beside `agentAtWork`, doc comment "The run the status names." When `p.problem?.cause === 'unsettled'`, it returns `runs.find(r => r.result === 'running' && r.step === 'exception')`, with no fallback. Otherwise it returns `runs.find(r => r.result === 'running') ?? runs[0]`, today's NowPage choice with its comment "a run that ended just now is the one its activity named". It is the one place that knows which run a status names, and both follow consumers below call it.
  - `problemWords(cause: Failure)` keeps its arms. It has no `'unsettled'` arm because the type excludes it.

#### Follow (`dashboard/src/app/now/NowPage.tsx`, `dashboard/src/app/issue/IssueView.tsx`)
- **Responsibility**: the overview row and the issue page open, and the issue page names, the run that the displayed status names.
- **Interface**:
  - `NowPage` `onFollow(key)` keeps its signature. It reads the row's Position from `overview.data.issues` by `key`, fetches the issue's launches as today, and opens `statusRun(p, rows)`.
  - `IssueView` passes `follow.running` to `NowLine` as `statusRun(issue.now, runs)` when that run's `result` is `'running'`, and as `undefined` otherwise. The issue page keeps offering "follow", and naming the run's role, only for a running launch. For every status but an unsettled one this is today's value (`runs.find(r => r.result === 'running')`). With an unsettled problem and no running exception launch, `inProgress` takes its existing branch for no running launch: the line reads "Addressing design review feedback · for <duration>", with no role and no follow.
- **Why here**: the exception is saved before the lead's run starts (`issues.rs:314-335`, `lib.rs:555-576`), so either view can show the status before that run exists. Each view also reads its two inputs separately: the overview's follow fetches launches after the overview poll, and `IssuePage.tsx:12-15` reads the issue and then its launches in sequence. The lead's retry can start the next review between the reads (`flow.rs:1538-1577`), so the launches can hold a running review while the status still says the lead is addressing feedback. Taking the first running launch would then name the reviewer ("Reviewer is addressing design review feedback") and follow the review. The newest-launch fallback would open the finished review, or the earlier exception run after an owner answer: after the owner answers a lead's question, the flow files a new exception item for the same cause (`owner.rs:884-915`, `flow.rs:615-618, 1427-1433`), and until it launches the newest launch is the ended exception run that asked (`history.rs:850-870`). Only one exception run of an issue runs at a time, so a running exception launch is the lead's run that the status names. Opening nothing during the fraction of a tick before that run starts matches the owner's answer. The rule lives in `statusRun` because both views must agree on it.

### Selected Design

Recording `Cause::Unsettled { step }` in the flow, serving the outstanding reviews from the existing round state, and deciding failure in one dashboard rule delivers FR1 through the renderers' existing normal branch. That branch already shows the role, duration, follow link and on-hold note, and the overview's follow comes from `agentAtWork`. FR2 and FR4 hold without change (Assumed Behaviors). FR3 holds because every other cause is a `failure`.

- **Addition**: `Cause::Unsettled { step }` (persisted variant; the owner approved adding `unsettled`, `sources/answers/GH-38-design-27.md`)
  - **Current evidence**: FR1/FR3 need to tell an unsettled review from a blocked or stuck one, but `Cause::Step` records both identically (`flow.rs:621`, callers 1132/1402/864 vs 1144/1193/1323/1413).
  - **Lower-surface insufficiency**: Deriving the reason at overview time would copy flow's classification into `overview.rs`. It would also need the config limits threaded into `position()`, and could not tell a stuck review from a stale output. A `#[serde(default)]` field on `Cause::Step` would allow an "unsettled" non-review step, a combination that must not exist.
  - **Subtraction result**: Without the variant, "blocked/stuck review shows as failure" and "unsettled review shows as progress" cannot both hold (GH-38 AC1 and AC3).
- **Addition**: `IssueState::feedback()` and `Position.feedback` (derived, not stored)
  - **Current evidence**: FR1 names the reviews, and a round's outstanding findings can belong to the security review alone or to both kinds. The owner asks for "security review feedback" when only security has outstanding findings, with the names derived from the existing round state unless a current consumer needs them stored (`sources/answers/GH-38-design-27.md`). The round state already holds them during the exception (Assumed Behaviors), and no consumer reads them from the stored cause: the dashboard reads Position, which is derived per request.
  - **Lower-surface insufficiency**: The dashboard has no round state, so the names must cross the API. `problem` serves the persisted `Cause` type, which keeps no names. Deriving them in `overview.rs` from the round fields would put the round rule outside the flow, which owns it.
  - **Subtraction result**: Without it, a security-only round would read "code review feedback" (FR1; the owner's answer).
- **Narrowing**: for an unsettled problem, the overview follow (`NowPage.tsx`) and the issue page's role and follow (`IssueView.tsx`) take only a running exception launch, through `statusRun`, and drop both the first-running choice and the newest-launch fallback.
  - **Current evidence**: I002, I005, and the owner's answer: "Keep Follow tied to the lead's current run. Until it starts, clicking Follow should open nothing." The exception is saved before the lead's launch starts, so a poll in between would let follow open the finished review (`issues.rs:314-335`, `lib.rs:555-576`, `NowPage.tsx:25`), or, after an owner answer, the earlier exception run (`owner.rs:884-915`, `flow.rs:1427-1433`). Both views read the status and the launches separately (`IssuePage.tsx:12-15`), so a retry between the reads gives a running review that the first-running choice would name and follow (`IssueView.tsx:370`). The UI Spec scopes the GlanceRow follow to the run of the displayed activity (`oven-spring-ui-spec.md:263`).
  - **Lower-surface insufficiency**: Not making the unsettled status followable would drop FR1's follow. A daemon-side "run started" field would add API surface for an interval that lasts a fraction of one tick.
  - **Subtraction result**: Without it, following the row in that interval opens the wrong run, and the issue page attributes the lead's feedback work to the reviewer and follows the review (FR1, UI Spec GlanceRow). Without the shared `statusRun`, the two views would each hold the rule and could disagree.
- **Addition**: `failure(p)` (one dashboard predicate)
  - **Current evidence**: Both renderers guard on `p.problem !== null` (`IssueView.tsx:56`, `Glance.tsx:95`).
  - **Lower-surface insufficiency**: Writing the check inline in each renderer would put the is-it-a-failure rule in two places.
  - **Subtraction result**: Without it, an unsettled review stays red (FR1).

### Data Contracts

#### Position (daemon → dashboard)

```yaml
Contract: crates/daemon/src/overview.rs Position (ts-rs → types.gen.ts)
Output:
  Type: problem: Cause | null, where Cause gains {cause: "unsettled", step: Step}; new feedback: Step[]
  Guarantees:
    - problem !== null only when section is in-progress and stage is Exception; problem is that cause (unchanged)
    - feedback is non-empty exactly when problem is unsettled; each entry is a review step, distinct, code review before security review
    - activity is null whenever problem is non-null (unchanged phase.rs behavior)
    - since is the exception launch time (Times::since, unchanged)
Invariants:
  - section(), phase, task, onHold, wait, ended, endedAt are computed as before
```

### Field Propagation Map

| Field | Boundary | Status | Serialized Format | Consumer Parse Rule | Detail |
|-------|----------|--------|-------------------|---------------------|--------|
| `Cause::Unsettled.step` | flow → issue record / `events.jsonl` (inside `Stage`) | preserved | `{"cause":"unsettled","step":"code-review"}` (tag `cause`, kebab-case) | serde internally tagged `Cause`; `FactEvent` reads typed `Stage` (`records.rs:124-132`) | New exceptions only |
| `round.correcting`, `round.maintained` | issue record → `IssueState::feedback()` | transformed | unchanged in the record | distinct `Review::step()`; `correcting`, else `maintained` | Round exceptions only |
| `Position.problem` | daemon → dashboard JSON | preserved | the cause as above, or as today for other causes | `types.gen.ts` `Cause \| null`; `failure(p)` splits it | |
| `Position.feedback` | daemon → dashboard JSON | new | `"feedback":["security-review"]`, `[]` otherwise | `types.gen.ts` `Array<Step>`; `activityWords` names it | |

### State Transitions and Invariants

```yaml
State Transitions (only the recorded cause changes; targets and round state are unchanged):
  Running{check step} → MaintainedApply twice | round limit → Exception{Unsettled{step}}          feedback [step]
  Reviewing → round limit | round kept twice → Exception{Unsettled{code-review}}                feedback from correcting | maintained
  Running{review} → Blocked | Stuck → Exception{Step{step}}            (unchanged)
  Exception{Unsettled{step}} → lead retry → same as Exception{Step{step}} retry from the same state (unchanged target)
  Exception{Unsettled} → lead ask/recover → LeadAsking/AwaitingRecovery{Unsettled}  (section You, unchanged)
System Invariants:
  - Routing to the lead (AC-036) and every transition target are identical to today's for the same events
```

### UI Error State Design

| Component / Screen | Loading | Empty | Error | Partial |
|-------------------|---------|-------|-------|---------|
| IssueNow / GlanceRow | unchanged | unchanged | failure exception: unchanged `color-fail` / "▲ Problem" | N/A |

### Error Handling

| Error Category | Example | Detection | Recovery Strategy | User Impact |
|---------------|---------|-----------|-------------------|-------------|
| Business logic | Review blocked or stuck | `CheckResult::Blocked`, `Fact::Stuck`, `round.stuck` | Unchanged: lead exception | Shown as a problem (FR3) |
| Infrastructure | State written by a newer binary read by an older one | serde rejects unknown tag `unsettled` | None added (see Risks) | Older binary cannot load that issue until upgraded again |

## Implementation Plan

### Implementation Approach

**Selected Approach**: Vertical slice: one slice through core → dashboard → UI Spec.
**Selection Reason**: The outcome is one display change. Each layer's part is small and only observable end to end, and the generated TS type couples the Rust and dashboard sides in one build.

### Technical Dependencies and Implementation Order

1. **Core cause and feedback**: `Cause::Unsettled`, the four callers, the or-patterns in `retry`, Display, `phase.rs:stopped_phase` and `issues.rs:exception_request`; `IssueState::feedback()`; `Position.feedback` in `overview.rs`. Regenerate `types.gen.ts` (`OVENSPRING_WRITE_TYPES=1 cargo test -p ovenspring-server`).
   - Technical Reason: everything downstream reads the recorded cause. Flow tests prove the riskiest premise: that exactly the four callers are the unsettled ones, GH-9's path is among them, and the round state at each round exit makes `feedback()` name the security review alone when only it holds findings.
   - Prerequisites: none.
2. **Dashboard rule and wording**: `failure`, `activityWords`, `agentAtWork(p)`, `problemWords(Failure)`, the two guards, `statusRun` and its two callers (`NowPage` follow, `IssueView` role and follow), fixtures for the issue states (existing Position literals gain `feedback: []`), `vocabulary.test.ts` cases, the fixture-e2e test.
   - Prerequisites: 1 (generated type).
3. **UI Spec rows**: § Status vocabulary "Any" row, GlanceRow "In progress" row and IssueNow "In progress" row. Each gains "Addressing <review> feedback" in normal styling, and failures keep "<step name> stopped" in `color-fail`.
   - Prerequisites: 2 (wording settled).

### Migration Strategy

No migration. Existing records and event lines hold only `Cause::Step` and the other existing tags, which parse unchanged and keep their failure meaning. An issue sitting in an exception recorded before the change keeps showing as a problem until the lead acts on it. `Position` is derived on each request.

## Security Considerations

- **Authentication & Authorization**: N/A. The local dashboard API is unchanged, and no new entry point is added.
- **Input Validation**: N/A. The new variant is produced by the flow and serialized by serde, with no external input.
- **Sensitive Data Handling**: N/A. Step names only.

## Test Boundaries

### Mock Boundary Decisions

| Component/Dependency | Mock? | Rationale |
|---------------------|-------|-----------|
| Flow (`drive()` / `step()` scenarios) | No | Pure state machine; existing tests drive it with real outputs |
| `position()` | No | Built from an `IssueRecord` and an `Around` in existing overview unit tests |
| Dashboard API in fixture-e2e | Yes (fixtures through `serve`) | The lane's boundary: the production bundle in a real browser, its API answered from typed fixtures |

### Data Layer Testing Strategy

- **Schema dependencies**: `Stage`/`Cause` in issue records and `events.jsonl` (`crates/core/src/flow.rs`, `crates/daemon/src/records.rs:124-132`).
- **Test data approach**: a literal pre-change JSON line (GH-9 `events.jsonl:834` shape) deserialized in a unit test.
- **Mock limitations acknowledged**: the fixture-e2e lane's issue states are typed by the generated `IssueDetail`, so it proves the dashboard consumes the serialized shape. The overview serialization test proves the daemon produces that shape.

### Integration Verification Points

- Daemon producer → JSON: `position()` serialized with `serde_json` (the server's serializer) reads `"problem":{"cause":"unsettled","step":"design-review"}` and `"feedback":["design-review"]`.
- JSON → production bundle → issue page and overview: the fixture-e2e test through `IssuePage` (issue and launch responses joined) and `NowPage` (follow opens the running exception launch; no run opens while the lead's current run has not started). Both views reach `statusRun`, so the same case proves that a running review in the launches is neither named nor followed under an unsettled status.

## Verification Strategy

### Correctness Proof Method

- **Correctness definition**: The four unsettled paths record `Unsettled`, and show as progress naming the reviews that hold the findings, with the lead's run followable from both views. Every other exception records and shows exactly as today, retry from either cause takes the same transition, and owner-input states stay in "Waiting for you".
- **Verification method** (each claim owned once):
  - `crates/core/src/flow.rs` tests (classification and routing, each branch on an existing scenario):
    - Document check, kept twice: `an_applied_finding_maintained_through_two_corrections_goes_to_the_lead` (2397) asserts `Unsettled { step: design-review }` and `feedback() == [design-review]`.
    - Document check, round limit: new `a_document_review_past_the_owners_round_limit_goes_to_the_lead`, driving the design-review correction loop with `review_rounds: Some(1)` (the `recheck` branch, 1193). It asserts `Unsettled { step: design-review }` and `feedback() == [design-review]`.
    - Round, round limit: `rounds_have_no_limit_unless_the_owner_sets_one` (2835) asserts `Unsettled { step: code-review }` and `feedback() == [code-review]`. A second case with `correcting: [SecurityReview]` asserts `feedback() == [security-review]`. A third with `correcting: [CodeReview{1}, SecurityReview]` asserts `[code-review, security-review]`.
    - Round, kept twice: `a_finding_kept_through_two_corrections_goes_to_the_lead` (2995) asserts `Unsettled { step: code-review }` and `feedback() == [code-review]`. A security-only variant (code review passes, security review keeps F001 twice) asserts `Unsettled { step: code-review }` and `feedback() == [security-review]`. That variant continues with the lead's `retry`, which files `security-review`, as today's `Step { code-review }` retry does from the same state.
    - Failures unchanged: `a_review_that_could_not_run_waits_for_the_round_then_goes_to_the_lead_who_may_rerun_it` (2555) and a blocked check still record `Cause::Step`, and `feedback()` is empty.
  - A serde test that `{"cause":{"cause":"step","step":"design-review"},"resume":null,"stage":"exception"}` loads as `Stage::Exception { cause: Cause::Step { step: DesignReview }, resume: None }`.
  - `crates/daemon/src/overview.rs` test: an in-progress record in `Exception{Unsettled{DesignReview}}` serializes, through `serde_json::to_value(position(..))`, with the `problem` and `feedback` JSON above and `activity: null`. A `LeadAsking{Unsettled}` record gives section `you`.
  - `dashboard/src/lib/vocabulary.test.ts`: `activityWords` for an unsettled problem with `feedback` `[design-review]`, `[prd-review]`, `[ui-spec-review]`, `[design-verify]`, `[security-review]` and `[code-review, security-review]` gives "Addressing design review feedback", "Addressing PRD review feedback", "Addressing UI Spec review feedback", "Addressing design check feedback", "Addressing security review feedback" and "Addressing code and security review feedback". `failure` returns null for unsettled and the cause for `step`.
  - `dashboard/e2e/now.fixture.e2e.test.ts`, one test per issue state, each serving the production bundle with `serve(page, { overview, issue, launches })`:
    - Addressing feedback: the lead's exception launch is running and `problem` is unsettled with `feedback` `[design-review]`. The overview row's button "Addressing design review feedback" opens the run dialog for the lead's exception launch, and nothing reads "▲ Problem". The issue page shows "Lead is addressing design review feedback" with "follow" and no `fail` class.
    - Lead's current run not started: `problem` is unsettled with `feedback` `[design-review]`, and the launches hold, newest first, a running design review (the next review, started between the reads), an ended exception launch (the run that asked the owner) and the completed design review. Clicking the overview row's button opens no run dialog, and no request for any of the three launches is sent. The issue page shows "Addressing design review feedback", without "Reviewer" and without "follow". This one case covers both earlier pending cases and the mismatched responses (I005): the running review is what the first-running choice would take, the ended exception launch is the owner-answer case, and the completed review is what the fallback would open when no exception launch exists.
    - Next review: a running design-review with activity `reviewing-the-design`. The issue page shows "Reviewer is reviewing the design".
    - Failed review: `problem` is `{cause: 'step', step: 'design-review'}` and `feedback` is `[]`. The issue page shows "The lead is handling a problem: Design review stopped" with the `fail` class (kept, as the owner asked).
    - Owner wait is not repeated in the browser: section `you` for `LeadAsking{Unsettled}` is owned by the `crates/daemon/src/overview.rs` unit test, and the issue page's `you` branch (`IssueView.tsx` `NowLine`, unchanged by this work; the ask text is the existing `ASK` lookup, "Answer a question" for `answer-question`) by the existing `IssueView.stories.tsx` `WaitingForYou` story.
- **Verification timing**: core and overview tests after step 1; vocabulary and fixture-e2e after step 2.

### Early Verification Point

- **First verification target**: the flow tests of step 1.
- **Success criteria**: GH-9's MaintainedApply scenario ends in `Unsettled { step: DesignReview }` with `feedback() == [DesignReview]`. The security-only round ends with `feedback() == [SecurityReview]` and retries into `security-review`. The could-not-run scenario ends in `Cause::Step`.
- **Failure response**: if a path cannot be classified at its call site, for example because one caller covers both cases, or the round state does not name the outstanding reviews at an exit (so the names would have to be stored), return to this design before building the dashboard rule.

### Output Comparison

- **Comparison input**: the existing failure fixtures and flow scenarios (`dashboard/src/fixtures/overview.ts` `problem`, flow tests for 838/864/906/912/985/1011/1073/1132/1402/1600).
- **Expected output fields**: the recorded `Cause`; `Position.problem`; the rendered failure text and class.
- **Diff method**: the existing assertions and stories pass unchanged.
- **Transformation pipeline coverage**: Exception{cause} → `Position.problem`, round state → `Position.feedback` (overview serialization test); `flow.activity()` → `activity` (unchanged, null); Position → JSON → TS (types drift test, fixture-e2e); `problem` → failure or words, and follow (vocabulary tests, fixture-e2e).

## Design Boundaries

- **Intentional limitations**:
  - Fix-CI round limit (1600) and refused commits (1073) stay as failures. They are not review feedback, and the requirement covers reviews with a round limit.
  - The lead's exception request text and the exception step file are unchanged. The non-goal excludes the review/retry workflow, and the Display or-pattern on `step` keeps the text identical.
  - Exceptions recorded before the change keep their failure display.
  - Any status other than an unsettled problem keeps the follow's current newest-launch fallback, including in the interval before its launch starts. That interval existed before this change, and this issue does not change those statuses.
- **Existing extension points used**: `Position.problem`, the round state (`round.correcting`, `round.maintained`), `Times::since` (exception launch time), `IssueView`/`Glance` normal in-progress branch, `agentAtWork`, `STEP_NAME`, the fixture-e2e `serve`.

## Risks and Mitigation

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| Downgrading the binary after an `Unsettled` exception is recorded makes that issue's record and event lines fail to parse | Medium | Low | Forward reading is not a current contract (releases move forward). Stated here; no code added |

## Review Corrections

| Finding | Disposition | Design effect |
|---------|-------------|---------------|
| I001 | apply | The outstanding reviews are named from the round state through `IssueState::feedback()` and `Position.feedback`; `step` keeps routing and request text |
| I002 | apply | `agentAtWork(p)` covers an unsettled cause, and the overview follow opens only a running exception launch for it (`NowPage.tsx`), so the interval before that launch starts opens no run instead of the finished review or an earlier, ended exception run (the owner-answer case). The running case and the pending case are verified in fixture-e2e |
| I003 | apply | `problem` keeps the cause and `failure(p)` is the one presentation rule. The later `Position.feedback` carries only review names derived from round state, not a split of the cause |
| Conflict-001 | apply | Design comparison removed from FR1's coverage and the wording list. Its author-disposition path to commit is unchanged, and a design comparison that fails to run records `Cause::Step` and shows as a failure (FR3) |
| I004 | apply | Flow scenarios per branch (document round limit and security-only added); overview serialization test; fixture-e2e over the issue states |
| I005 | apply | The issue page took the first running launch for its role and follow, so a retry between its two reads could name the reviewer and follow the review under the lead's status. `statusRun(p, runs)` in `vocabulary.ts` is now the one rule for which run a status names; `NowPage` and `IssueView` both call it, and IssueView keeps following only a running launch. The combined pending-run fixture-e2e case gains a running design review and an issue-page assertion |
| Conflict-002 | apply | `docs/design/owner-interface-design.md` § Dashboard server: the `GET /overview` Position lists `feedback` (review steps during an `unsettled` exception, `[]` otherwise, pointing here for derivation) with `problem` still the exception's cause; `GET /issues/{key}` names the same Position |
| Owner answer (`sources/answers/GH-38-design-27.md`) | apply | `unsettled` approved. `Cause::Unsettled` drops its stored `reviews`; the names come from round state (`IssueState::feedback()`, `Position.feedback`), since no current consumer needs them stored. Security-only reads "security review feedback". Follow stays tied to the lead's current run and opens nothing before it starts. The two pending-run browser cases are one case. The failed-review issue-page assertion is kept |

## References

- Issue GH-38; requirements `outputs/20261128-144006-requirements-GH-38.json`; codebase analysis `outputs/20261128-144129-codebase-analysis-GH-38.json`
- `docs/prd/oven-spring-prd.md` AC-036; `docs/ui-spec/oven-spring-ui-spec.md`; `docs/design/owner-interface-design.md` § Dashboard server
