# Correction Under Way Design Document

## Overview

An issue's saved state records which correction is under way. The correction raised after the build (the review round's, or the pull request comments') has its own slot, apart from the check whose findings are being corrected, so a correction raised inside it no longer overwrites it. Core decides each kind of correction's continuation in one route function per decision. The transitions apply those routes, and the phase and the dashboard's "Next" line read them. The daemon's handoffs read the same state. The `test-review` item describes the change it reviews with two trees: the worktree when the task or correction's executor started, and the worktree when it ended. `diffBase`, `diffHead` and `changedFiles` all come from that pair.

## Background and Context

### Prerequisite ADRs

None. `docs/adr/` holds no ADR governing the issue flow. The governing merged design is `docs/design/factory-core-design.md` (§ Steps table, § Phases); this design updates the rows it changes, and the statements in `owner-interface-design.md` and `events-and-error-notices-design.md` that read the changed state (Implementation Path Mapping; Implementation Order item 5).

### External Resources Used

None. The change is confined to the repository: core flow, daemon, dashboard and one factory step file.

### Requirement Convergence

- **Outcome**: Which correction a step belongs to, and what follows it, is decided in one place for each kind of correction, with an outer correction kept while an inner one runs; the flow's transitions, the daemon's handoffs and the dashboard read that decision instead of inferring it. (Requirements record `outputs/20261128-143909-requirements-GH-52.json`, from GH-52 § Expected.)
- **Non-Goals**: A separate reproduction of the GH-34 path (a merge-base or fix-ci commit followed by a comment correction's test review), which main no longer reaches (GH-52 comment, Proof).
- **Left open by the owner**: None.

### Standards and Assumptions

#### Applicable Standards
- [x] Rust format, clippy `-D warnings`, `cargo test --workspace`, `cargo deny`, `cargo machete` `[explicit]` - Source: `.github/workflows/ci.yml` (rust job)
- [x] Dashboard `pnpm run check:all` (biome, knip, tokens, type-check, vitest including Storybook story tests) `[explicit]` - Source: `.github/workflows/ci.yml` (dashboard job), `dashboard/package.json`
- [x] Generated TS types are checked against the Rust types; `OVENSPRING_WRITE_TYPES=1 cargo test -p ovenspring-server` rewrites `dashboard/src/api/types.gen.ts` `[explicit]` - Source: `crates/server/src/types.rs:1-30`
- [x] Earlier saved issue states stay readable and their in-flight continuations finish `[implicit]` - Evidence: `crates/core/src/flow.rs:253-263` (`After::Correction` retained), `:284-285` (`Round.author_follows`), `:1173` (`DesignVerify`) - Confirmed: Yes
- [x] Integration tests carry `// AC:`, `// Behavior:`, `// @lane:`, `// @dependency:` headers `[implicit]` - Evidence: `crates/cli/tests/issue_flow_int.rs:2252-2259` - Confirmed: Yes
- [x] Dashboard text is written in words the owner understands `[explicit]` - Source: `dashboard/CLAUDE.md`

#### Assumed Behaviors
- [x] Comparing two recorded trees sees what a worktree comparison misses. `tree_of` (`crates/daemon/src/issues.rs:1070-1091`) writes the worktree through a temporary index (`git add -A`, `git write-tree`), so a new file is in the recorded tree even though it stays untracked in the worktree's own index. Evidence: a probe in a temporary repository (design review finding I003) recorded a tree before and after adding `tests/new.rs` and deleting `tests/old.rs`, both untracked in the ordinary index. `git diff-tree -r --name-only <before> <after>` listed both. `git diff --quiet <before> -- tests/new.rs` exited 0, because the worktree comparison does not see the untracked file. `git diff --quiet <before> <after> -- <path>` exited 1 for both, the deletion included. Design effect: whenever the item gives `diffHead`, the reviewer compares `diffBase` with it, not with the worktree (Selected Design, item 8). Confirmed: Yes
- [x] The worktree can change between the tree recorded after one executor step and the start of the next one. Evidence: `quality` and other agent steps edit the worktree without recording a tree (`issues.rs:966-990`), and `place_for` merges what was pushed to the branch into the worktree right before every launch (`crates/daemon/src/lib.rs:625-651`, `bring_in_pushed`). Design effect: the start of a change is recorded when its executor's launch is placed, not taken from the previous recorded tree (Selected Design, item 6). Confirmed: Yes
- [x] Pushed commits can be merged into a change that is still open, and not necessarily by the executor's own launch. Evidence: pushed commits enter an issue worktree only through `pulls::merge_from_origin` (`crates/daemon/src/pulls.rs:921`). It has two issue-worktree callers: `bring_in_pushed`, which `place_for` runs before every agent launch on an issue with a pull request (`lib.rs:628-649`), and `push` through `bring_in` (`pulls.rs:187-193, 287-312`). A blocked executor files the lead's `exception` first (`flow.rs:611-623`). That launch's `place_for` merges what was pushed, and the retried executor's merge then skips the already contained head (`pulls.rs:930-934`, design review finding I002, round 3). So only the merge itself sees what it brought in. The start can be moved past what a merge brought in, keeping the earlier attempt's edits: the tree after the merge, with each path that differs between the change's start and the tree before the merge set back to its version at the start. A probe in a temporary repository (design review finding I002) covered this. The first attempt edited `edited`, added `added` and deleted `removed`. The merge then brought in `pushed`, and the second attempt edited `keep`. The probe built the moved start with `git read-tree <post>` and `git diff-tree -r --no-renames <before> <pre>`, feeding each entry's old mode and blob to `git update-index --index-info` (mode 0 removes the path), then ran `git write-tree`. `git diff-tree -r --name-only <moved start> <after>` listed `added`, `edited`, `keep` and `removed`, not `pushed`, and `git diff --quiet <moved start> <after> -- <path>` exited 1 for each. Design effect: Selected Design, item 6. Confirmed: Yes
- [x] An item already on the board keeps the entries it was filed with across an upgrade, while its launch reads the step instructions of the running version. Evidence: `reconcile_issue` skips filings whose item is present (`issues.rs:1249-1261`); the prompt is assembled from the embedded step file at launch (`lib.rs:prompt_for`, `crates/agents/src/prompt.rs:assemble`). Design effect: the step file accepts a `test-review` item without `diffHead` (Selected Design, item 8). Confirmed: Yes
- [x] `review_round` is 1 on a check's first run and grows by one on each re-review after a correction. Evidence: `check()` sets 1 (`flow.rs:728-733`), and `recheck()` increments it (`flow.rs:1187-1197`). An exception `Retry` reruns the step without changing it (`flow.rs:1570-1575`). Design effect: the test review's recheck signal (Selected Design, item 5). Confirmed: Yes
- [x] Unknown JSON fields are ignored when a saved `IssueRecord` is read. Evidence: no `deny_unknown_fields` in `crates/core/src/flow.rs` or `crates/daemon/src/issues.rs`. Design effect: written records may omit the legacy fields. Confirmed: Yes

#### Quality Assurance Mechanisms
- [ ] Rust quality gate — Enforces: format, lint, tests, build, licenses, unused deps — Config: `.github/workflows/ci.yml` — Covers: `crates/**` — Status: `adopted`
- [ ] Dashboard `check:all` — Enforces: lint, format, unused code, tokens, types, unit and story tests — Config: `dashboard/package.json` — Covers: `dashboard/**` — Status: `adopted`
- [ ] Generated types check — Enforces: `types.gen.ts` equals the Rust `TS` types — Config: `crates/server/src/types.rs` — Covers: `Position`, the new `Following` and `Upcoming` types — Status: `adopted`

### Problem to Solve

The flow, the daemon and the dashboard each decide on their own which correction a step belongs to. They work it out from one `origin` slot and the fields read with it, and that slot cannot hold an outer correction while an inner one runs. As a result, a comment correction can end in a new review round instead of the push and replies. A task's test review can receive another task's review. A test review gets no task file and can get a `changedFiles` list that disagrees with `diffBase`. The issue page can name tasks that no longer remain.

### Current Challenges

Observed (GH-52 § What was observed and its comment; `codebase_analysis` currentPath):

1. `checked` overwrites `origin` with `TestReview`/`Document` on findings (`flow.rs:1132-1138`), and `next_task_or_redo` clears every origin but `Round` before `quality` (`flow.rs:1053-1056`). A comment correction whose `quality` reports `stub_detected` therefore reaches `after_commit(Quality)` with `origin` `None` and starts a review round (`flow.rs:1105-1115`). The integration test `comments_corrected_through_a_stub_repair_and_a_review_round_are_answered` (`crates/cli/tests/issue_flow_int.rs:2260`) currently pins that round.
2. `origin = TestReview` survives task advance, and `review_scope` treats any test review under it as a recheck (`handoff.rs:576-582`). The next task's first test review receives the previous task's `previous_review` and `change_to_review`.
3. `filing` sets `task` only for `implement` (`flow.rs:550-553`), so the `test-review` item has no `taskFile`.
4. `changedFiles` is the diff of the last two trees, while `diffBase` is `last_commit` (`handoff.rs:551-573, 706-722`). Nothing ties the two.
5. The dashboard's "Next" line is fixed per activity (`vocabulary.ts:423-445`).

### Requirements

#### Functional Requirements

From the requirements record (`desired-future`):

- FR1: A comment correction whose `quality` reports `stub_detected` pushes and answers the comments once `quality` passes, instead of starting a new review round.
- FR2: After a task's test-review correction passes, the next task's first test review receives no `previous_review` or `change_to_review` from the earlier task.
- FR3: The test-review item of a planned task carries its `taskFile`.
- FR4: The issue page's Next line reflects what actually follows.
- FR5: Every path in the test-review item's `changedFiles` differs from `diffBase`, whatever commits the flow made since the task or correction began.
- FR6: `changedFiles` keeps its meaning: the files the latest task or correction changed.
- FR7: `factory/steps/test-review.md` tells the reviewer what `diffBase` is in its new form.
- FR8 (Expected): each kind of correction's continuation is decided in one place; the flow, handoffs and dashboard read it.

#### Non-Functional Requirements

- **Compatibility of saved state**: an issue saved by an earlier version keeps its outer correction and finishes its continuation - Source: repository practice (Applicable Standards) and the requirements record's cost evidence ("In-flight issues saved by earlier versions must keep being read").

## Acceptance Criteria (AC) - EARS Format

### Nested corrections keep the outer correction

- [ ] **AC1** **When** a pull request comment correction's `quality` reports `stub_detected` and the redone `implement` asks for a test review, the system shall file `test-review` for that task. If that review raises findings, the executor's `review-response` shall receive them, and the next `test-review` shall receive `prior_feedback`. Once the test review passes and `quality` passes, the system shall push and answer the comments without filing `code-review` or `security-review` for that correction (FR1; GH-52 § Problem, the nested route; GH-44 § Expected: "The test review after each implementation task in Build keeps its current behavior").
- [ ] **AC2** **When** a planned task's test review raised findings that were corrected and then passed, and the next task's `implement` asks for a test review, the system shall file that test review without `previous_review`, `change_to_review` or `prior_feedback` (FR2).
- [ ] **AC3** **While** a test review rechecks the correction of its own findings, the system shall give it `prior_feedback`, or `previous_review` and `change_to_review` for a clear previous result, as before (preserved, `handoff.rs:576-617`).
- [ ] **AC4** **If** a round correction's Design Doc correction passes its design review and design sync, **then** the system shall continue to the round's rechecks after the design-sync commit. This holds also when the lead sends a blocked design review or design sync back to the Design Doc's author (`Redesign`), and for a record saved by an earlier version (preserved `After::ReturnToRound` behavior, `flow.rs:1091-1094, 1211-1230, 1524-1536`). **If**, outside a round, a design review's applied findings clear and the design sync that follows is blocked and sent back to the author (`Redesign`), **then** the system shall commit the Design Doc after the author's judgment without filing the design review again (preserved `redirect_author` keep, `flow.rs:1524-1530`).
- [ ] **AC10** **When** a `review-response` for pull request comments applies a comment and reports `requiresTestReview: true`, the system shall run `quality` next, with no `test-review` filed (preserved, GH-44 § Done when).

### Test review inputs

- [ ] **AC5** **When** a planned task's `test-review` item is filed, the system shall name that task's file as `taskFile` (FR3). **When** an issue saved by an earlier version files again a test review of a whole correction (`after: {"correction": …}`), the system shall name no `taskFile`.
- [ ] **AC6** (ubiquitous) A `test-review` item's `diffBase` shall be the worktree's tree when the latest `implement` or `review-response` started, after what was pushed to the branch was merged in, and `diffHead` its tree when that step ended. `changedFiles` shall list exactly the paths that differ between them (FR5, FR6). A path changed in the worktree before that executor started is not listed. This includes a path merged from the branch between a blocked attempt and its retry, by the lead's `exception` launch or by the retried launch, while the edits of both attempts stay listed. A path the step added or deleted is listed, and `git diff --quiet <diffBase> <diffHead> -- <path>` exits 1 for it.

### Next line

- [ ] **AC7** **While** the last task's test review runs and no redone task remains, the issue detail's `now.next` and the issue page shall name the quality checks only. **While** a redone task runs and another redone task remains, they shall name the remaining tasks first (FR4; GH-44 observation).
- [ ] **AC8** **While** a pull request comment correction's `quality`, `implement` or `test-review` runs, the Next line shall name the push and the replies to the comments, not the code and security review. **While** the executor judges findings (`review-response`), the Next line shall name what follows if a correction is applied and what follows otherwise. **While** the Design Doc's author corrects an open design review or design sync inside a review round, the Next line shall name the round's rechecks, after the design's checks when the design review is open (FR4).

### Saved state

- [ ] **AC9** **When** the daemon reads an issue record saved with `origin` `round` or `pr-comments`, or with `after` `return-to-round` or `{"correction": …}`, the system shall continue that correction as the saved version would have (NFR). **When** a `test-review` item filed by an earlier version, without `diffHead`, is launched after the upgrade, the step file shall have the reviewer compare `diffBase` with the worktree, as that version did, and run its basis check (NFR).

## Existing Codebase Analysis

### Implementation Path Mapping
| Type | Path | Description |
|------|------|-------------|
| Existing | `crates/core/src/flow.rs` | `IssueState`, `Origin`, `After`, the transitions that set and read them, `filing` |
| Existing | `crates/core/src/phase.rs` | `phase_of`, `correction_phase`, `activity`; gains `following` |
| Existing | `crates/daemon/src/handoff.rs` | `kept_under`, `corrected_checks`, `handed`, `executor_applied`, `author_findings`, `executor`, `test_review`, `review_scope`, `changed_files` |
| Existing | `crates/daemon/src/issues.rs` | `origin` reads at :440, :610, :751, :860, :870; `IssueRecord` gains `change`; `after_done` and `planned` |
| Existing | `crates/daemon/src/lib.rs` | `launch` opens a change after `place_for`; `bring_in_pushed` merges through `merged_into` |
| Existing | `crates/daemon/src/pulls.rs` | `push` merges through `merged_into` |
| Existing | `crates/daemon/src/history.rs` | `judging` (:711-714), tests at :949-961 |
| Existing | `crates/daemon/src/overview.rs` | `Position` gains `next` |
| Existing | `dashboard/src/lib/vocabulary.ts`, `dashboard/src/app/issue/IssueView.tsx`, `IssueView.stories.tsx` | Next line reads `now.next` |
| Existing | `dashboard/src/api/types.gen.ts` | Regenerated |
| Existing | `factory/steps/test-review.md` | `diffBase` and `diffHead` inputs, Step 1 check, the Quality Checklist's blocking condition |
| Existing | `docs/design/factory-core-design.md` | Steps rows `implement`, `test-review`, `quality`; § The review round's opening sentence; § Phases sentence naming `origin` |
| Existing | `docs/design/owner-interface-design.md` | `Position` contract in `GET /overview` row, reused by `GET /issues/{key}` `now` |
| Existing | `docs/design/events-and-error-notices-design.md` | § Event details, Review round: the count names `Origin::Round` |
| Existing | `crates/cli/tests/issue_flow_int.rs` | AC1, AC2, AC5, AC6, AC7 tests |

### Integration Points
- **Integration Target**: the daemon's issue records (`IssueRecord.flow`, `IssueRecord.change`), read by `records::issue`.
- **Invocation Method**: serde deserialization of saved JSON; every reader then uses the `IssueState` accessors and route functions below.

### Code Inspection Evidence

| File/Function | Relevance |
|---------------|-----------|
| `flow.rs:executed`, `test_review_follows` (:1004-1028) | Where a test review is started; only a round correction suppresses it |
| `flow.rs:next_task_or_redo` (:1041-1058) | Clears origin before quality; the clearing is removed, the choice becomes `task_route` |
| `flow.rs:after_commit` (:1082-1122) | What follows the quality and design-sync commits; the quality choice becomes `quality_route` |
| `flow.rs:checked`, `check_cleared`, `corrected_by_author`, `corrected_by_executor`, `redirect_author` (:1127-1263, :1524-1536) | The overwrite, and the readers of the inner correction; the judgment choices become `judgment_route` |
| `phase.rs:phase_of`, `correction_phase` (:193-226) | Phase derived from origin and after |
| `handoff.rs:test_review`, `review_scope`, `changed_files`, `executor` (:477-514, :551-617, :706-722) | Test-review inputs; quality's `correction_findings` |
| `issues.rs:after_done`, `planned`, `tree_of` (:966-1037, :1070-1091) | Tree recording reused for the change's end; `tree_of` for its start |
| `lib.rs:place_for`, `bring_in_pushed`, `launch` (:625-690) | Where the worktree is made ready for every agent launch; an executor's change is opened after it |
| `pulls.rs:merge_from_origin` (:921-944), its issue callers `bring_in_pushed` and `push` (`bring_in`, :187-193, :287-312) | The only way pushed commits enter an issue worktree; an open change's start is moved here |
| `history.rs:issue_detail` (read by `issue_flow_int.rs:517`) | The issue-detail API boundary that carries `now.next` |
| `scripts.rs:designed` (`crates/cli/tests/support/scripts.rs:84`) | Planned-task fake-agent script for AC2/AC5 |

### Fact Disposition Table

| Fact ID | Focus Area | Disposition | Rationale | Evidence |
|---------|------------|-------------|-----------|----------|
| crates/core/src/flow.rs:IssueState.origin | Single correction slot read by every decision point | transform | `origin` becomes `correction: Option<Correction>` (`Round` \| `PrComments`): the outer correction only. The inner correction is the existing `check`. Readers use `IssueState::corrected()` (innermost) or `correction` (outer), per Selected Design item 2. | crates/core/src/flow.rs:323, 1027, 1054, 1105, 1134, 1216, 1238, 1525, 1581; crates/core/src/phase.rs:196, 220; crates/daemon/src/handoff.rs:35-40, 224, 261, 287, 381, 507, 579; crates/daemon/src/issues.rs:440, 610, 751, 860, 870; crates/daemon/src/history.rs:712 |
| crates/core/src/flow.rs:checked | Inner correction overwrites outer | transform | Findings no longer touch the outer correction; `check` already names the inner one. | crates/core/src/flow.rs:1132-1138 |
| crates/core/src/flow.rs:next_task_or_redo | Origin cleared before quality, not between tasks | transform | Neither clearing remains. The outer correction lasts until its own continuation ends it (push, replies and gate, or the round's convergence). The inner check is cleared by `check_cleared`, as today. The choice between the next task, the next redo and `quality` becomes `task_route`, which `next_task_or_redo` applies. Existing tests `stubs_send_their_owning_tasks_back_and_the_quality_run_follows_them` and `stubs_after_a_round_correction_are_completed_in_review_and_the_rechecks_follow` stay green unchanged. | crates/core/src/flow.rs:1041-1058 |
| crates/core/src/flow.rs:test_review_follows | Whether a correction's redone task gets a test review | preserve | Only a round correction suppresses the test review, read from `correction` instead of `origin`. Its reviews recheck the redone work, as the merged `quality` row states. A comment correction's redone task asks for its test review like any implementation task: GH-44 took the test review only out of the direct comment `review-response` route, and kept "the test review after each implementation task". That test review's correction is the inner `check` while the comment correction stays in `correction`. | crates/core/src/flow.rs:1004-1028; docs/design/factory-core-design.md:406, 410; GH-44 § Expected, § Done when; GH-52 § Problem |
| crates/core/src/flow.rs:filing | test-review has no task | transform | `filing` sets `task` for `TestReview` from `IssueState::test_review_task()`: the current task when no outer correction is under way or while redone tasks run. It is none for a test review of a whole correction, which only an earlier version filed. `calls_for` refiling inherits it. A change without task files still has none. | crates/core/src/flow.rs:550-553; crates/daemon/src/handoff.rs:563-569; flow.rs:253-263 (`After::Correction`) |
| crates/daemon/src/handoff.rs:test_review | changedFiles and diffBase derived independently | transform | One recorded change gives all three entries: `diffBase` = its start tree, `diffHead` = its end tree, `changedFiles` = `diff-tree` of the two. Its start is recorded when the executor's launch is placed, and moved past what a merge into the worktree brings in while it is open. | crates/daemon/src/handoff.rs:551-573, 706-722; crates/daemon/src/issues.rs:966-990, 1070-1091; crates/daemon/src/lib.rs:625-651; crates/daemon/src/pulls.rs:187-193, 921-944 |
| crates/daemon/src/handoff.rs:review_scope | Recheck detection for test-review | transform | A test review is a recheck when its filing's `round` > 1 (`review_round`, reset to 1 by `check()` for each task's first test review). | crates/daemon/src/handoff.rs:576-582 |
| dashboard/src/lib/vocabulary.ts:NEXT | 'Next' line keyed by activity alone | transform | Core computes `Following` from the same route functions the transitions apply, and `Position.next` carries it. The dashboard words the codes, and keeps its activity table only for activities whose continuation does not depend on the state. | dashboard/src/lib/vocabulary.ts:423-450; dashboard/src/app/issue/IssueView.tsx:200; crates/daemon/src/overview.rs:191-206, 566-570 |
| crates/core/src/flow.rs:IssueState (serde) | Saved state read across upgrades | transform | Records saved with `origin`/`after` are mapped on read (Migration Strategy). Written records carry `correction` only. | crates/core/src/flow.rs:253-263 (After::Correction retained), 284-285 (Round.author_follows serde default), 1173 (DesignVerify retained), 1241-1245 (pre-upgrade executor edits) |
| crates/core/src/phase.rs:phase_of | Phase of steps inside a nested correction | transform | An outer correction gives every step run inside it that correction's phase: Round → Review, PrComments → Merge. A comment correction's redone `implement` and its `test-review` now show Merge. Otherwise the phase is the step's own phase, then the inner check's phase, then Build. | crates/core/src/phase.rs:193-226; docs/design/factory-core-design.md:304 |

## Design

### Change Impact Map

```yaml
Change Target: the representation and readers of the correction under way
Direct Impact:
  - crates/core/src/flow.rs: IssueState fields (origin, after → correction), Origin/After removed, Correction added, route functions task_route / quality_route / judgment_route, transitions listed in Selected Design
  - crates/core/src/phase.rs: phase_of, correction_phase; new Following, Upcoming + IssueState::following
  - crates/daemon/src/handoff.rs, issues.rs, history.rs: origin reads → corrected()/correction
  - crates/daemon/src/issues.rs, lib.rs: IssueRecord.change opened at the executor's launch and closed at its end
  - crates/daemon/src/issues.rs, lib.rs, pulls.rs: merges into an issue worktree (bring_in_pushed, push) go through merged_into, which moves an open change's start
  - crates/daemon/src/handoff.rs: test_review + changed_files → the recorded change
  - crates/daemon/src/overview.rs: Position.next
  - dashboard: vocabulary.ts, IssueView.tsx, stories, types.gen.ts
  - factory/steps/test-review.md: diffBase/diffHead inputs, Step 1 check, Quality Checklist
Indirect Impact:
  - Saved issue records: new fields `flow.correction` and `change`; legacy `origin`/`after` read once
  - Launch start lines record a comment correction's redone implement and its test review in Merge instead of Build
No Ripple Effect:
  - The review round's own rechecks, rerun-check, author handoff and kept-finding limits
  - Document checks' review_scope (previous output at any round), unchanged
  - Sprint flow, gate, CI and push handling
  - `IssueRecord.trees` and its readers (`task_files`, `stubs`)
  - Request texts written for agents (wording unchanged; only the condition that selects the comment text reads the new accessor)
```

### Interface Change Matrix

| Existing | New | Conversion Required | Compatibility Method |
|----------|-----|--------------------|--------------------|
| `IssueState.origin: Option<Origin>` | `IssueState.correction: Option<Correction>` | Yes | Legacy read mapping (Migration Strategy) |
| `IssueState.after: Option<After>` | removed (derived from `correction` + `check`) | Yes | Legacy read mapping |
| `enum Origin { Document, TestReview, Round, PrComments }` | `enum Correction { Round, PrComments }` and `enum Corrected { Check(Check), Round, PrComments }` (not serialized) | Yes | Callers updated in the same change |
| `IssueState::correcting() -> bool` | unchanged signature: `check.is_some() \|\| correction.is_some()` | No | — |
| `filing(..).task` for `implement` | for `implement`, and for `test-review` from `test_review_task()` | No | Additive |
| — | `IssueRecord.change: Option<Change { before: String, after: Option<String> }>` | No | Additive, `serde(default)`; absent in older records (fallback, Selected Design item 6) |
| `Position { …, task, … }` | `Position { …, task, next: Option<Following>, … }` | No | Additive field; TS regenerated |
| `test-review` input `diffBase`: last commit or `origin/<base>` | the tree the change started from; new input `diffHead`: the tree it ended at | Yes | `factory/steps/test-review.md` updated; an item without `diffHead` (filed before the upgrade, or with nothing recorded) is compared with the worktree, as before |

### Architecture Overview

Core (`crates/core`) owns the decision; the daemon and dashboard read it.

- `IssueState.correction` is the outer correction. It is set by `start_round` (Round) and `judge_comments` (PrComments). It is cleared only where that correction ends: `round_converged`, the comment correction's push in `after_commit(Quality)` or its no-applied path to replies and the gate, and `redirect_author` when it abandons the correction.
- `IssueState.check` is the inner correction, as it is today: the check under way, whose findings its receiver corrects and which then rechecks.
- Three pure route functions in `flow.rs` decide each continuation from the state: `task_route` (the next task, the next redone task, or `quality`), `quality_route` (what follows the quality commit, by `correction`) and `judgment_route` (what follows a receiver's judgment, applied or not). The transitions apply them, and `following()` maps them to codes. Each decision therefore exists once.
- `IssueState::corrected()` answers "whose findings the step under way receives": the open check's when `check` is set, otherwise the outer correction's. Every handoff reader that needs the findings' source uses it.
- The daemon records each executor change's two endpoints in `IssueRecord.change`. The test-review handoff reads only that record.

### Data Flow

```
judge_comments ─► correction = PrComments ─► review-response (corrected() = PrComments: comment findings, comment request text)
   └► quality ── stub_detected ─► redo implement(i) (redoing) ─► requiresTestReview ─► check(TestReview): task = i
         └► findings ─► review-response (corrected() = Check(TestReview), kept under test-review-judgment)
               └► recheck (round 2, prior_feedback) ─► clear ─► task_route ─► quality
         └► quality pass ─► commit(Quality) ─► quality_route: correction PrComments ─► Push; correction = None ─► replies (owe_replies recorded after the comments' review-response)

implement(task i, requiresTestReview, correction None) ─► check(TestReview): review_round = 1, filing.task = i
   └► findings ─► review-response (corrected() = Check(TestReview): kept under test-review-judgment)
         └► recheck: review_round = 2 ─► review_scope: round > 1 ─► prior_feedback
               └► clear ─► check_cleared ─► task_route ─► implement(i+1) ─► check(TestReview): round 1 ─► no previous review

merged_into(key, merge) (bring_in_pushed for every agent launch, push) ─► open change ─► pre = tree_of, merge, post = tree_of ─► before = moved_start(before, pre, post) when pre ≠ post
launch(implement | review-response) ─► place_for ─► change = { before: tree_of, after: None } when no change is open
after_done(implement | review-response) ─► change.after = tree_of; trees.push(tree) as today
test_review handoff ─► change ─► diffBase = before, diffHead = after, changedFiles = diff-tree(before, after)
```

### Integration Points List

| Integration Point | Location | Old Implementation | New Implementation | Switching Method | Verification Method |
|-------------------|----------|-------------------|-------------------|------------------|-------------------|
| Saved state read | `IssueState` deserialization | derived serde of `origin`, `after` | serde reads legacy fields and maps them to `correction` | in-place replacement | unit test on saved JSON (AC9, AC4, AC5) |
| Executor launch | `lib.rs:launch` | no tree at start | `change.before` recorded after `place_for` | additive | `issue_flow_int.rs` (AC6) |
| Merge into an issue worktree | `lib.rs:bring_in_pushed`, `pulls.rs:push` | `merge_from_origin` alone | `merged_into(key, …)` around it | in-place | `issue_flow_int.rs` (AC6, merge before a retry) |
| Test-review handoff | `handoff.rs:test_review` | `last_commit` + last-two-trees diff | the recorded change | in-place | `issue_flow_int.rs` (AC5, AC6) |
| Issue-detail and overview API | `overview.rs:position` | no `next` | `next: flow.following()` while in progress | additive field | `issue_flow_int.rs` via `history::issue_detail` + types check + story test (AC7, AC8) |

### Main Components

#### `Correction` and `IssueState` (crates/core/src/flow.rs)

- **Responsibility**: Records the outer correction and decides its continuation.
- **Interface**:
  - `pub enum Correction { Round, PrComments }` (serde kebab-case: `round`, `pr-comments`, the same strings `Origin` wrote).
  - `Correction::phase(self) -> Phase`: Round → Review, PrComments → Merge.
  - `pub enum Corrected { Check(Check), Round, PrComments }` and `IssueState::corrected(&self) -> Option<Corrected>`: `check` first, then `correction`.
  - `IssueState.correction: Option<Correction>`, documented as: "The correction raised after the build, kept while a check inside it corrects its own findings; it decides what follows its quality run."
  - `IssueState::test_review_task(&self) -> Option<usize>`: `self.task` when `tasks` has it and (`correction.is_none()` or `redoing`), else `None`.
- **Dependencies**: `Check`, `Phase`.

#### Route functions (crates/core/src/flow.rs)

- **Responsibility**: Each continuation decision, once, for both the transitions and `following()`.
- **Interface** (pure, `&IssueState`):
  - `task_route(s) -> TaskRoute { Implement(usize), Quality }`: the last `redo` entry if any; else `task + 1` when `!redoing` and it exists; else `Quality`. `next_task_or_redo` applies it (pops `redo`, resets `redoing` and `task`, as today).
  - `quality_route(s) -> QualityRoute { Design, Rechecks, Push, Round }`: `Round` with `round.author_follows` → `Design`; `Round` → `Rechecks`; `PrComments` → `Push`; `None` → `Round`. `after_commit(Quality)` applies it.
  - `judgment_route(s, applied: bool) -> JudgmentRoute { Recheck, Cleared, Quality, Converged, Answer }`: `check` set → `Cleared` for `DesignSync` or when nothing is applied, else `Recheck`. Otherwise by `correction`: applied → `Quality`, or `Recheck` of the Design Doc's checks for the round's author; nothing applied → `Converged` (Round) or `Answer` (PrComments). `after_judgment`, `corrected_by_executor` and `corrected_by_author` apply it. The pre-upgrade author-edit refile in `corrected_by_executor` stays ahead of it.
- **Dependencies**: `IssueState`.

#### `following` (crates/core/src/phase.rs)

- **Responsibility**: What follows the running step, as codes the dashboard words.
- **Interface**: `pub enum Upcoming` and `pub struct Following { then: Upcoming, if_applied: Option<Upcoming> }` (Serialize, TS, kebab-case values, camelCase fields), and `IssueState::following(&self) -> Option<Following>` (Selected Design item 7).
- **Dependencies**: the route functions, `stage`, `refused`.

#### Recorded change (crates/daemon)

- **Responsibility**: Gives `test-review` one description of the change it reviews.
- **Interface**:
  - `IssueRecord.change: Option<Change>`, `Change { before: String, after: Option<String> }` (`serde(default)`).
  - In `lib.rs:launch`, for an `implement` or `review-response` item of an issue: when `change` is `None` or has its `after`, set `change = Change { before: tree_of(key), after: None }` after `place_for`. An open change (no `after`: a retried or refiled executor) is kept as it is.
  - `issues.rs:merged_into(key, merge: impl FnOnce() -> Result<bool, DaemonError>) -> Result<bool, DaemonError>` runs every merge into an issue's worktree: `bring_in_pushed`'s `merge_from_origin` call, for every agent launch on the issue (the lead's `exception` included), and `push`'s `bring_in` call. When the issue's `change` is open, it records `pre = tree_of(key)`, runs `merge`, and records `post = tree_of(key)`. When both are recorded and differ, `before` becomes `moved_start(before, pre, post)`, and the record is saved. Otherwise, and when no change is open, it only runs `merge`. It returns `merge`'s result. The agreements branch's `push_agreements` (`sprint.rs:732`) merges into no issue worktree and keeps calling `bring_in` directly.
  - `issues.rs:moved_start(before, pre, post) -> Option<String>`: in a temporary index (as `tree_of` uses), `git read-tree <post>`. Then the old mode and blob of each entry of `git diff-tree -r --no-renames <before> <pre>` go to `git update-index --index-info` (mode 0 removes the path), and `git write-tree` gives the result. On a Git failure it returns `None` and `before` is kept.
  - `after_done` (Implement, ReviewResponse) sets `change.after` to the tree it already records for `trees`. `planned` clears `change` when it resets `trees`.
  - `handoff.rs:recorded_change(record, fallback_base) -> Option<(String, String)>` returns `(before, after)` and replaces `changed_files`.
- **Dependencies**: `tree_of`, `git diff-tree`.

### Selected Design

1. **State.** In `IssueState`, `origin` and `after` are replaced by `correction: Option<Correction>`. `check`, `attempts`, `review_round`, `redo`, `redoing` and `task` stay as they are. `Origin` and `After` are removed; their legacy values are read through the Migration Strategy.

2. **Readers.** Each reader maps to one accessor:

| Reader | Today | New |
|---|---|---|
| `flow.rs:test_review_follows` | `origin != Round` | `correction != Some(Round)` |
| `flow.rs:next_task_or_redo` | clears non-Round origin before quality | applies `task_route`; no clearing |
| `flow.rs:after_commit(Quality)` | match origin | applies `quality_route`: `Design` → `design`; `Rechecks` → `recheck_round`; `Push` → `correction = None`, Push; `Round` → `start_round` |
| `flow.rs:after_commit(DesignSync)` | `after == ReturnToRound` | `correction == Some(Round)` → `recheck_round` (first arm) |
| `flow.rs:checked` | sets origin on Findings | does not touch `correction` |
| `flow.rs:check_cleared(TestReview)` | `after.take()` | `next_task_or_redo` (a legacy correction-wide review has `task` at the last task and `redoing` false, so it runs `quality` under its correction) |
| `flow.rs:corrected_by_author` | `origin != Round` → `after_judgment` | applies `judgment_route`: `check.is_some()` → as `after_judgment`; else `Round`: none applied → `recheck_round`, applied → `check(DesignReview)` |
| `flow.rs:return_to_round` | sets origin Round | removed; callers call `recheck_round` |
| `flow.rs:corrected_by_executor` | Round/PrComments by origin | applies `judgment_route`: `check.is_some()` → `after_judgment` first; else by `correction`, as today |
| `flow.rs:round_converged`, PrComments none-applied | clear origin | clear `correction` |
| `flow.rs:start_round`, `judge_comments` | set origin | set `correction` |
| `flow.rs:redirect_author` | keeps when `origin == Document` and the check is the target's | keeps `check` and `correction` when the check is the target's and any of these holds: `correction == Some(Round)` (the round's Design Doc correction, whose continuation is the round's rechecks); the check is `DesignSync`, whose correction ends with the author's judgment and the commit (`after_judgment`), so a design review that already cleared does not run again; or its findings had reached the author (the failed step is the target's author step, or `review_round > 1`). Otherwise clears `check`, `correction` and `round`, as today |
| `phase.rs:phase_of` | origin/after | Quality keeps its `refused` rule. Then, if `correction` is set: `correction.phase()`. Else `own_phase()`, then the inner check's phase (`check.step().own_phase()`, TestReview → Build), else Build |
| `handoff.rs:kept_under` | `(ReviewResponse, TestReview)` | `(ReviewResponse, Some(Corrected::Check(Check::TestReview)))` |
| `handoff.rs:corrected_checks`, `handed`, `executor_applied`, `author_findings` | origin | `corrected()` |
| `handoff.rs:executor` quality `correction_findings` | `origin.is_some()` | `correction.is_some()`; at that point `check` is `None`, so `executor_applied` reads the comments' or the round's judgment under `review-response`. An inner test review's judgment is kept under `test-review-judgment` and does not replace it |
| `handoff.rs:review_scope` (test-review) | `origin != TestReview` → none | `filing.round <= Some(1)` → none |
| `issues.rs:440` (`record_review_round`) | origin Round | `correction == Some(Round)`: then `handed(&record).len()`, else zero. When the round hands findings to `review-response`, no `check` is open, so `handed` reads `corrected()` = `Round` and counts the entries the correction receives; a directly converged round has cleared `correction` |
| `issues.rs:610, 751, 870` | origin PrComments | `corrected() == Some(Corrected::PrComments)` (an inner test review's `review-response` neither gets the comment text nor owes replies) |
| `issues.rs:860` | `kept_under(origin, …)` | `kept_under(corrected(), …)` |
| `history.rs:judging` | `origin?` | `correcting()` gate, then `handed` |

3. **Comment correction (FR1).** Under `correction = PrComments`, `Fact::Stubs` sends the owning tasks back as today. A redone `implement` that asks for a test review gets one (`test_review_follows`). Its findings are the inner `check`'s: `review-response` judges them under `test-review-judgment`, and the test review rechecks with `prior_feedback`. When it clears, `next_task_or_redo` runs the next redone task or `quality` without clearing anything. `after_commit(Quality)` pushes. The replies were already recorded as owed after the comments' `review-response` (`issues.rs:870`), so they are posted after the push. A `review-response` for the comments themselves still goes straight to `quality` (AC10, `corrected_by_executor`).

4. **Test review task (FR3).** `filing` sets `task` for `Implement` as today, and for `TestReview` from `test_review_task()`. The handoff already turns `filing.task` into `taskFile` (`handoff.rs:563-569`). A test review of a whole correction, which only an earlier version filed (`After::Correction`, mapped to `correction` with `redoing` false), has no task. Its refiled item keeps its correction-wide basis.

5. **Test-review recheck (FR2, AC3).** `review_scope` returns early for `test-review` unless the filing's round is greater than 1. Everything after that line is unchanged.

6. **Recorded change (FR5, FR6).** `recorded_change` returns `(change.before, change.after)` when both are recorded. For a record without them (an executor launched before the upgrade), it falls back to today's pair: the last two `trees` entries, or `(fallback, last tree)` with one entry, where `fallback` is `last_commit` or `origin/<base>`. Both endpoints still come from that one pair, so the guarantee below holds; only which files are attributed to the change stays as before the upgrade. With nothing recorded, it returns `None`. `test_review` emits `diffBase = before`, `diffHead = after` and `changedFiles = diff-tree -r --name-only before after`. Every listed path differs between the two trees by construction. Recording the start after `place_for` puts what was merged from the branch, and what `quality` or other steps changed before the executor ran, inside `before`. While a change is open, every merge into the worktree goes through `merged_into`, whichever launch or push runs it. `moved_start` puts what that merge brought in inside `before`, and keeps each path the earlier attempt changed at its start version. This includes a merge by the lead's `exception` launch between a blocked attempt and its retry. Both attempts' edits are listed, and the merged paths are not. A path that both the earlier attempt and the merge changed is listed, because the change did touch it. An agent's own edits between the attempts (a lead's fix in the worktree, for example) belong to the correction and stay attributed to the change, as the fallback does.

7. **Next line (FR4).** `IssueState::following()` returns a value only for `Stage::Running { step }`. It names what follows once the running step's work holds (a check clear, an executor done with nothing left incomplete). For a judgment, where applying and declining are both valid, it names both branches. `task(s)` below maps `task_route`: `Implement(_)` → `remaining-tasks`; `Quality` → by `quality_route`: `Round` → `quality-checks`, `Rechecks` or `Design` → `quality-then-rechecks`, `Push` → `quality-then-push`.
   - `implement`, `test-review`: `then = task(s)`.
   - `quality`: `refused` at `Push` gives `push-again`. `refused` at another point except `Quality` gives `commit-again`. Otherwise by `quality_route`: `Round` → `reviews`; `Rechecks` → `rechecks`; `Design` → `design-then-rechecks`; `Push` → `push-and-replies`.
   - `review-response`: `if_applied` from `judgment_route(s, true)` and `then` from `judgment_route(s, false)`. `Recheck` of `TestReview` → `test-recheck`; `Cleared` of `TestReview` → `task(s)`; `Quality` → `quality-then-rechecks` (Round) or `quality-then-push` (PrComments); `Converged` → `push`; `Answer` → `replies-and-gate`.
   - `design` under `Round` with no open check: `if_applied = design-then-rechecks`, `then = rechecks`. `design` under `Round` correcting an open `DesignReview`: `then = design-then-rechecks` (applied, the design review checks again; not applied, `Cleared` goes to the design sync, so both branches reach the design's checks, then the round's rechecks). `design` under `Round` correcting an open `DesignSync`: `then = rechecks` (`judgment_route` gives `Cleared`, then the design-sync commit and `recheck_round`). `design-review` under `Round`: `design-then-rechecks`. `design-sync` under `Round`: `rechecks`.
   - Otherwise `None`: the dashboard's per-activity text applies.

   `Position.next = flow.following()` while in progress. `vocabulary.ts` gains `UPCOMING: Record<Upcoming, string>`. `nextInPhase(position)` returns `UPCOMING[then]` when `next` is set and has no `ifApplied`, or "`UPCOMING[ifApplied]`, if a correction is applied; otherwise `UPCOMING[then]`" when it has one. With no `next`, it returns `NEXT[activity]`. The rows `implementing`, `reviewing-tests`, `quality-checks` and `applying-findings` are removed from `NEXT`, because `following` always answers for them. Owner-facing wording, following `dashboard/CLAUDE.md`, for the implementer to keep (the line ends with a period):
   - `remaining-tasks`: "the remaining tasks, then the quality checks"
   - `quality-checks`: "the quality checks"
   - `reviews`: "the code review and security review"
   - `quality-then-rechecks`: "the quality checks, then the reviews check again"
   - `rechecks`: "the reviews check again"
   - `design-then-rechecks`: "the corrected design and its checks, then the reviews check again"
   - `quality-then-push`: "the quality checks, then the push and the replies to the comments"
   - `push-and-replies`: "the push and the replies to the comments"
   - `test-recheck`: "the test review checks the correction"
   - `push`: "the push"
   - `replies-and-gate`: "the replies to the comments and the merge checks"
   - `commit-again`: "the commit again"
   - `push-again`: "the push again"

8. **Step file (FR7).** In `factory/steps/test-review.md`:
   - § In the factory: `changedFiles` is every file the task or correction changed between `diffBase` and `diffHead`.
   - § Input Parameters: `diffBase`: "Git tree or revision the reviewed change started from"; new `diffHead` (optional): "Git tree of the worktree when the change ended; the reviewed change is `git diff <diffBase> <diffHead>`. When absent, the change ends at the worktree: `git diff <diffBase>`".
   - § Verification Process, Step 1: "Confirm every changed path differs between `diffBase` and `diffHead` (`git diff --quiet <diffBase> <diffHead> -- <path>` exits 1). A path absent from `diffHead` was deleted by the change. Read each changed test as `diffHead` holds it (`git show <diffHead>:<path>`). Without `diffHead`, confirm every changed path exists and differs from `diffBase`, and read it from the worktree." The basis selection that follows is unchanged. The sentence without `diffHead` is today's check, so an item filed before the upgrade, or one with nothing recorded, is reviewed as before (AC9).
   - The Quality Checklist's blocking condition "A changed test file or `diffBase` is unavailable" also names "`diffHead` when given".

Added design surface:

- **Addition**: `Correction` enum and `correction` field (replacing `origin`/`after`).
  - **Current evidence**: GH-52 Expected ("an outer correction kept while an inner one runs"), observations 1 and 2.
  - **Lower-surface insufficiency**: keeping the one-slot `origin` requires the overwrite or the inference this issue removes. A general stack adds depth no path reaches. `test_review_follows` and `judge_comments` bound nesting to an outer correction plus one check (`codebase_analysis` simplifications).
  - **Subtraction result**: without it, a comment correction loses its push after an inner correction (FR1) and the readers keep inferring (FR8).
- **Addition**: `Corrected` (non-serialized) and `IssueState::corrected()`.
  - **Current evidence**: about a dozen daemon readers need "whose findings this step receives" (Fact Disposition, first row).
  - **Lower-surface insufficiency**: if each reader combines `check` and `correction` itself, the decision is repeated at every reader (FR8).
  - **Subtraction result**: FR8 is unmet.
- **Addition**: route functions `task_route`, `quality_route`, `judgment_route`.
  - **Current evidence**: FR8. Design review finding I005: a separately written `following()` already disagreed with `next_task_or_redo` (pending redo work) and `after_judgment` (all findings declined).
  - **Lower-surface insufficiency**: matching the state again in `following()` repeats each decision in a second place, where it drifts.
  - **Subtraction result**: FR8 and AC7/AC8 are unmet on the states where the copies differ.
- **Addition**: `Following`, `Upcoming`, `IssueState::following()` and `Position.next`.
  - **Current evidence**: FR4, GH-52 Expected ("the dashboard … read that decision").
  - **Lower-surface insufficiency**: exposing raw facts makes the dashboard re-derive flow rules (`codebase_analysis` candidateDecisionPoints, option 2, rejected by the Expected). One code cannot state a judgment's two branches without one code per pair of continuations.
  - **Subtraction result**: AC7 and AC8 are unmet.
- **Addition**: `IssueRecord.change`, recorded at the executor's launch, and the `diffHead` input.
  - **Current evidence**: FR5, FR6. Assumed Behaviors: the worktree changes between recorded trees, a merge brings pushed commits into an open change, and a worktree comparison misses new files (findings I002, I003).
  - **Lower-surface insufficiency**: the previous recorded tree can predate a merge from the branch or a `quality` edit, which would be attributed to the executor (FR6). `git diff <diffBase>` against the worktree misses new tests and sees later branch updates (FR5). Reusing `trees` for the start would break its pairing for `task_files`.
  - **Subtraction result**: FR5 or FR6 is unmet.
- **Addition**: `moved_start`, and `merged_into` around the two merges into an issue worktree.
  - **Current evidence**: FR6; design review finding I002 (pushed commits merged into an open change, by the retried executor's launch or by the lead's `exception` launch before it).
  - **Lower-surface insufficiency**: keeping the first start attributes the merged paths to the change. Restarting at the retried launch drops the earlier attempt's edits. Comparing trees around one kind of launch misses a merge made by another launch or by a push, because the later merge skips what is already contained (`pulls.rs:930-934`). `merge_from_origin` itself has no issue key, and it also serves the agreements branch, so the wrapper sits at its two issue callers. Only a merge while a change is open pays the two extra `tree_of` calls.
  - **Subtraction result**: FR6 is unmet when a pushed commit is merged into an open change, as after a blocked executor on a branch with a pull request.
- **Addition**: `IssueState::test_review_task()`.
  - **Current evidence**: FR3; finding I006 (a legacy correction-wide test review would receive the last task's file).
  - **Lower-surface insufficiency**: setting `task` for every `TestReview` gives that legacy review an unrelated task file.
  - **Subtraction result**: FR3 or AC9 is unmet.
- **Addition**: legacy read mapping for `origin`/`after`.
  - **Current evidence**: NFR, repository practice.
  - **Lower-surface insufficiency**: dropping the fields loses an in-flight round's return after its Design Doc correction (`after: return-to-round` with `origin: document` would continue to test design and the work plan).
  - **Subtraction result**: AC9 is unmet.

### Data Contracts

#### Saved `IssueState`

```yaml
Contract: IssueState (serde JSON in the issue record)
Input:
  Type: JSON object; new field `correction`: null | "round" | "pr-comments"; legacy fields `origin`: null | "document" | "test-review" | "round" | "pr-comments" and `after`: null | "next-task" | "return-to-round" | {"correction": "round" | "pr-comments"}
  Preconditions: none; every field optional as before
  Validation: serde; an unknown variant fails the record read as today
Output:
  Type: IssueState with `correction`
  Guarantees: a written record carries `correction`, and neither `origin` nor `after`
  On Error: serde error, as today
Invariants:
  - correction is Some only between start_round/judge_comments and the end of that correction
  - check is Some only while a check, its correction, or its recheck is under way (unchanged)
```

#### `IssueRecord.change`

```yaml
Contract: IssueRecord.change (daemon issue record)
Type: null | {"before": "<tree id>", "after": null | "<tree id>"}
Written: before — at an implement or review-response launch, after place_for, when no change is open; moved by moved_start in merged_into when a merge into the worktree changed it while the change is open; after — in after_done of that step; cleared by planned
Invariants:
  - before is the worktree when the open change's first executor launch was placed, plus what every later merge into the worktree brought in while the change was open
  - after, when set, is the worktree when that executor's step ended
```

#### `test-review` item inputs

```yaml
Contract: Handoff::test_review entries
Output:
  Type: changedFiles (comma-separated paths, omitted when empty), diffBase (tree id, or a revision in the fallback), diffHead (tree id, omitted when nothing is recorded; the reviewer then compares diffBase with the worktree), taskFile (path, a planned task's review only), skeletonFiles
  Guarantees: changedFiles = diff-tree -r --name-only diffBase diffHead; for each path p in it, `git diff --quiet <diffBase> <diffHead> -- p` exits 1, whatever the worktree holds at review time
Invariants:
  - diffBase, diffHead and changedFiles come from one call of recorded_change
```

#### `Position.next`

```yaml
Contract: Position (GET /overview, GET /issues/{key} `now`)
Output:
  Type: next: null | {"then": Upcoming, "ifApplied": null | Upcoming}
  Upcoming: "remaining-tasks" | "quality-checks" | "reviews" | "quality-then-rechecks" | "rechecks" | "design-then-rechecks" | "quality-then-push" | "push-and-replies" | "test-recheck" | "push" | "replies-and-gate" | "commit-again" | "push-again"
  Guarantees: non-null whenever activity is implementing, reviewing-tests, quality-checks or applying-findings, and for design, design-review and design-sync steps under a round correction; null outside in-progress
```

### Field Propagation Map

| Field | Boundary | Status | Serialized Format | Consumer Parse Rule | Detail |
|-------|----------|--------|-------------------|---------------------|--------|
| `origin` | saved record → `IssueState` | transformed | `"round"`/`"pr-comments"`/`"document"`/`"test-review"` | `round`/`pr-comments` → `correction`; others dropped (the inner correction is in `check`) | read only |
| `after` | saved record → `IssueState` | transformed | `"next-task"`, `"return-to-round"`, `{"correction":"round"\|"pr-comments"}` | `return-to-round` → `correction = round`; `{"correction":x}` → `correction = x`; `next-task` dropped | read only; takes precedence over `origin` |
| `correction` | `IssueState` ↔ saved record | new | `null`/`"round"`/`"pr-comments"` | serde | written and read |
| `change` | daemon ↔ issue record | new | `{"before": "<40-hex>", "after": "<40-hex>" \| null}` or absent | serde, default `None` | written at launch, in merged_into and in after_done |
| `diffBase` | daemon → test-review item | transformed | `diffBase: <40-hex tree id>` (commit sha / `origin/<base>` only in the fallback) | the reviewer passes it to `git diff` | `factory/steps/test-review.md` |
| `diffHead` | daemon → test-review item | new | `diffHead: <40-hex tree id>` | the reviewer passes it to `git diff` and `git show` | `factory/steps/test-review.md` |
| `taskFile` | daemon → test-review item | preserved (now filled) | `taskFile: <absolute task path>` | read as a path | from `filing.task` (`test_review_task()`) |
| `next` | daemon → dashboard | new | `{"then": "<kebab>", "ifApplied": "<kebab>" \| null}` or null | `types.gen.ts`; `UPCOMING[…]` | `Position` |

### State Transitions and Invariants

```yaml
State Definition:
  - Initial State: correction None, check None
  - Possible States: (correction, check) ∈ {None, Round, PrComments} × {None, Some(check)}; under Round the check can be the Design Doc's checks, and test-review only from a legacy record; under PrComments the check is a redone task's test-review, or a legacy correction-wide one

State Transitions:
  None,None → quality commit → Round,None (start_round)
  Round,None → review-response none applied → None,None (round_converged → Push)
  Round,None → quality commit with author_follows → Round,None (design) → applied → Round,DesignReview → … → Round,DesignSync clear → commit → Round,None (recheck_round)
  Round,DesignReview|DesignSync → blocked → exception Redesign → Round,(same check) (design, then its checks) → commit → Round,None (recheck_round)
  None,DesignReview findings → design → recheck clear → None,DesignSync (round 1) → blocked → exception Redesign → None,DesignSync (design) → judgment → commit (no design review again)
  merge stage → comments → PrComments,None → review-response none applied → None,None (AnswerComments, RunGate)
  PrComments,None → review-response applied → quality (no test-review)
  PrComments,None → quality stub_detected → PrComments,None (implement redo) → requiresTestReview → PrComments,TestReview → [findings → review-response → recheck]* → clear → PrComments,None → quality pass → commit → None,None (Push)
  None,None → implement requiresTestReview → None,TestReview (round 1) → findings → review-response → recheck (round 2) → clear → None,None → next task

System Invariants:
  - A check's findings never change `correction`
  - `correction` ends only where its continuation ends (push, replies and gate, round convergence, or redirect_author abandoning it)
```

### Error Handling

| Error Category | Example | Detection | Recovery Strategy | User Impact |
|---------------|---------|-----------|-------------------|-------------|
| Business logic | Nothing recorded at a test review (`change` and `trees` empty) | `recorded_change` returns `None` | `changedFiles` and `diffHead` omitted, `diffBase` = fallback (as today); the reviewer blocks per its step file and the lead's `exception` handles it | Same as today |
| Business logic | `tree_of` fails at launch | `change` stays unopened | the test review falls back as for an older record | Attribution as before this change |
| Data | Saved record with an unknown `origin`/`after` variant | serde error on read | Propagated, as today for any unreadable record | None expected: every variant earlier versions wrote is mapped |

## Implementation Plan

### Implementation Approach

**Selected Approach**: Hybrid. A foundation step first: the state representation, its legacy read, the route functions and the core readers, proven by flow unit tests. The daemon, test-review and dashboard slices follow, each verified at its own boundary.
**Selection Reason**: every daemon and dashboard reader depends on the new `IssueState` fields and accessors. The riskiest premise is that the narrowed state keeps all existing flow transitions, including the round's Design Doc correction and the comment correction's nested test review. That premise is entirely provable in core unit tests before any daemon change.

### Technical Dependencies and Implementation Order

1. **Core state, routes and transitions** (`flow.rs`, `phase.rs` phase part)
   - Technical Reason: every other slice reads these fields.
   - Prerequisites / Dependent Elements: none / all below.
   - Verification: existing flow and phase tests pass. New unit tests:
     - AC1 at flow level: comments → `stub_detected` → implement with `requiresTestReview: true` → `TestReview` filed → findings → `review-response` → recheck → clear → quality → commit → `Push`, with no `CodeReview` filed.
     - AC10: a comment `review-response` with `requiresTestReview: true` → `Quality`.
     - AC4 from saved JSON: `origin: document`, `after: return-to-round`, `check: design-sync`, `review_round: 1`, running `design-sync` → blocked → exception `Redesign` → design applied → design-sync commit → the round's rechecks filed.
     - AC4 outside a round: design review findings → design applied → recheck clear → design sync blocked → exception `Redesign` → design → design-sync commit, with no `DesignReview` filed after the redirect.
     - The legacy read table (AC9).
     - `filing(TestReview).task` for a legacy `after: {"correction": "pr-comments"}` record (None) and for a comment correction's redone task (Some) (AC5).
     - The phase of a comment correction's redone `implement` and its `test-review` = Merge.
2. **Daemon readers** (`handoff.rs`, `issues.rs`, `history.rs`)
   - Technical Reason: these readers must compile against the removed `Origin`.
   - Verification: existing daemon unit and integration tests pass. `comments_corrected_through_a_stub_repair_and_a_review_round_are_answered` is renamed to `comments_corrected_through_a_stub_repair_are_pushed_and_answered_without_a_review_round`. Its script keeps the `code-review` and `security-review` entries. Its redone `implement` changes a test file and asks for a test review. `test-review` first returns a finding, `review-response` applies it, and the recheck passes. The test asserts:
     - a `test-review` item is filed for the redone task;
     - the recheck item has a `prior_feedback` line;
     - no `code-review` item is filed after `comments-accepted`;
     - its existing reply assertion (AC1).
3. **Test-review inputs** (`filing`, `review_scope`, `IssueRecord.change`, `launch`, `merged_into`, `recorded_change`, `test-review.md`)
   - Verification: AC2, AC5, AC6 and AC7 in `issue_flow_int.rs`.
   - A new test uses `designed("ACME-…", &[], &[("1",""),("2","1")])`. Each `implement` asks for a test review and changes a test file. Task 1's first test review returns findings, then passes. The test asserts that task 2's first `test-review` item has a `taskFile` line naming task 2's file, and no `previous_review:`, `change_to_review:` or `prior_feedback:` line (AC2, AC5).
   - In the same run, `quality` then reports `stub_detected` for files of both tasks. While the first redone task runs, `history::issue_detail(..).now.next` has `then = remaining-tasks`. While the last task's test review runs with no redo left, it has `then = quality-checks` (AC7).
   - `a_test_review_receives_the_tests_a_task_changed_whatever_their_naming` gains an unrelated change made before the executor starts: a `files` entry written by a step that runs before `implement`, or a commit pushed to the branch that `bring_in_pushed` merges. Its `tests/export_int.rs` is new and untracked in the worktree's index. The test asserts that:
     - `changedFiles` names `tests/export_int.rs` and not the unrelated path;
     - for each listed path, `git diff --quiet <diffBase> <diffHead> -- <path>` exits 1 (AC6).
   - A daemon unit test of `moved_start` in a temporary repository repeats the I002 probe (Assumed Behaviors). The first attempt edits, adds and deletes a path, a merge brings in an unrelated path, and the second attempt edits another path. `diff-tree` of the moved start and the end lists the four attempt paths and not the merged one (AC6, retry).
   - An integration test reaches the I002 route through the wiring. It starts from the AC1 run's comment correction, whose `quality` reports `stub_detected`. The redone `implement` edits one path and returns `blocked`. A commit that adds an unrelated path is then pushed to the issue branch on the harness's bare `origin`, before the lead's `exception` launch. That launch merges it (`bring_in_pushed`). The lead chooses `Retry`, and the retried `implement` edits another path and asks for a test review. The filed `test-review` item's `changedFiles` names both attempts' paths and not the pushed one, and `git diff --quiet <diffBase> <diffHead> -- <path>` exits 1 for each listed path (AC6, merge before a retry).
   - `factory/steps/test-review.md` states both comparisons: two trees when `diffHead` is given, the worktree when it is absent (AC9). This is agent instruction text, checked in the person's review of that change.
4. **Next line** (`phase.rs::following`, `overview.rs`, types, dashboard)
   - Verification: core unit tests for `following()`:
     - the last task's test-review → `quality-checks`;
     - a redone task with another redo pending under PrComments → `remaining-tasks`;
     - quality under PrComments → `push-and-replies`;
     - a comment `review-response` → `if_applied = quality-then-push`, `then = replies-and-gate`;
     - a test review's `review-response` → `if_applied = test-recheck`, `then = task(s)`;
     - `design` under Round with an open `DesignReview` → `design-then-rechecks`, and with an open `DesignSync` → `rechecks` (AC8).

     Then `types.gen.ts` is regenerated. An `IssueView.stories.tsx` story asserts "Next: the quality checks." for `activity: 'reviewing-tests'` with `next: {then: 'quality-checks', ifApplied: null}` (AC7). The existing story asserting the applying-findings text gains `next: {then: 'push', ifApplied: 'quality-then-rechecks'}` and asserts both branches (AC8).
5. **Documents**: the merged designs that state a decision this design changes are updated in the same change, so no merged design keeps `origin`'s single slot as its rule.

   `docs/design/factory-core-design.md`:
   - `implement` row: "`test-review` when `requiresTestReview` outside a correction of the review round".
   - `test-review` row: given the task file and the change's start and end trees.
   - `quality` row, `pass` and `verification_incomplete`: "the daemon commits, then by the correction under way: after the build's last task, the review round; after the round's correction, its rechecks (the Design Doc's correction and its checks first when the round's author follows); after the pull request comments' correction, push and the replies, with no new review round" (`quality_route`). `stub_detected`: after a round correction, in Review without `test-review`; after a comment correction, in Merge, with the redone task's `test-review` when it asks for one; either way the commit after the passing `quality` follows the destinations above. A reader following the comment correction's stub repair through this row reaches push and the replies, and no `code-review` or `security-review`.
   - § The review round, first sentence: "After the build's `quality` the daemon files every review of the round at once". The comments' `review-response` still goes straight to `quality` (its row, AC10) and is unchanged.
   - § Phases, the correction sentence becomes: "A step run inside a correction raised after the build (the issue state's `correction`) belongs to that correction's phase: the review round's to review, the pull request comments' to merge; this covers its redone `implement`, their `test-review` and its `review-response`, and the Design Doc's checks under a round. Otherwise a correction of a check (an author run again in update mode, `review-response`, the `quality` after it) belongs to the phase of the check whose findings it answers (`check`), and a step outside any correction to its own phase. A `quality` whose commit or push was refused belongs to the point it returns to; an `exception` keeps the phase it stopped in." So an ordinary task's test review and its `review-response` run in Build, the same steps nested under a comment correction in Merge, and the Design Doc's checks under a round in Review (`phase_of`, Selected Design item 2).

   `docs/design/owner-interface-design.md`, `GET /overview` row: `Position` gains `next: null | {then, ifApplied: null | then}`, the codes `Upcoming` lists, "what follows the running step; `ifApplied` when applying and declining a correction lead on differently; null outside in progress and where the activity alone decides (the dashboard's activity text applies)", contract in `crates/core/src/phase.rs` (`Following`) and `docs/design/correction-under-way-design.md` § Data Contracts, `Position.next`. `GET /issues/{key}` reuses the same `Position` for `now`, so both carry it.

   `docs/design/events-and-error-notices-design.md`, § Event details, Review round: "Where the round is handing findings to `ReviewResponse` (the saved state's `correction` is `Round`, with no `check` open), obtain the count from `handoff::handed`, which then returns the round's findings (`corrected()` = `Corrected::Round`): the same output keys and entries the correction receives." The rest of the paragraph stays: one per entry, zero for a round that converged directly (`round_converged` has cleared `correction` before the daemon handles `RecordReviewRound`), the same `review-round-ended {issue, round, findings}` fields, emitted once at the same boundary. This matches `issues.rs:440` in Selected Design item 2.

### Migration Strategy

`IssueState` reads saved JSON through a private saved form that accepts `correction` and the legacy `origin` and `after`. It is converted with `From`, so `IssueState` itself holds only valid fields. A field added to `IssueState` without the conversion fails to compile. The mapping, applied in order:

1. `after = {"correction": x}` → `correction = x`
2. `after = "return-to-round"` → `correction = Round`
3. `origin = "round" | "pr-comments"` → that correction
4. `correction` as saved, when present (records written by this version)
5. otherwise `None`

`after = "next-task"` and `origin = "document" | "test-review"` carry nothing the new state needs: the inner correction is `check`. A legacy correction-wide test review (`after = {"correction": x}`, `redoing` false) keeps its correction-wide basis through `test_review_task()`. A legacy round Design Doc correction (`after = "return-to-round"`) keeps its return to the round across `redirect_author`, because the mapped `correction` is `Round`. An in-flight issue from observation 1 whose `origin` was already overwritten cannot be recovered. It continues as it would have before this change: a review round, then push. A record without `change` gets the fallback pair (Selected Design item 6). A `test-review` item filed before the upgrade keeps its entries (no `diffHead`); the step file's comparison with the worktree reviews it as that version did (Selected Design item 8).

## Security Considerations

- **Authentication & Authorization**: N/A. No new entry point; `Position.next` is served by the existing overview endpoints.
- **Input Validation**: `diffBase`, `diffHead` and `changedFiles` are produced by the daemon from its own recorded tree ids, not from agent output. Saved state is the daemon's own file.
- **Sensitive Data Handling**: N/A. No secrets in the changed fields.

## Test Boundaries

### Mock Boundary Decisions

| Component/Dependency | Mock? | Rationale |
|---------------------|-------|-----------|
| Agents | Yes (fake agent scripts) | Existing integration harness (`crates/cli/tests/support`) |
| GitHub | Yes (`FakeGitHub`) | Existing harness |
| Git | No | The recorded change's guarantee is a Git property; tests run real Git in a temp repository |
| Flow / handoff | No | Boundaries under test |

### Data Layer Testing Strategy

- **Schema dependencies**: `IssueRecord.flow` and `IssueRecord.change` JSON (`crates/daemon/src/issues.rs:24-90`, `crates/core/src/flow.rs:IssueState`).
- **Test data approach**: inline JSON literals of records as earlier versions wrote them (one per mapping row).
- **Mock limitations acknowledged**: none. Serde is exercised directly.

### Integration Verification Points

- Comment correction with a stub repair and its nested test-review correction: push and replies without a review round (AC1).
- Planned two-task issue: test-review items' `taskFile` and recheck context, and the issue detail's `now.next` (AC2, AC5, AC7).
- A test review after an unrelated earlier worktree change, with a new untracked test: the recorded change (AC6).
- A test review after a blocked executor whose retry follows a pushed commit merged by the lead's `exception` launch: both attempts' edits, not the pushed path (AC6).

## Verification Strategy

### Correctness Proof Method

- **Correctness definition**: AC1–AC10 hold. Every existing flow, phase, handoff and integration test that does not pin an observed defect passes unchanged. The one test that pins observation 1 is rewritten to the expected behavior (AC1).
- **Verification method**: unit tests in `flow.rs`/`phase.rs`, integration tests in `issue_flow_int.rs`, a story test, and the generated-types check. Then `cargo test --workspace` and `pnpm run check:all`.
- **Verification timing**: core tests after order item 1, before any daemon change; integration tests after items 2 and 3.

### Early Verification Point

- **First verification target**: `flow.rs` with `correction` and the route functions in place, the existing test suite, and the AC1, AC4 and AC10 flow tests.
- **Success criteria**: all existing `flow.rs` and `phase.rs` tests pass unchanged, and the new tests pass.
- **Failure response**: if an existing transition test cannot pass without reading the inner check's findings state beyond `check` and `review_round`, the two-slot premise is wrong. Return to this design before changing daemon readers.

### Output Comparison

- **Comparison input**: the existing flow tests' fact sequences (round correction, round author correction, PR comments applied/none applied, planned-task test reviews, stub repairs in build and in a round). For the test-review pipeline, the existing `a_test_review_receives_the_tests_a_task_changed_whatever_their_naming` run, a test review right after `implement` with nothing changed before the executor started.
- **Expected output fields**: the actions and resulting stages each flow test asserts today. For the test-review item, the same `changedFiles` paths as today, with `diffBase` now the tree the change started from.
- **Diff method**: the unchanged assertions of those tests.
- **Transformation pipeline coverage**: `codebase_analysis.dataTransformationPipelines`:
  - `handoff.rs:test_review`: covered by the `changedFiles` and `diffBase`/`diffHead` assertions (implementation order item 3).
  - `review_scope`: covered by AC2 and AC3 (the AC1 recheck and the two-task test).
  - `executor_applied`/`correction_findings`: covered by the AC1 integration test. The quality run after the nested test-review correction still receives the applied comments, because the inner judgment stays under `test-review-judgment` (Selected Design item 2).

## Design Boundaries

- **Intentional limitations**:
  - Nesting deeper than an outer correction plus one check is not represented, because no path reaches it (`codebase_analysis` simplifications).
  - A record already corrupted by observation 1 before the upgrade is not repaired (Migration Strategy).
  - A test review whose executor launched before the upgrade attributes files as before (fallback pair).
  - After a merge into an open change, a path that both the change's work and the merged push changed is listed. An agent's own edits between a blocked attempt and its retry are attributed to the change (Selected Design item 6).
  - The GH-34 path is not reproduced separately (Non-Goals).
  - Request texts written for agents are unchanged; the test-review step file's input description and Step 1 check change (Selected Design item 8).
- **Existing extension points used**:
  - `filing` (consumers: `run`, `calls_for`, handoff, issues request text).
  - `IssueRecord.trees` (consumers: `task_files`, `stubs`, the fallback pair).
  - `tree_of` (consumers: `after_done`, `planned`, now `launch` and `merged_into`).
  - The `Position` → `types.gen.ts` route (consumer: dashboard `IssueView`).

## Risks and Mitigation

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| Git prunes an unreachable tree object between its recording and the test review, so `git diff <diffBase> <diffHead>` fails | Medium | Low | Git's default prune grace period is two weeks, and both trees are recorded during the task or correction under review. `changed_files` already relies on the same objects today. The reviewer blocks on a missing tree, and the lead's `exception` handles it. |
| An open change keeps its start across a lead's `Redesign`, so the next executor's change also lists the Design Doc's edits | Low | Low | The test reviewer chooses integration and E2E tests among `changedFiles`, so an extra document path adds nothing to review. `Replan` resets it through `planned`. |
| The `redirect_author` keep rule differs from the old `origin == Document`, whose value could be left over from any earlier document check. A first design sync blocked after a design review that cleared on its first run now goes from the author's judgment to the commit, as a design sync's findings do, instead of running the design review again. A first design review blocked while an earlier document's findings left `origin` at `Document` now restarts from the design | Low | Low | The new rule matches every case where the old origin was set by the check being redirected, every round case and the design sync after corrected design review findings (AC4). Flow unit tests cover `Redesign` in these cases: after a blocked design-review recheck (keep), after a blocked first design review outside a round (clear), after a blocked design sync that follows corrected design review findings (keep, AC4), and the AC4 round case from a saved record (keep). |

## References

- GH-52 and its comment: `state/sources/issues/GH-52.md`
- GH-44 (§ Expected, § Done when): https://github.com/shinpr/ovenspring/issues/44
- `docs/design/factory-core-design.md` § Steps, § Phases
- `docs/design/owner-interface-design.md` `GET /overview`, `GET /issues/{key}`
- `docs/design/events-and-error-notices-design.md` § Event details, Review round
- `docs/ui-spec/oven-spring-ui-spec.md:293` (Next line)
- git-diff(1): https://git-scm.com/docs/git-diff
