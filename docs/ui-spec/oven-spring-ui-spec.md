# Oven Spring UI Specification

## Overview

The owner meets Oven Spring through two surfaces: Slack, where the owner acts (every decision that is theirs arrives and is answered there, the lead talks with them, and they pause and resume the factory), and a local dashboard, where they set the factory up, keep the product goal in view, see how the work stands, follow the agents, change settings, and download records. Each function lives on one of the two; the terminal is used only to install, start and stop. This specification fixes both surfaces' structure, states and visual contract.

Five principles decide every dashboard screen below.

1. **The goal is always in view.** The factory chooses issues, judges how far to take each, and proposes every sprint against the product goal; it is the product's core value. The goal sits in the same band under the header on every screen.
2. **Two layers, two ways of writing.** Now is glanced at many times a day: position, a fixed set of short words and numbers, no sentences. Every other screen is read: sentences that name who acts.
3. **Groups are visible.** Distances step up and never overlap (inside an item, between items, heading to list, between subgroups, between groups, between page regions); hairlines only between items of a list; headings step up in size; group content starts 8px in from its heading.
4. **One signal per state; marks only with their word.** Colour and marks carry meaning only next to the word they mark and only where attention is needed: ● with what is the owner's (and with Setup's "Checking…"), ▲ with a problem the lead is handling. No legend anywhere.
5. **Decisions are never made in the dashboard.** Each one opens in Slack; commands say where to run them.

### Requirement Context

- PRD path: `docs/prd/oven-spring-prd.md`
- Feature scope: AC-029 to AC-035 (Slack), AC-002, AC-043 to AC-050, AC-053, AC-056, AC-057, AC-059 and AC-060 (dashboard, setup and goal), AC-076 to AC-078 (the Events page, error notices and their switch), and the owner-facing parts of AC-007 to AC-012, AC-027, AC-031, AC-037, AC-052, AC-058 and AC-072 (the issue filter's drops and failures).

### Design Source

| Source | Path | Version |
|--------|------|---------|
| Prototype code | `docs/ui-spec/assets/dashboard/index.html` (one page) | The layout and wording reference; this document governs where they differ |
| Prototype code | `docs/ui-spec/assets/events/prototype.html` (one page) | The Events page (S-08), the Error notices row in Settings, and the wording of the Slack error notices (M-13, M-14); approved by the owner (GH-9) |

## Prototype Management

- **Attachment path**: `docs/ui-spec/assets/dashboard/`; and `docs/ui-spec/assets/events/prototype.html` for S-08, the Error notices row of S-05 Connections and the wording of M-13 and M-14. Where the two prototypes overlap (the header, the goal band, the Settings page around the new row), the events prototype decides only what it adds: the Events link in the navigation and the Error notices row.
- **Version identification**: file date; later changes to the dashboard's look update the file first.
- **Compliance premise**: the dashboard's tokens (`dashboard/tokens/tokens.json`); no external design system.
- **Reference strength**: `binding` for structure, layout, spacing, vocabulary and copy tone; data, names and numbers are fictional. The events prototype is `binding` the same way: the owner chose it for the Events page and the error-notices switch, "to match their approved prototype in layout, spacing and on-screen words" (GH-9, `sources/answers/GH-9-spec-13.md`).
- **Not part of the product**: the prototype's top control bar (factory situation, Explain each element), its explanation notes and its "How this was designed" page. In the events prototype: its control bar, its "Why this is here" notes, the pages that read "Unchanged by this issue", and the chrome of its "The Slack notices" page (whose message texts are the wording of M-13 and M-14).
- **Relationship to canonical spec**: the prototype decides structure, layout, spacing and the words on screen; where this document paraphrases on-screen copy, the prototype's wording applies. This document decides behavior, states, triggers and data, and wins where the two differ on those.

The Slack surface has no prototype; its messages are specified in Slack Messages below.

## External Resources Used

| Resource (project-tier label) | Feature-specific identifier | Notes |
|-------------------------------|-----------------------------|-------|
| Design Origin | `docs/ui-spec/assets/dashboard/index.html` | This specification's prototype |
| Design Origin | `docs/ui-spec/assets/events/prototype.html` (`#/events`, `#/settings/connections` row `row-notices`, `#/slack`) | The Events page, the Error notices row, the Slack error notices (GH-9) |
| Design System | None; tokens in `dashboard/tokens/tokens.json` | Fonts: Hanken Grotesk, IBM Plex Mono (SIL OFL), bundled in the binary |
| Visual Verification Environment | Storybook: stories for the golden states and the main component states in this document; for GH-9, the Events golden states 9 to 15 below | `dashboard/` |

## AC Traceability (Prototype)

| Criterion ID | Criterion Summary | Screen / State | Prototype Reference | Adoption Decision |
|-------|-----------|----------------|------------------------|-------------------|
| AC-043 | First screen: goal, working or paused (with cause), waiting on the owner, sprint progress, issue states | G-01, S-01 every situation | `#/now` | Adopted |
| AC-044 | Issue: outcome and non-goals, progress, findings, links, launches and cost | S-02 | `#/issue/<key>` | Adopted |
| AC-045 | Launches filtered by sprint and issue; model, duration, cost and log | S-04, D-01 | `#/activity`, run panel | Adopted |
| AC-046, AC-047 | One download per sprint or issue, masked | S-02, S-03 DownloadPanel | "Download this … records" | Adopted |
| AC-048 | Light, dark and system themes, WCAG 2.2 AA | All | Theme in the header | Adopted |
| AC-071 | The version the repository runs; a newer one and how to move to it | All, AppHeader | - (added after the prototype) | Adopted |
| AC-049 | Decisions link to Slack | S-01 Waiting for you, S-02 Now | ● actions | Adopted |
| AC-067 | The owner cancels an issue that has not ended | S-02 CancelPanel; M-02, M-09 Cancel issue | "Cancel issue…" | Adopted |
| AC-050 | English everywhere | All | All | Adopted |
| AC-053 | OpenCode note | S-05 Agents, RoleEditor | OpenCode choice | Adopted |
| AC-052, AC-058 | Permission mode and MCP servers, with their warnings | S-05 Agents | Permission mode, MCP rows | Adopted |
| AC-056 | Goal in the same place on every screen; rewritten from there | G-01, S-07 | Goal band, `#/goal` | Adopted |
| AC-007 to AC-012 | Role, connection and merge settings | S-05 | `#/settings/*` | Adopted |
| AC-002, AC-060, AC-065, AC-066 | Setup: Connect, Run the product, Set the goal; missing prerequisites | S-00; S-01 status | situations "First start …", "Two prerequisites missing" | Adopted |
| AC-031 | Prototype page with its reference strength | S-06 | `#/prototype/<key>` | Adopted |
| AC-076 | A page of recorded events, newest first; each with when, what, and its issue or sprint | S-08, EventLine | events prototype `#/events`, `line()` | Adopted |
| AC-072, AC-076 | The issue filter's drops (key, URL, reason) and failures (key, what failed) readable there | S-08, EventLine, EventFilters "The issue filter's drops and failures" | `#/events` lines ACME-150, ACME-151, ACME-153 | Adopted; the drop's link is named after the recorded URL's tracker; a key with no factory record is not linked to S-02; a filter failure on another issue gets its own line instead of folding into the first |
| AC-077 | One notice when an error starts and one when it clears, however often it repeats | M-13, M-14; S-08 ErrorsNow and the folded start line | `#/slack`; `#/events` Errors now | Adopted; notices go to Slack only, a message Slack did not accept is posted once Slack accepts again (Slack Messages, Delivery), an error that pauses the factory is told by M-08 alone, M-14 drops the prototype's "judged now" sentence, and Errors now on S-08 shows what is going on |
| AC-078 | Error notices on or off in Settings, on by default; off stops only the notices | S-05 Connections, ErrorNoticesRow; S-08 notices-off state | `#/settings/connections` `row-notices`; situation "Error notices turned off" | Adopted |
| AC-076 | Settings changes shown as events | S-08, EventLine | `#/events` "Settings changed: …" | Adopted |

## Screen List and Transitions

### Screen List

| Screen ID | Screen Name | Layer | Description | Entry Condition |
|-----------|------------|-------|-------------|-----------------|
| S-00 | Setup | Read | Three steps: Connect (GitHub, issues, Slack, the agent), Run the product, Set the goal | Setup is not finished: no configuration, how the product runs not settled, or no goal |
| G-01 | Goal band | - | The product goal and Rewrite, on every screen under the header | Set up |
| S-01 | Now | Glance | The factory's state, the sprint line, Waiting for you, In progress, Waiting to start, Done | Root; header "Now" |
| S-02 | Issue | Read, cancel | Cancel issue… (header), Now, Path, What it is, Review findings, Runs and cost | An issue key or title anywhere |
| S-03 | Sprints | Read | This sprint (purpose, done count, cost so far by role, download); ended sprints | Header "Sprints" |
| S-04 | Activity | Read | Every agent run, newest first: Under way, Earlier by hour; filter by sprint and issue | Header "Activity"; "See them in Activity" |
| D-01 | Run panel | Read | One run: agent, model, effort, start, duration, result, why it failed, cost; what the agent said; the raw log | An activity on S-01; "follow" or "open run" on S-02 and S-04 |
| S-05 | Settings | Read | Connections, Agents, Merging, Safety and limits, Running the product, Messages | Header "Settings"; a prerequisite's link |
| S-06 | Prototype | Read | The issue's prototype, sandboxed, with its reference strength | M-04's link; "Prototype" on S-02 |
| S-07 | Goal | Read | What changes when the goal is saved; the new goal | "Rewrite…" in G-01 |
| S-08 | Events | Read | Errors now; What the factory did: every recorded event, newest first, under day headings, narrowed by Show and Issue | Header "Events"; "See it on Events" in M-13; "Events" in the Error notices row |

### Transition Conditions

| Source | Destination | Trigger | Guard Condition |
|--------|------------|---------|-----------------|
| Start | S-00 | `ovenspring start` opens the dashboard | Setup is not finished; it opens on the first step not done |
| S-00 | S-01 | "Start" on Set the goal | The goal is saved |
| Any | S-01, S-04, S-08, S-03, S-05 | Header link | Set up |
| Any | S-07 | Goal band link | Set up |
| S-01 | S-02 | Issue key or title | - |
| S-01 | D-01 | The activity of an issue whose step has a run | A run is under way |
| S-01 | Slack | A Waiting for you action | Decision delivered |
| S-01 | GitHub | "Merge on GitHub" or "Approve on GitHub" | `merge.by = owner`, ready; or GitHub requires the owner's approving review on the agreements pull request |
| S-01 | S-05 section | A prerequisite's link | The prerequisite names a setting |
| S-02 | S-04 filtered by the issue | "See them in Activity" | - |
| S-02 | S-06 | "Prototype" | The issue has a prototype |
| S-07 | S-01 | Save | The text changed and is not empty |
| S-08 | S-02 | An issue key in an event line or in Errors now | The factory has a record of the issue (otherwise the key is not a link) |
| S-08 | S-03 at the sprint | "Sprint" in an event line | The event concerns a sprint and no issue; S-03 scrolls to that sprint |
| S-08 | S-05 Connections | "Turn them on in Settings" (Errors now), a link in an error's what-to-do line | Notices are off; the error's fix is a setting |
| S-08 | Tracker | "Linear issue ↗" / "GitHub issue ↗" / "Tracker issue ↗" on a drop line | The drop recorded a URL |
| S-05 Connections | S-08 | "Events" in the Error notices row's summary | Notices are off |
| Slack M-13 | S-08 | "See it on Events" | - |

## Vocabulary (glance layer)

The dashboard owns these words; the API returns codes (owner-interface design). A word appears only in its column's meaning.

### Phases

Five, the same for every issue: **Requirements**, **Design**, **Build**, **Review**, **Merge**. A step belongs to the phase of the work it advances; a correction (an author run again in update mode, `review-response`, `quality` after a correction) belongs to the phase of the check whose findings it answers (a document check: Requirements or Design; `test-review`: Build; the review round: Review; pull request comments: Merge). An exception keeps the phase it stopped in. A small issue's Design is "skipped".

### Activities (three words or fewer)

| Phase | Activities |
|-------|------------|
| Requirements | Reading the issue · Writing the PRD · Reviewing the PRD |
| Design | Reading the code · Choosing documents · Making a prototype · Reading the screens · Writing the UI Spec · Reviewing the UI Spec · Recording decisions · Reviewing decisions · Writing the design · Checking the design · Reviewing the design · Comparing designs · Writing test plans · Planning · Reviewing the plan · Splitting tasks |
| Build | Task n of m (Implementing for a small issue) · Reviewing tests · Quality checks · Committing |
| Review | Reviews running · Judging findings · Rechecking |
| Merge | Pushing · Waiting for CI · Fixing CI · Resolving a conflict · Waiting for bots · Waiting for a GitHub review · Blocked by GitHub · Checking merge |
| Any | Addressing <review> feedback (the lead's exception after reviews ended with findings outstanding, in normal styling; the reviews as they read in running text, such as "Addressing design review feedback", "Addressing security review feedback", "Addressing code and security review feedback") · ▲ Problem: <cause> (the lead's exception for anything that could not go on; cause: "<step name> stopped", such as "Code review stopped", "dependency dropped", "PR closed", "push refused", "<operation> failing") · On hold (paused) |

### Waiting for you (actions)

Answer a question · Approve PRD · Approve prototype · Approve design · Approve merge · Merge on GitHub · Approve on GitHub · Approve recovery · Resolve a stop · Approve sprint proposal · Close the sprint.

### Waiting to start (reasons)

"After <KEY> merges" (the first dependency not merged; "After <KEY> and n more merge" for several) · "When an issue in progress ends" (Issues at once in Settings is reached).

### Done (outcomes)

Merged · Closed: already done (the lead closed it with evidence) · Dropped (the lead dropped it from an exception) · Canceled by you (the owner canceled it) · Held for the review.

### Time

A duration ("3 min", "2 h 5 min") is how long an issue has been in its current activity. A clock time ("12:48") is when it ended, with its date ("Nov 20, 21:00") when that was on an earlier day, since a sprint spans days. Waiting for you and Waiting to start show no time.

On S-08, events are grouped under a day heading: "Today, Nov 27", "Yesterday, Nov 26", then "Nov 25" for earlier days of this year and "Dec 30, 2025" for an earlier year. A line shows only its clock time, since the heading gives the day. A time in a line's second line or in Errors now ("last at", "since") is a clock time when it falls on the same day as the line's heading (for Errors now: today), and the date and clock ("Nov 26, 23:58") otherwise. All times are the browser's local time.

### Events (read layer)

The server returns each event's kind and fields; the dashboard words them (`dashboard/src/lib/vocabulary.ts`), as it does for the glance layer. Each event is one sentence in the past tense or the present state it left, naming who acted when it was not the factory ("You approved …", "Linear refused …", "Claude Code could not …"). The events prototype's lines are the wording for their kinds: pushed, merged, closed on the tracker, asked in Slack, review round ended, filter dropped, sprint proposed and approved, started, paused and resumed, settings changed, and the error and clear lines below. Closed on the tracker says what the event records: "Marked done on <Tracker>." when it records that the issue was marked done and on which tracker, "Canceled on <Tracker>." when it records a cancellation, and "Closed on the tracker." when it records neither (the factory sets Done for a merged issue and Canceled for one it closes otherwise, under one event kind; `crates/daemon/src/issues.rs`, `notify_dependents` and the close-on-tracker path). Every other kind recorded in `events.jsonl` gets a sentence written the same way in the vocabulary. A kind with no sentence there shows its code with hyphens as spaces and its first letter capitalized ("Replay reset."), so no recorded line is dropped or blank. Quoted words (a reason, a sprint's purpose, the owner's words) are in curly quotes; commands, paths and the output they printed are in the mono face. Every text shown is masked as the download masks it (AC-047).

**Errors.** An error is an operation that failed. It starts at its first failure and clears when the same operation next succeeds, or when the owner removes what was failing (removing the issue filter command clears the filter's error, since the factory no longer runs it); the failures between are its repeats. Each error has a name, its start sentence, its clear sentence, and one line of what to do while it lasts. The events prototype fixes four:

| Error | Name (Errors now, M-13) | Start line | Clear line | What to do |
|-------|-------------------------|-----------|------------|------------|
| A service does not answer (GitHub, the tracker) | Can't reach <service> | "Could not reach <service> while <what it was doing>: <cause>. The factory tries again<interval>." | "<Service> answers again." | "Nothing to do unless it lasts. The factory tries again<interval>." |
| The tracker refuses the connection | <Tracker> refuses the connection | "<Tracker> refused the connection (<cause>). No new issue or change is read from it until it is fixed." | "<Tracker> accepts the connection again." | "Check the tracker's key or token in Settings." with Settings linked to the Issue tracker row |
| The agent CLI cannot start a run | <CLI> cannot start a run | "<CLI> could not start a run: <its error as it wrote it>." | "<CLI> starts again." | "Fix it in a terminal on this computer, as the error says." |
| The issue filter fails | The issue filter fails | "The issue filter could not judge <KEY>: <command> <what failed> (<its output>). The issue goes on as without a filter." | "The issue filter works again."; when the owner removed the command, "The issue filter was removed." | "Issues go on as without a filter until it works. Fix <command> or change it in Settings." with Settings linked |

<interval> is " every minute" for the tracker, whose reachability is recorded only by its read once a minute (`Timing.tracker_poll` in `crates/daemon/src/lib.rs`; `crates/daemon/src/intake.rs`), and empty for GitHub ("The factory tries again."), since a GitHub failure can come from the 30-second read of pull requests or from repairing the board on every daemon pass (`crates/daemon/src/lib.rs`, `tick`; `crates/daemon/src/stops.rs`, `settle_for`), which repeat at different intervals. This overrides the events prototype's "every 30 seconds". No start line promises an effect the factory does not hold to: a tracker that refuses new reads does not stop a sprint proposal built from issues read earlier, so its line says only that nothing new is read.

The issue filter's failure is always an error (AC-072, AC-077); its drop never is. Which other failures are errors is settled by the Design Doc; each it adds gets the same four texts in the vocabulary. The name on S-01's "Can't reach <service>" title and the same error's name and "since" time on S-08 come from the same record, so the two pages never disagree.

## Component Decomposition

### Component Tree

```
App
  +-- AppHeader (wordmark, version and its update notice, repository, navigation Now · Activity · Events · Sprints · Settings, theme)
  +-- GoalBand (G-01)
  +-- Page
      +-- Setup (S-00) > SetupSteps; Connect > ConnectionBlock x3 (ConnectionResult), AgentChoice; RunTheProduct; SetGoal
      +-- Now (S-01)
      |   +-- FactoryStatus (page title; prerequisite list when missing)
      |   +-- SprintLine
      |   +-- GlanceSection "Waiting for you" > GlanceRow
      |   +-- GlanceSection "In progress" > GlanceRow (activity opens D-01)
      |   +-- GlanceSection "Waiting to start" > GlanceRow
      |   +-- GlanceSection "Done" > GlanceRow
      +-- Issue (S-02)
      |   +-- IssueHeader (breadcrumb, title, "Cancel issue…", links), CancelPanel when opened, IssueNow
      |   +-- Group "Path" > PhaseItem > RunLine
      |   +-- Group "What it is" (outcome, not in it, size)
      |   +-- Group "Review findings" (summary such as "3 findings: 2 applied, 1 being judged.", expandable list; each finding names its source: a review and its round, or "<author> on the pull request" for a comment)
      |   +-- Group "Runs and cost" ("n runs, m running." with the cost by CLI and an Activity link; "No runs yet." alone when none; DownloadPanel)
      +-- Sprints (S-03): Group "This sprint" (SprintCost, DownloadPanel), Group "Ended sprints" > SprintRow
      +-- Activity (S-04): ActivityFilters, Group "Under way", Group "Earlier" > HourGroup > RunLine
      +-- Events (S-08): Contact, page title and intro, ErrorsNow > ErrorItem, Group "What the factory did" > EventFilters, DayGroup > EventLine, "Show older events"
      +-- RunPanel (D-01)
      +-- Settings (S-05): SettingsNav, Group per section > SettingRow / RoleBlock (RoleEditor); Connections > ErrorNoticesRow
      +-- Prototype (S-06), Goal (S-07)
```

### Component: AppHeader, version and update

The version is about the executable, not the repository, so it shows from the first screen, Setup included. It sits right after the wordmark, small and in `color-text-muted`, so it reads as Oven Spring's version. The notice follows it as a link-style button in `color-accent`. The repository's name comes after them, set apart by a rule in `color-line`, so the version is not read as the repository's.

| State | Trigger | Display | Interaction / Recovery | Evidence |
|-------|---------|---------|------------------------|----------|
| Current | `update` is null | "v0.1.0" | - | AC-071 |
| Available | A newer release is out and not installed | "v0.1.0", then "v0.2.0 is out" | The button opens the steps in a row of its own under the header's line: "Oven Spring 0.2.0 is out." with "What's new" (the release page, external); "To update, run this in a terminal:" and the install command in a block; "Run `ovenspring stop` and then `ovenspring start` in the folder of each repository that runs Oven Spring. Stopping ends the runs under way. They start again when the factory starts." Pressing it again closes them | AC-071, AC-004 |
| Restart | A version other than the running one is installed | "v0.1.0", then "Restart to run v0.2.0" | The button opens: "Oven Spring 0.2.0 is installed, but this repository still runs 0.1.0." and the restart sentences, for this repository's folder | AC-071, AC-004 |
| Unknown | The daemon has not answered | Nothing after the wordmark | - | - |

Nothing restarts or installs from the dashboard: a restart ends the runs under way, so the owner chooses when.

### Component: GoalBand

| State | Trigger | Display | Interaction / Recovery | Evidence |
|-------|---------|---------|------------------------|----------|
| Set | A goal exists | `color-raised` band under the header: "Goal" label (small, muted), the goal (lead), "Rewrite…" at the right | Rewrite → S-07 | AC-056 |
| Setup | Setup is not finished | Band absent; the goal is set as setup's last step | - | AC-002 |

### Component: GoalPage (S-07)

| State | Trigger | Display | Interaction / Recovery | Evidence |
|-------|---------|---------|------------------------|----------|
| Rewrite | A goal exists | Title "Rewrite the product goal"; group "What changes when you save": every run from then on receives it, including the running sprint's; the running sprint keeps its purpose and issues; the next sprint is proposed toward it; group "New goal": help line, textarea with the current text, "Save the new goal" (disabled until the text changes and is not empty), Cancel, a note saying why Save is disabled | Save stores it, returns to S-01, the band shows it | AC-056, AC-057 |
| Save failed | The API refused | The reason under the field; the text kept | Retry | AC-056 |

### Component: FactoryStatus

The page title of S-01 (page-title size, so the first line does not move between screens). Exactly one state, in this order; a halting state's title is in `color-fail`, a state waiting only on the owner in `color-accent`. Lines under the title are lead size; instructions appear only when something is the owner's to fix.

| State | Trigger | Title | Lines | Evidence |
|-------|---------|-------|-------|----------|
| Lost contact | The dashboard's poll fails | The dashboard lost contact with the factory | Last contact <clock>. Everything below is as it was then. If the factory stopped, run `ovenspring start` in a terminal in this repository's folder. It opens a new dashboard tab | AC-043 |
| Paused | `HALT` exists | Paused | The cause; "Resume with `@<bot name> !resume` in Slack" (after fixing the cause when there is one), the bot's name as Slack shows it (the app manifest's name until Slack has said it) | AC-034, AC-037 |
| Prerequisites missing | One or more | Partly on hold | "Steps that need these wait; everything else goes on." then one item per prerequisite: what (bold), how to fix and where (muted), a link to the setting that fixes it | AC-060 |
| Service out of reach | GitHub, the tracker or Slack failing to answer | Can't reach <service> | Since <clock>; the factory tries again every 30 seconds; what waits; nothing to do unless it lasts | AC-062 |
| Settings not applied | `.ovenspring/config.toml` cannot be used while the factory runs | Settings not applied | The key and the reason; the factory goes on with the settings it last read without an error; change it in Settings or fix the file | AC-010 to AC-012 |
| Proposing | `sprint-plan` running | Proposing the first sprint (none ended yet) / Proposing the next sprint | The lead is choosing issues from the goal and the open issues on the tracker (their count); the proposal will reach you in Slack | AC-015 |
| Waiting for an issue | The proposal held no issue | Waiting for an issue that serves the goal | The lead told you in Slack what is missing; a new sprint is proposed once an issue opens | AC-015 |
| Proposal or review with the owner | Awaiting approval | Waiting for you | No sprint runs until you answer in Slack (proposal) / Read the sprint review in Slack. Reply with what you think of the result, or close the sprint. | AC-016, AC-041 |
| A sprint step stopped | The lead is finding a way past a sprint step that could not finish | The lead is handling a problem with the sprint | Nothing new starts until it is past; you are asked in Slack if the lead needs you | AC-037 |
| A sprint step waits for you | The owner is asked about a sprint step | Waiting for you | The sprint goes on when you answer in Slack | AC-037 |
| Reviewing the sprint | Collecting facts, writing the review, checking agreements | Reviewing the sprint | Which of the three is under way; the review reaches you in Slack | AC-041 |
| Applying agreements | The agreements pull request is open, the factory merges, and GitHub requires no approving review | Merging the working agreements | They merge once their checks pass, and the next sprint is proposed then. Nothing is needed from you; "See the pull request ↗" | AC-042 |
| Agreements awaiting approval | The agreements pull request is open, the factory merges, and GitHub requires an approving review | Waiting for you | "Approve the working agreements and knowledge on GitHub. The factory then merges them and proposes the next sprint."; the pull request is an Approve on GitHub row under Waiting for you | AC-042 |
| Agreements with the owner | The agreements pull request is open and the owner merges (`merge.by = owner`) | Waiting for you | "Merge the working agreements and knowledge on GitHub. The next sprint is proposed once they are merged."; the pull request is a Merge on GitHub row under Waiting for you | AC-042 |
| Working | Otherwise | Working | "n issues in progress." — or, titled "Waiting for you" (accent), "Everything under way waits on you." when none is in progress and decisions wait | AC-043 |

A change of state is announced through a polite live region.

### Component: SprintLine

| State | Trigger | Display | Evidence |
|-------|---------|---------|----------|
| Running | A sprint runs | "Sprint" (muted), the purpose, "· n of m ended" (bold); m is every issue of the sprint, n those merged, closed, dropped, canceled or held | AC-043 |
| None | No sprint runs | "No sprint has run yet." before the first sprint; after one, "No sprint running. The last sprint ended <date, time>." and a link to Sprints. The In progress, Waiting to start and Done sections appear only while a sprint runs | - |

### Component: GlanceRow

One grid for every section of S-01: key (mono, muted) · title · phase · status · meta. Each issue appears in exactly one section. Rows are ordered by key within a section.

| Section | Phase | Status | Meta | Interaction | Evidence |
|---------|-------|--------|------|-------------|----------|
| Waiting for you | - | ● <action> ↗ (accent, opens the Slack message in a new tab, or the pull request for Merge on GitHub and Approve on GitHub); "not in Slack yet" (muted) when undelivered; sprint decisions have no key and are titled by the sprint's purpose; the agreements pull request the owner merges is titled "Working agreements and knowledge" | - | Key and title → S-02 | AC-029, AC-030, AC-049 |
| In progress | Phase word (muted) | Activity; "Addressing <review> feedback" in normal styling while the lead addresses outstanding review findings; ▲ Problem: <cause> in `color-fail`; On hold (paused) muted | Duration | Activity with a running run → D-01 (for "Addressing <review> feedback", the lead's running run only; none before it starts); key and title → S-02 | AC-043 |
| Waiting to start | - | Reason (muted), a dependency's key links to it | - | → S-02 | AC-030 |
| Done | - | Outcome (Merged in text colour, the others muted) | Clock time, with the date for an earlier day | → S-02 | AC-041 |

Empty sections: "Nothing is in progress." and "No issue has ended yet."; Waiting for you and Waiting to start are omitted when empty. At widths below 960px a row is key and title on the first line, then phase · status · meta under the title; five columns leave a title too little room there.

### Component: IssueNow (S-02)

| State | Display | Evidence |
|-------|---------|----------|
| In progress | "<Who> <activity sentence> · for <duration>" and "follow" when a run is under way; "GitHub won't merge it yet." and "See why on the pull request ↗" while GitHub blocks the merge; "Lead is addressing <review> feedback · for <duration>" and "follow" while the lead addresses outstanding review findings in its running run, or "Addressing <review> feedback · for <duration>" with no one named and no "follow" before that run starts (normal styling); "The lead is handling a problem: <cause>" (`color-fail`) during any other exception; "· on hold while the factory is paused" while paused | AC-044 |
| Waiting for you | "Waiting for you: <action>" and "Answer in Slack ↗", or "Open the pull request ↗" for Merge on GitHub | AC-049 |
| Waiting to start | The reason: "Starts after <KEY> merges." ("Starts after <KEY> and <KEY> merge." for several), or "Starts when <KEY> (or one of <KEYS>) ends, because Settings limits how many issues run at once." with Settings linked | AC-030 |
| Done | "<Outcome> at <clock>.", with ": <the owner's words>" when the owner canceled it with words | AC-041 |

### Component: CancelPanel (S-02)

Opened by "Cancel issue…", a plain button at the end of the title's line (under the title, at its start, below 760px), shown while the issue has not ended, or ended held. It cancels the whole issue, so it sits with the page's header, away from the links (which only lead elsewhere) and from routine actions such as follow and the download. The panel opens under the header, aligned with the button and about 62 characters wide (the field fills it), before the issue's Now line; only its final button, "Cancel the issue", is drawn in `color-fail`. Closing it keeps what was typed and a request already sent, so reopening it before the page shows the issue ended does not send it again. On an issue that ended, the button is not shown: nothing is left to stop, and the Now line says how it ended.

| State | Display | Evidence |
|-------|---------|----------|
| Open | "The factory stops work on it, closes its pull request (the branch stays) and marks it canceled on <tracker>.", the field "Why (optional)" with "Shown on <tracker>.", and "Cancel the issue" | AC-067 |
| Sent | "Canceled. The work stops within seconds." beside the disabled button; the page shows the issue as Canceled by you on the next poll | AC-067 |
| Failed | "Canceling failed. Try again." | AC-067 |

### Component: PhaseItem

| State | Display | Evidence |
|-------|---------|----------|
| Done | Phase name (subgroup size), "done · <start>–<end>", "n runs" collapsed | AC-044 |
| Now | Phase name (accent), "now · from <clock>", its runs listed oldest first (RunLine), the running one last with "follow", then a muted "Next: <what follows in the phase>"; what the issue is doing and for how long is said once, above the path | AC-044 |
| Ahead | Phase name, "ahead", one sentence of what the phase holds (also for an issue waiting to start; why it waits is said once, above the path) | AC-044 |
| Skipped | Phase name, "skipped", why (a small change goes straight to building; "Not needed: you canceled the issue." after a cancel; not needed after a close) | AC-018 |

A phase lists each step once: a design-phase step's preparation turn (whether to ask the owner anything first) is left out once it ended normally, while its span still counts it. One running or failed stays listed, to be followed or opened. "n runs" in Runs and cost counts every run.

### Component: RunLine

The time (mono): `<start>–<end>` on S-02; on S-04 the end time (`<start>–` while running) and the issue's key · "<Role> <what it did>", "· for <duration>" while running, and "→ <result>" (`color-ok` for pass, `color-fail` for a failure, muted otherwise) · "open run" (or "follow" while running).

A preparation turn reads by what it came to, before the step it prepares: "Lead asked you a question before reading the code", "found nothing to ask you before…", "told you something before…"; "is checking whether to ask you anything before…" while it runs, and "checked whether to ask you anything before…" when it failed.

### Component: RunPanel (D-01)

| State | Trigger | Display | Evidence |
|-------|---------|---------|----------|
| Running | The run has not ended | Agent · model · effort; started; running for; "Known when it ends" for cost; "What the agent has said so far": its messages in order | AC-045 |
| Ended | The run ended | Agent · model · effort; started; took; result; cost, or "Not known"; "What the agent said last" | AC-045 |
| Failed | The run failed | Result in `color-fail`; "Why": the failure as the CLI reported it and what the factory did (ran it again, paused) | AC-037 |
| Any | - | "Open the full log ↗": the raw output in a new tab | AC-045 |
| Could not be read | The API failed | "This run could not be read." | - |

Escape and the Close button close the panel; focus moves into the panel when it opens.

### Component: ActivityFilters and lists (S-04)

Sprint: this sprint, each ended sprint, All sprints. Issue: All issues, then the sprint's issues when this sprint is selected. "Under way": runs not ended. "Earlier": ended runs, newest first, under an hour heading ("14:00"). Empty: "Nothing is running for this filter." / "Nothing has happened yet." Only agent runs are listed.

### Component: Contact

Shared by every page that polls (S-02, S-04, S-08 and the others that use it). Its lost-contact line is two sentences, as the events prototype words it and as Copy on Screen asks: "The dashboard lost contact with the factory. This page is as it was at <clock>." The not-yet-read line is unchanged. Now keeps its own FactoryStatus wording.

### Component: EventsPage (S-08)

Page title "Events", then the muted intro "Everything the factory recorded, newest first." Two groups follow: "Errors now" (ErrorsNow), then "What the factory did" (EventFilters and the list). The page lists every line of `events.jsonl` except an error's repeats, which are folded: a repeat that concerns the same issue (or no issue) and records the same cause as an earlier line of the same error raises that line's count and last time instead of adding a line. A repeat on another issue, or with another cause, gets its own line, so every failure's time, issue key and cause stays readable and the Issue filter finds it (AC-072, AC-076, AC-078). An error has one start line, its first failure; its other lines are Error repeat lines (EventLine). Agent runs stay on S-04 and are not listed here. The page polls like S-04 and keeps the last answer when the factory does not answer.

| State | Trigger | Display | Interaction / Recovery | Evidence |
|-------|---------|---------|------------------------|----------|
| Listed | Events exist and some match the filters | Lines under day headings, newest first; the first 15 matching lines, then "Show older events" when more match | "Show older events" shows the next 50 older matching lines; a changed filter shows the first 15 again | AC-076; events prototype `viewEvents` |
| All shown | Every matching line is shown and no filter narrows the list | "That is everything the factory has recorded." after the last line | - | events prototype |
| Filtered to nothing | Events exist and none matches | "No event matches this filter." | Change a filter | events prototype |
| Nothing recorded | `events.jsonl` is missing or empty | "Nothing has been recorded yet."; ErrorsNow "Nothing is wrong now." | - | events prototype situation "Nothing recorded yet" |
| Unreadable | The server answered that it could not read the events | "The events could not be read. The dashboard tries again in a few seconds." in place of the list; ErrorsNow "Not known while the events cannot be read." | The next poll reads again | events prototype situation "The events could not be read" |
| Lost contact | The poll fails after an earlier answer | Contact's lost-contact line above the title; the page as last read | The next poll that answers removes the line | events prototype situation "Dashboard lost contact"; Contact |
| Not read yet | The poll fails before any answer | Contact's not-yet-read line ("These events could not be read: …"); nothing else under the title | The next poll that answers shows the page | Contact |

New lines that arrive while the page is open appear at the top under their day; the lines already shown stay shown.

### Component: ErrorsNow

Group heading "Errors now". It lists every error that has started and not cleared, oldest start first, whatever the filters say.

| State | Trigger | Display | Interaction / Recovery | Evidence |
|-------|---------|---------|------------------------|----------|
| Errors going on | One or more errors have not cleared | One item each: the error's name (`color-fail`, weight 600), then muted "since <time>, first on <KEY>. <n> times, last at <time>." ("first on <KEY>" only when the first failure concerned an issue; "<n> times, last at <time>." only when n > 1); under it, the error's what-to-do line (small) | The key → S-02 when the factory has a record of the issue (plain text otherwise, as in EventLine); a Settings link → S-05 Connections | AC-077; events prototype `OPEN` |
| Nothing wrong | No error is open | "Nothing is wrong now." | - | events prototype |
| Notices off | Error notices are off | Above the items or the empty line: "Error notices are off, so Slack tells you nothing about these." and "Turn them on in Settings" (→ S-05 Connections) | Link | AC-078; events prototype situation "Error notices turned off" |
| Unknown | The events cannot be read | "Not known while the events cannot be read." | - | events prototype |

### Component: EventFilters

Two labelled selects kept in the URL's query, reusing how Activity (S-04) keeps and reads its filters. Their layout and spacing are the events prototype's `.filters`: the two controls side by side, each label beside its select, wrapping onto a second row when they do not fit (gaps 10px between rows and 16px between controls); not Activity's label-and-control grid. They narrow the list only; ErrorsNow is not narrowed.

- **Show**: "Everything" (default); "Errors and when they cleared" (Error started, Error repeat and Error cleared lines); "The issue filter's drops and failures" (drop lines, and the issue filter's Error started, Error repeat and Error cleared lines), so every recorded filter failure stays listed with its key and cause (AC-072, AC-076).
- **Issue**: "All issues" (default), then each issue key that appears in the events, sorted, with its title after it; the bare key when the title is not known. A line matches when it concerns that issue.

### Component: EventLine

One grid per line: clock (mono, muted) · subject · sentence. The subject is:

- the issue key linking to S-02 when the factory has a record of that issue; the key as plain text (mono) when it has none, as for an issue the filter dropped before the factory ever took it in, since S-02 has nothing to show for it (`crates/server/src/api.rs`, `get_issue` answers 404);
- "Sprint" when the event concerns a sprint and no issue. Its accessible name and tooltip are "Sprint: <the sprint's purpose>" when the sprint's record holds a purpose, and "Sprint begun <date, clock>" otherwise, the time read from the sprint's ID, which is when it was begun (`crates/daemon/src/sprint.rs`, `maybe_begin_sprint`), so a `sprint-begun` line recorded before any purpose exists still names its sprint and two sprint lines tell their sprints apart. When S-03 lists the sprint, the subject links to that sprint's own place there (`/sprints#sprint-<id>`: the "This sprint" group for the current sprint, the sprint's SprintRow for an ended one), scrolled into view; S-03 gives each sprint it lists that anchor. A sprint S-03 does not list (one not yet approved, `crates/daemon/src/overview.rs`) is plain text, not a link, as for an issue key with no factory record, since no page shows it; being plain text, it shows its name ("Sprint: <purpose>" or "Sprint begun <date, clock>") as its visible text, wrapping inside the subject column, rather than only in a tooltip, so every owner reads which sprint it is without hovering or focusing it (AC-048, AC-076);
- blank when the event concerns the whole factory. An event concerns an issue or a sprint when it records one, directly or through the work item it names; the Design Doc settles how each kind is attributed.

| State | Trigger | Display | Evidence |
|-------|---------|---------|----------|
| Plain | Any event that is not an error's start or clear | The event's sentence (Vocabulary, Events) | AC-076 |
| Error started | The first failure of an error | "Error" (`color-fail`, weight 600), then the start sentence. Second line (muted): "Happened <n> times so far, last at <time>." while it is open, "Happened <n> times, last at <time>." once cleared, only when n > 1; then whether the owner was told | AC-077 |
| Error repeat | A failure of an open error on another issue, or with another cause, than its earlier lines | "Error" as above, then the start sentence with this failure's issue key and cause. Second line: the count as above when n > 1, then "Same error as at <time of its start line>, so no new message." | AC-072, AC-077 |
| Error cleared | The operation next succeeded | "Cleared" (`color-ok`, weight 600), then the clear sentence; second line: whether the owner was told | AC-077 |
| Told | A start or clear line whose message Slack accepted (M-13, M-14, or M-08 for an error that paused the factory) | "You were told in Slack."; "You were told in Slack at <time>." when it was accepted later than the line | AC-077 |
| Waiting for Slack | Notices are on and Slack has not accepted the message yet | "Not told yet: Slack did not accept the message. It is sent once Slack accepts messages again." | AC-077; Slack Messages, delivery |
| Not told, notices off | A start or clear line while notices were off, or notices were turned off while its message waited | "Not told: error notices are off." | AC-078 |
| Drop | The issue filter dropped an issue | "The issue filter dropped <KEY>: “<reason>”"; second line: a link that opens the recorded URL in a new tab, named after that URL's tracker ("Linear issue ↗" for a linear.app address, "GitHub issue ↗" for a github.com address, "Tracker issue ↗" otherwise), so a drop recorded before a tracker switch still names what it opens | AC-072 |

The Error and Cleared words carry the meaning; colour only adds to them. Below 620px the grid is clock · one column, and the subject sits above the sentence.

### Interaction Definition (S-08)

| Criterion ID | EARS Condition | User Action | System Response | State Transition | Error Handling |
|-------|---------------|-------------|-----------------|-----------------|----------------|
| AC-076 | When the owner opens Events | Header "Events" | The newest 15 lines under day headings, ErrorsNow above | - → Listed / Nothing recorded | Unreadable or Lost contact as above |
| AC-076 | When more lines match than are shown | "Show older events" | The next 50 older matching lines appear below | Listed → Listed / All shown | - |
| AC-072, AC-076 | When the owner chooses "The issue filter's drops and failures" | Show select | Only drops, every filter failure line (start and repeat) and their clears are listed; the URL keeps the choice | Listed → Listed / Filtered to nothing | - |
| AC-076 | When the owner chooses an issue | Issue select | Only lines concerning it are listed | Listed → Listed / Filtered to nothing | - |
| AC-077 | While an error lasts | - | A repeat raises the count and last time on its line (same issue and cause) or adds an Error repeat line (another issue or cause); ErrorsNow's count and last time rise either way; no message is sent | - | - |
| AC-077, AC-072 | When the owner removes the failing filter command | Settings | A Cleared line "The issue filter was removed." appears; the error leaves ErrorsNow; its earlier lines stay | Errors going on → Nothing wrong when it was the last | - |
| AC-077 | When the error clears | - | A Cleared line appears at the top; the error leaves ErrorsNow | Errors going on → Nothing wrong when it was the last | - |

### Component: DownloadPanel

| State | Trigger | Display | Evidence |
|-------|---------|---------|----------|
| Collapsed | Default | "▸ Download this sprint's / this issue's records" | AC-046 |
| Open | Activate | What the file is (one JSONL file of every record, for an agent or an audit, secrets masked) and "Download (.jsonl)" | AC-046, AC-047 |
| Preparing | Download | "Preparing…", button disabled | AC-046 |
| Failed | The export failed | "The download failed. Try again." | AC-046 |

### Component: SprintCost

"Cost so far, by role": one line per role that ran: "<n> runs on <CLI> · $<sum>" or "no known cost"; then "Costs are at API prices, from the tokens each run used, whatever plan you pay for the CLIs on."

### Component: SprintRow (ended)

Summary: "▸ <date range>", the purpose, "<n> of <m> key results met". Open: each key result Met / Not met (ok / fail colour) with its evidence; the counts merged, dropped and held; working agreements changed; runs and cost with what it covers; DownloadPanel.

### Component: SettingRow

Label (bold) with one line of what it changes (muted) · the whole current value (wraps) · "Change". Editing opens the form below the row, at the item edge: each field with what to enter and where to get it; Save ("Check and save" where the backend checks the connection) and Cancel; when it applies. One row edits at a time; leaving with an unsaved change discards it and says so. A refused save shows the reason under the field.

| Section | Rows |
|---------|------|
| Connections | GitHub (repository, acting account, "Connected when last saved"); Issue tracker (Linear team or GitHub Issues; Filter command, empty for no filter, summarized as "Issues filtered by <command>" or "Every open issue reaches the agents"); Slack (channel, tokens set); Who the factory answers (member IDs; removing yourself warns). A saved token or key shows as "Saved, ending in" its last four characters, with Replace; its field opens only on Replace, with "Keep the saved one" to go back. A saved GitHub token is replaced, not removed |
| Agents | Roles (RoleBlock per role); Permission mode (Auto default, Bypass with its warning); MCP servers agents may use (where each is defined; a server that reaches GitHub, the tracker or Slack warns); Working agreements folder and Knowledge folder (each a folder in the repository git tracks, `.ovenspring/agreements` and `.ovenspring/knowledge` by default; AC-070) |
| Merging | Base branch; Who merges; Always held for you (fixed, no Change); Changes a person reviews (one line per kind, `name: the changes it covers`, the name in bold and the words in body type; its line under the name says it waits for your approval when the factory merges, and that the ready-to-merge message names it when you merge; AC-069); Review bots; Also held when a pull request changes these files (one row per category: Schema changes, External connections, Infrastructure, CI changes, External contracts), last, so its heading groups only its rows |
| Safety and limits | Denied commands (the always-denied list shown above the owner's); Issues at once; Round limits (review rounds, CI fixes; "no limit") |
| Running the product | The row "Commands": "Prepare: / Start: / Ready when this answers:" or "Nothing to run." with the reason; Change offers the same choice as setup, and "Check and save" starts the product once before saving |
| Messages | (Not built in this release: see Open Items) |

### Component: ErrorNoticesRow

A SettingRow in Connections, after Slack and before Who the factory answers. Label "Error notices", what-line "A message in Slack when an error starts and when it clears." Change opens a RadioGroup with its legend "Error notices" visible:

- **On** (default): "One message when an error starts and one when it clears, however often it repeats in between."
- **Off**: "No messages about errors. The Events page still shows every error."

These two texts are the owner's own words (`sources/answers/GH-9-prototype-14.md`) and are used as written. The buttons are Save and Cancel, with "Applies at once." Saving checks no connection, so the owner can turn notices off while Slack itself fails.

| State | Trigger | Display | Interaction / Recovery | Evidence |
|-------|---------|---------|------------------------|----------|
| On | The setting is on or was never set | "**On**" | Change | AC-078 |
| Off | The setting is off | "**Off**. Errors still show on Events." with "Events" linking to S-08 | Change; the link | AC-078 |
| Editing | Change | The RadioGroup with the current value checked; focus on the checked radio | Save stores it and closes the form; Cancel closes it unchanged | AC-078; SettingRow |
| Save refused | The server refused | The reason under the radios; the choice kept | Save again | SettingRow |

Turning notices off or on is recorded as an event: "Settings changed: Error notices are now off." / "… now on."

### Component: RoleBlock and RoleEditor

Summary: role name, what the role does, then Agent, Model, Effort, Skills from this repository, Additional instructions (two lines, clamped).

| CLI | Model | Effort |
|-----|-------|--------|
| Claude Code | Text, "Leave empty for Claude Code's default" | Select: Claude Code's default, low, medium, high, xhigh, max (`claude --effort`) |
| Codex | Text, "Leave empty for Codex's default" | Select: the model's default, low, medium, high, xhigh, max (the levels every Codex model offers) |
| OpenCode | Text, `provider/model` | Text: the provider's variant (such as high, max, minimal) |

| State | Trigger | Display | Evidence |
|-------|---------|---------|----------|
| CLI changed | Another agent chosen | "Note: the model and effort set for <old> do not apply to <new>, so they are cleared." | AC-007 |
| Skill not loadable | A selected skill is not in the new CLI's skill directory | "Note: <skill> is not in <dir>, where <CLI> loads skills, so it is unselected." | AC-008 |
| No skills | The directory holds none | "No skills there yet." | AC-008 |

### Component: Setup (S-00)

Title "Set up Oven Spring for <repository>" and the steps 1 Connect, 2 Run the product, 3 Set the goal (done ones ticked). The goal band and the header navigation are absent. Setup opens on the first step not done, from what is saved: no configuration → Connect; how the product runs not settled → Run the product; no goal → Set the goal. Closing the page loses nothing saved.

**Connect.** Blocks GitHub, Issues, Slack and Agents, one Continue.

| Block | Fields | Checked | Evidence |
|-------|--------|---------|----------|
| GitHub | Base branch (`main`), Who merges (the factory / you), GitHub token (optional; else the signed-in `gh`) | When the page opens, and again once a changed value is complete; the result names the repository, the acting account and that the base branch exists | AC-002 |
| Issues | GitHub Issues (default) or Linear (team key, API key) | When the page opens for GitHub Issues; for Linear once both are entered | AC-002 |
| Slack | How to create the app from the manifest (copy the manifest); bot token, app-level token, channel ID, your member ID | Once all four are entered | AC-002 |
| Agents | Claude Code, Codex, OpenCode; none chosen | Not checked: the agent runs as set up on this computer, and a run that fails stops the factory with its own error | AC-060, AC-065 |

A block is checked by connecting with its values (debounced while typing, at once on leaving a field); the newest result wins. The result shows under the block, on a line kept even while there is none, so a check starting as a field is left moves nothing below it: ✓ and what was found, or ✗ with what the service answered and "Check again" (for a fix made outside the page, such as inviting the app). Values are not checked by their form. Tokens and keys are password fields with a Show/Hide toggle. Continue lists what is missing above it (empty fields, a choice of agent, a check that failed), each linked to its block and leaving the list once fixed; checks still answering are waited for, the button reading "Checking…". When nothing is missing it saves the configuration (every role on the chosen agent) and the credentials, and goes to Run the product.

**Run the product.** What it is for: each issue's own copy of the product, which agents check against and the owner previews before approving a held merge.

| State | Trigger | Display | Interaction | Evidence |
|-------|---------|---------|-------------|----------|
| Reading | The lead's `run-commands` step runs | Progress: "The lead is reading the repository", then "Start it once to check"; "What the lead has said so far" (its messages, in order); it takes minutes, and closing the page does not stop it | None | AC-066 |
| Checking | The lead found commands; the factory starts the product once | Progress on the second line with the command running and the ready address awaited | None | AC-066 |
| Started | The check answered | "It runs with these commands" chosen, with ✓ "Started on port <n> and answered after <duration>", the three commands editable; "There is nothing to run" | Continue saves them; an edit is checked again by starting it, the button reading "Checking…" | AC-066 |
| Did not start | The check failed, or the lead could not tell how it starts | "It runs with these commands" chosen, with ✗, where it failed and the last output (or the lead's reading); the commands editable (empty when the lead found none) | Continue starts it again with the commands as they stand | AC-066 |
| Nothing to run | The lead found nothing to run | "There is nothing to run" chosen, with the lead's reading quoted and what it means (tests only, no preview); "It runs with these commands" to enter them | Continue records it | AC-066 |
| The lead's agent stopped | Its launch failed | "The lead's agent stopped with an error." and the agent's own error as it wrote it; the factory is paused; resume with `@<bot name> !resume` in Slack, and the lead reads the repository again | Slack | AC-034, AC-060 |

**Set the goal.** What the goal is for and what Start does (the lead proposes the first sprint toward it in Slack; nothing is worked on until that is approved); Goal (textarea, with an example); Start, which saves it and opens Now. An empty goal shows "Enter the goal."

### Component: Prototype (S-06)

Breadcrumb to the issue; title; "Binding:" or "Reference:" with what it means for the build; "Approve or reply in Slack ↗" with "You approve it in Slack."; the page in a sandboxed frame.

## Slack Messages

Slack is where the owner acts. Every message the factory posts follows the same shape: the conclusion or the question first, in one or two sentences; then each point the owner decides on its own line; then the details, from the first heading on, as a reply in the thread; the PRD (M-11) and the Design Doc (M-03) are attached as files. Messages name issues by key and title, never by internal item ids. A sprint proposal and a sprint review open with the product goal they serve (AC-057).

| ID | Message | Content | Actions | After the action |
|----|---------|---------|---------|------------------|
| M-01 | Sprint proposal | Goal; the sprint's purpose; its key results; its issues in dependency order. With no issue to propose: Goal and what is missing, as a notice | Approve, or reply in the thread; none for a proposal with no issue | Approve starts the sprint; a reply goes to the lead, who revises; with no issue, the factory proposes again once an issue opens |
| M-02 | Question | The issue; what no source decides (behavior, a requirement the code analysis showed could be smaller, what a PRD needs, a technical decision the factory cannot make alone, a review finding that would change the outcome); why it matters; the lead's options and recommendation | Reply in the thread, or Cancel issue | The answer settles the question; the issue continues, and a technical decision is recorded as an ADR. Cancel issue ends the issue (AC-067); a question about the sprint has no Cancel |
| M-03 | Design approval | The issue; what the design adds (external connection, infrastructure, CI, schema, external contract); why; the Design Doc attached in the thread | Approve, or reply | Approval lets the design proceed |
| M-04 | Prototype approval | The issue; the reference strength and why; the link to its prototype page (S-06), on its own line at the end | Approve, or reply | approval lets the UI Spec be written from it |
| M-11 | PRD approval | The issue; that its requirements are written and reviewed; the PRD attached in the thread, which records the owner's answers with their source | Approve, or reply | Approval lets design start; a reply goes to the lead, who revises the PRD, and the revised PRD comes back attached to a new request |
| M-05 | Merge request | The issue; the held categories it includes; the pull request | Approve merge, Hold, and Preview when the product has start commands; under them, "Hold keeps it unmerged until the sprint review." | Preview replies with the local address; Approve merges the exact commit shown; Hold keeps it for the sprint review |
| M-06 | Ready to merge | (`owner` merge setting) The issue; the pull request; the person-review kinds it touches, with their words | Preview when the product has start commands | The factory continues once the owner merges |
| M-07 | Sprint review | Goal; each key result met or not with its evidence; what merged, was held or dropped; changes to working agreements | Close sprint, or reply | A reply about how the factory works goes to the lead, who revises the review; a reply about what to build next also reaches the next proposal. Closing lets the review's agreement and knowledge changes merge and approves nothing else: it ends the sprint and opens their pull request, which merges as the merge setting says (the thread links it and starts with what the owner does: approve it when GitHub requires an approving review, merge it under `merge.by = owner`, or nothing); the next sprint is proposed (M-01) once it has merged, or at once when no agreement changed |
| M-09 | Recovery | The issue or the sprint, and what could not go on; the cause in plain words; what the procedure changes and whether it can be undone; the procedure itself, as it will run | Approve, Resolved, Cancel issue, or reply | Cancel issue ends the issue (AC-067; not on a sprint's recovery); Approve runs exactly the procedure shown, then the factory continues from where it stopped; Resolved tells the factory the owner fixed it, and it continues the same way; a reply goes to the lead, who writes another way forward. When the lead itself could not be launched, the message has no procedure: it names the operation and its error, and offers Resolved, and Cancel issue when it is about an issue |
| M-08 | Paused | What stopped and its cause in plain words, and how to resume; posted when the factory pauses itself (an owner's `!pause` is answered in its own thread) | Mention the bot with `!resume` | Resumes |
| - | Preparation | The issue; what the owner should know or prepare before a design-phase step; the step goes on without waiting | None | - |
| M-10 | Conversation | Thread started by mentioning the bot with anything but a command; the lead answers in the thread | Reply | - |
| M-13 | Error started | (Only while error notices are on.) "**Error: <name, lower-cased after the colon>.** <what failed, with the issue key and the cause>" / "<what the factory does meanwhile>. You will hear once more when it clears." / "See it on Events" (the dashboard's S-08 address). Wording: the events prototype's `#/slack` page | None | Sent once when the error starts; its repeats send nothing |
| M-14 | Error cleared | (Only while error notices are on.) "**Cleared: <clear sentence, lower-cased after the colon>.** It failed <n> times from <time>." Nothing more: the events prototype's "Issues it could not judge meanwhile are judged now." is not used, since a judgment that failed is not run again until the issue changes (`crates/daemon/src/intake.rs`, `start_next_filter_run`) | None | Sent once when the error clears |
| M-12 | Reminder | "Still waiting for you:" and one line per decision delivered and still open, and per issue's pull request ready for the owner to merge, each linking where to answer; sent once an hour while any waits (AC-068) | None | - |

An error notice tells what failed; M-08 tells that the factory stopped and why. When an error's start pauses the factory, M-08 is its one message: it names the error as the cause and how to resume, and no M-13 is posted; the start line reads "You were told in Slack." once M-08 is accepted. Its clear is M-14 while notices are on. Turning error notices off leaves M-08 as it is.

**Delivery.** When Slack does not accept M-13 or M-14, the factory keeps the message and posts it once Slack accepts messages again, once, in the order the errors started and cleared; later failures of the same error add nothing to it. An error that clears while its M-13 still waits keeps it: M-13 is posted first and its M-14 after it, so the owner hears once that it started and once that it cleared (AC-077). Turning notices off drops the waiting messages. The EventLine's told line says which of these happened.

The factory responds only to the owners set in Settings. Approvals are buttons; only owners' presses count (AC-032), and a press by anyone else is answered privately with why it did not count. Commands reach the factory only by mentioning the bot with the command right after the mention, as in `@Oven Spring !pause <reason>`; a message that does not mention the bot is never a command. Commands: `!pause` (no new agent starts; agents already at work finish their step), `!resume`, and `!dashboard` (replies with the dashboard's address); the factory answers each in the command's thread. A command sent inside a thread is answered in that thread. A reply in a decision's thread answers the decision with or without a mention, unless it is a command.

## Design Tokens and Component Map

### Environment Constraints

- Target browsers: current Chrome, Safari and Firefox on macOS and Linux.
- Theme support: light, dark, and system preference; the choice is remembered per browser.

#### Responsive Behavior

| Breakpoint | Width | Key Changes |
|-----------|-------|-------------|
| Rows | < 960px | GlanceRow stacks phase · status · meta under the title |
| Event lines | < 620px | EventLine stacks the subject above the sentence, beside the clock |
| Narrow | < 760px | 16px side padding; settings navigation becomes a wrapped row; two-column definition lists become one column |
| Default | ≥ 760px | Content column 1200px, centered; 40px page side padding |

### Existing Component Reuse Map

| UI Element | Decision | Existing Component | Notes |
|-----------|----------|-------------------|-------|
| AppHeader, Wordmark | Extend | `components/AppHeader`, `components/Wordmark` | Navigation Now, Activity, Events, Sprints, Settings (an `events` area, current on S-08); the theme in the header |
| Contact | Extend | `components/Contact` | The lost-contact line becomes two sentences on every page |
| EventsPage, ErrorsNow, EventFilters, EventLine | New | Page under `app/events/`, using `api/usePoll`, `components/Contact`, `components/ExternalLink`, and Activity's way of keeping filters in the URL query and its select styling | The filter row's layout and spacing and the event line grid are the page's own CSS from the events prototype, not Activity's `.filters` grid |
| SprintRow, This sprint group | Extend | `app/sprints/SprintsView.tsx` | Each sprint carries the anchor `sprint-<id>` that S-08's "Sprint" link opens |
| ErrorNoticesRow | Reuse | `components/SettingRow`, `components/Choices` (RadioGroup, legend visible) | Same form as Who merges |
| GoalBand | Reuse | `components/GoalBand` | Links to S-07 to rewrite the goal |
| Button, Field, Choices, Warning | Reuse | `components/*` | Selects, radios and checkboxes drawn from tokens (no operating-system defaults) |
| SettingRow, RoleEditor, AgentsSettings | Reuse | `components/*` | Per-CLI model and effort |
| DownloadPanel, ActionPanel | Reuse | `components/*` | On S-02 and S-03 |
| FactoryStatus, SprintLine, GlanceRow, IssueNow, PhaseItem, RunLine, RunPanel, CancelPanel, SprintRow | Reuse | `components/*`, `app/*` | As specified above |

### Design Tokens

#### Color Roles

| Role | Token | Light | Dark | Usage |
|------|-------|-------|------|-------|
| Background Surface | `color-bg` | `#f7f3ec` | `#16110d` | Page |
| Background Surface | `color-raised` | `#fffdf9` | `#1f1813` | Goal band, fields |
| Border | `color-line` | `#e4dccf` | `#30271f` | Hairlines between items |
| Border | `color-field-border` | `#8a7c6b` | `#7a6c5c` | Field and button outlines |
| Text | `color-text` | `#2a1f16` | `#f2e9dc` | Text |
| Text | `color-text-muted` | `#6e6152` | `#ab9d8b` | Labels, keys, phases, secondary lines, the select caret |
| Brand / Accent | `color-accent` | `#9c4416` | `#f0a25a` | Links, ● and what is yours, primary button, focus, checked controls |
| Brand / Accent | `color-on-accent` | `#fffdf9` | `#16110d` | Text on the primary button, the checkbox tick |
| Status | `color-ok` | `#3f6b33` | `#a3c98f` | Pass, applied, met, connected |
| Status | `color-fail` | `#a3301c` | `#f08a72` | ▲ problems, halting states, failed runs, not met, warnings |

The run panel dims the page behind it, and the prototype frame's background is white; no other colour appears.

#### Typography Hierarchy

| Role | Font | Size | Weight | Line Height |
|------|------|------|--------|-------------|
| Page title (every page, including FactoryStatus) | Hanken Grotesk | 30px | 600 | 1.2 |
| Group heading | Hanken Grotesk | 22px | 600 | 1.25 |
| Subgroup heading, phase name | Hanken Grotesk | 17px | 600 | 1.3 |
| Lead (goal, status lines, issue now, sprint line) | Hanken Grotesk | 17px | 400 | 1.55 |
| Body | Hanken Grotesk | 15px | 400 | 1.55 |
| Small | Hanken Grotesk | 14px | 400 / 600 | 1.55 |
| Monospace (keys, models, commands, paths, times) | IBM Plex Mono | 13px | 400 | 1.55 |
| Wordmark | SVG | 18px high | - | - |


#### Spacing Scale

| Token | Value | Usage |
|-------|-------|-------|
| `space-1` | 4px | Inside an item |
| `space-2` | 8px | Inside an item; item side padding; heading to its prose |
| `space-3` | 12px | Between items (with a hairline), form gaps |
| `space-4` | 16px | Column gaps, narrow page padding |
| `space-5` | 24px | Heading to its list; between subgroups |
| `space-6` | 40px | Between groups; page side padding |
| `space-7` | 64px | Between page regions (FactoryStatus to the sprint line) |

#### Elevation and Radius

Flat everywhere. `radius-control` (4px) on fields, buttons and the boxes below. A box (1px `color-line`) surrounds the prototype frame on S-06, the CancelPanel, typed commands (`kbd`) and command output. Selection is one signal: the current page in the navigation is bold text colour; others muted.

The wordmark is the `Wordmark` component, drawn in `color-text`; the favicon is `dashboard/public/icon.svg`.

## Visual Acceptance

### Golden States

1. **S-01 Working, light**: one decision waiting, two in progress, two waiting to start, two done.
2. **S-01 Problem, light**: ▲ Problem on one row, the rest unchanged.
3. **S-01 Paused, dark**.
4. **S-01 Two prerequisites missing**.
5. **S-02 in Review** with the current phase open.
6. **S-04** with the run panel open on a running run.
7. **S-05 Agents** with a role's editor open on Codex.
8. **S-00 Connect** after an empty Continue, and **S-00 Run the product** with the product started.
9. **S-08 Two errors going on, light**: ErrorsNow with two errors; lines with a folded start, a clear, two drops and a filter failure.
10. **S-08 Nothing wrong now**: both errors cleared.
11. **S-08 Error notices off, dark**: the notice line in ErrorsNow; "Not told: error notices are off." on lines after the switch.
12. **S-08 Nothing recorded yet**.
13. **S-08 The events could not be read**.
14. **S-08 Lost contact**.
15. **S-05 Connections** with the Error notices row open.

### Layout Constraints

- Content column at most 1200px; header and goal band contents align with it.
- The goal band appears under the header on every screen but Setup, with identical content and position.
- Every page starts with its page title at the same position and size.
- No horizontal page scroll at any width from 320px.
- Variable-length values (titles, models, skill names, paths, instructions) wrap inside their own column; column start lines stay aligned.

## Accessibility Requirements

### Keyboard Navigation

| Component | Tab Order | Key Binding | Behavior |
|-----------|-----------|-------------|----------|
| AppHeader | 1 | Enter | Follows the link; theme select; the update button opens or closes its steps (`aria-expanded`, `aria-controls`) |
| GoalBand | 2 | Enter | Rewrite → S-07 |
| GlanceRow | in order | Enter | Key/title → S-02; activity → D-01; action → Slack |
| PhaseItem, SprintRow, DownloadPanel, findings | in order | Enter / Space | Expands or collapses |
| RunPanel | on open | Escape | Closes; focus returns to the page |
| SettingRow | in order | Enter | Change opens the form and focuses its first field |
| EventFilters | Show, then Issue | Arrow keys / Enter | Changes the filter; the list updates without moving focus |
| EventLine, ErrorItem links | in order | Enter | Key → S-02 (only a key the factory has a record of is a link); Sprint → that sprint on S-03 when S-03 lists it, named as EventLine says ("Sprint: <purpose>" or "Sprint begun <date, clock>") to screen readers; a sprint S-03 does not list is not a link and needs no key, since its name is visible text; tracker link → new tab |
| "Show older events" | after the last line | Enter / Space | Shows older lines; focus stays on the button, or moves to the first new line when the button goes away |
| ErrorNoticesRow | in order | Arrow keys, Enter | Arrow keys move between On and Off; Save with Enter |

### Screen Reader

| Component | Role | Accessible Name | Live Region |
|-----------|------|-----------------|-------------|
| GoalBand | region | "Product goal" | none |
| FactoryStatus | status, heading level 1 | its title | polite (on change) |
| GlanceSection | section | its heading | none |
| ● / ▲ marks | hidden | (the word beside each carries the meaning) | none |
| RunPanel | dialog (modal) | the run's sentence | none |
| External links | link | its text and "(opens in a new tab)" | none |
| ErrorsNow, What the factory did | section | its heading | none |
| DayGroup | section | its day heading (h3) | none |
| Event lists | list | - | none (new lines are not announced) |
| Error / Cleared | text | the word itself | none |
| Error notices radios | radiogroup in a fieldset | the visible legend "Error notices" | none |

### Contrast Requirements

| Element | Foreground | Background | Ratio Target |
|---------|-----------|------------|-------------|
| Body text | `color-text` | `color-bg`, `color-raised` | 4.5:1 (lowest measured 14.5:1 light, 14.5:1 dark) |
| Muted text | `color-text-muted` | `color-bg`, `color-raised` | 4.5:1 (lowest measured 5.4:1 light, 6.6:1 dark) |
| Accent text and links | `color-accent` | `color-bg`, `color-raised` | 4.5:1 (lowest measured 5.8:1 light, 8.3:1 dark) |
| Status text | `color-ok`, `color-fail` | `color-bg`, `color-raised` | 4.5:1 (lowest measured 5.6:1 light, 7.1:1 dark) |
| Primary button text | `color-on-accent` | `color-accent` | 4.5:1 (measured 6.3:1 light, 8.9:1 dark) |
| Field borders | `color-field-border` | `color-bg`, `color-raised` | 3:1 (lowest measured 3.6:1 light, 3.4:1 dark) |

## Copy on Screen

The dashboard's sentences are written for the owner as reader: a sentence stays when it changes what the owner does or understands at that place, or when leaving it out would make the rest misleading or unsafe. How the factory works, and what does not change the owner's choice, goes to the guide (README, GUIDE.md). Sentences are split rather than joined with semicolons. What the owner types (a Slack command, a terminal command) is set apart as typed text; identifiers and paths are in the mono face.

### Explained in the guide, not on screen

These are explained in `docs/GUIDE.md`:

- OpenCode has no hook: its own permissions deny the same commands the guard stops on Claude Code and Codex (pushes, merges, approvals and the owner's denied commands), and the paths the hook protects.
- Under Bypass the guard still holds on every CLI: the hook on Claude Code and Codex, the permissions on OpenCode.
- What the download holds: every record in time order, one per line: runs (with usage and cost), state changes, decisions and approvals with who gave them in Slack, each step's result, commits and merges. Tokens and other secrets are masked.
- How the product's commands are found at setup: the lead reads the repository, and the factory starts the product once to make sure.
- A goal change is kept in the records the downloads hold.
- A service that stops answering after it was connected shows on Now.
- The factory merges the exact commit that passed, once checks, required approvals and the owner's approvals for held changes are in.

## Open Items

- Messages (S-05) waits on a change to the factory's rules (the language of messages to the owner); the prototype shows it. Until then it is not shown.

## Update History

| Date | Version | Changes | Author |
|------|---------|---------|--------|
| 2026-11-21 | 1.0 | Initial version | Oven Spring team |
| 2026-11-24 | 1.1 | Copy on Screen: copy cut to what changes the owner's action; moved explanations listed for the guide; typed commands set apart; the resume mention uses the bot's name from Slack | Oven Spring team |
| 2026-11-25 | 1.2 | AppHeader: the version and its update notice (AC-071) | Oven Spring team |
| 2026-11-25 | 1.3 | Reconciled with the implementation: Slack messages, Now and issue page states, RunLine, the row breakpoint, boxes and colours, the component map | Oven Spring team |
| 2026-11-27 | 1.4 | PhaseItem leaves out a preparation turn that ended normally; RunLine words a preparation turn by what it came to (GH-10) | Oven Spring team |
| 2026-11-27 | 1.5 | GH-9: the Events page (S-08), error words, the Error notices row, M-13 and M-14, the two-sentence lost-contact line; the events prototype attached as binding | Oven Spring team |
| 2026-11-27 | 1.6 | GH-9 review: repeats fold per issue and cause; keys without a factory record are not linked; Sprint links to its sprint; tracker wording from the record; removing the filter clears its error; retry intervals per service; one message when an error pauses; delivery of messages Slack refused; the filter row's prototype layout | Oven Spring team |
| 2026-11-27 | 1.7 | GH-9 review: the error views of Show list Error repeat lines; a sprint without a purpose is named by when it was begun, and one S-03 does not list is not linked; GitHub's retry line promises no interval; a waiting M-13 is posted before its M-14, not dropped | Oven Spring team |
