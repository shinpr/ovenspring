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
- Feature scope: AC-029 to AC-035 (Slack), AC-002, AC-043 to AC-050, AC-053, AC-056, AC-057, AC-059 and AC-060 (dashboard, setup and goal), and the owner-facing parts of AC-007 to AC-012, AC-027, AC-031, AC-037, AC-052 and AC-058.

### Design Source

| Source | Path | Version |
|--------|------|---------|
| Prototype code | `docs/ui-spec/assets/dashboard/index.html` (one page) | The layout and wording reference; this document governs where they differ |

## Prototype Management

- **Attachment path**: `docs/ui-spec/assets/dashboard/`
- **Version identification**: file date; later changes to the dashboard's look update the file first.
- **Compliance premise**: the dashboard's tokens (`dashboard/tokens/tokens.json`); no external design system.
- **Reference strength**: `binding` for structure, layout, spacing, vocabulary and copy tone; data, names and numbers are fictional.
- **Not part of the product**: the prototype's top control bar (factory situation, Explain each element), its explanation notes and its "How this was designed" page.
- **Relationship to canonical spec**: the prototype decides structure, layout, spacing and the words on screen; where this document paraphrases on-screen copy, the prototype's wording applies. This document decides behavior, states, triggers and data, and wins where the two differ on those.

The Slack surface has no prototype; its messages are specified in Slack Messages below.

## External Resources Used

| Resource (project-tier label) | Feature-specific identifier | Notes |
|-------------------------------|-----------------------------|-------|
| Design Origin | `docs/ui-spec/assets/dashboard/index.html` | This specification's prototype |
| Design System | None; tokens in `dashboard/tokens/tokens.json` | Fonts: Hanken Grotesk, IBM Plex Mono (SIL OFL), bundled in the binary |
| Visual Verification Environment | Storybook: stories for the golden states and the main component states in this document | `dashboard/` |

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

### Transition Conditions

| Source | Destination | Trigger | Guard Condition |
|--------|------------|---------|-----------------|
| Start | S-00 | `ovenspring start` opens the dashboard | Setup is not finished; it opens on the first step not done |
| S-00 | S-01 | "Start" on Set the goal | The goal is saved |
| Any | S-01, S-04, S-03, S-05 | Header link | Set up |
| Any | S-07 | Goal band link | Set up |
| S-01 | S-02 | Issue key or title | - |
| S-01 | D-01 | The activity of an issue whose step has a run | A run is under way |
| S-01 | Slack | A Waiting for you action | Decision delivered |
| S-01 | GitHub | "Merge on GitHub" or "Approve on GitHub" | `merge.by = owner`, ready; or GitHub requires the owner's approving review on the agreements pull request |
| S-01 | S-05 section | A prerequisite's link | The prerequisite names a setting |
| S-02 | S-04 filtered by the issue | "See them in Activity" | - |
| S-02 | S-06 | "Prototype" | The issue has a prototype |
| S-07 | S-01 | Save | The text changed and is not empty |

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
| Any | ▲ Problem: <cause> (the lead's exception; cause: "<step name> stopped", such as "Code review stopped", "dependency dropped", "PR closed", "push refused", "<operation> failing") · On hold (paused) |

### Waiting for you (actions)

Answer a question · Approve PRD · Approve prototype · Approve design · Approve merge · Merge on GitHub · Approve on GitHub · Approve recovery · Resolve a stop · Approve sprint proposal · Close the sprint.

### Waiting to start (reasons)

"After <KEY> merges" (the first dependency not merged; "After <KEY> and n more merge" for several) · "When an issue in progress ends" (Issues at once in Settings is reached).

### Done (outcomes)

Merged · Closed: already done (the lead closed it with evidence) · Dropped (the lead dropped it from an exception) · Canceled by you (the owner canceled it) · Held for the review.

### Time

A duration ("3 min", "2 h 5 min") is how long an issue has been in its current activity. A clock time ("12:48") is when it ended, with its date ("Nov 20, 21:00") when that was on an earlier day, since a sprint spans days. Waiting for you and Waiting to start show no time.

## Component Decomposition

### Component Tree

```
App
  +-- AppHeader (wordmark, version and its update notice, repository, navigation Now · Activity · Sprints · Settings, theme)
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
      +-- RunPanel (D-01)
      +-- Settings (S-05): SettingsNav, Group per section > SettingRow / RoleBlock (RoleEditor)
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
| In progress | Phase word (muted) | Activity; ▲ Problem: <cause> in `color-fail`; On hold (paused) muted | Duration | Activity with a running run → D-01; key and title → S-02 | AC-043 |
| Waiting to start | - | Reason (muted), a dependency's key links to it | - | → S-02 | AC-030 |
| Done | - | Outcome (Merged in text colour, the others muted) | Clock time, with the date for an earlier day | → S-02 | AC-041 |

Empty sections: "Nothing is in progress." and "No issue has ended yet."; Waiting for you and Waiting to start are omitted when empty. At widths below 960px a row is key and title on the first line, then phase · status · meta under the title; five columns leave a title too little room there.

### Component: IssueNow (S-02)

| State | Display | Evidence |
|-------|---------|----------|
| In progress | "<Who> <activity sentence> · for <duration>" and "follow" when a run is under way; "GitHub won't merge it yet." and "See why on the pull request ↗" while GitHub blocks the merge; "The lead is handling a problem: <cause>" (`color-fail`) during an exception; "· on hold while the factory is paused" while paused | AC-044 |
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

### Component: RunLine

The time (mono): `<start>–<end>` on S-02; on S-04 the end time (`<start>–` while running) and the issue's key · "<Role> <what it did>", "· for <duration>" while running, and "→ <result>" (`color-ok` for pass, `color-fail` for a failure, muted otherwise) · "open run" (or "follow" while running).

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
| M-12 | Reminder | "Still waiting for you:" and one line per decision delivered and still open, and per issue's pull request ready for the owner to merge, each linking where to answer; sent once an hour while any waits (AC-068) | None | - |

The factory responds only to the owners set in Settings. Approvals are buttons; only owners' presses count (AC-032), and a press by anyone else is answered privately with why it did not count. Commands reach the factory only by mentioning the bot with the command right after the mention, as in `@Oven Spring !pause <reason>`; a message that does not mention the bot is never a command. Commands: `!pause` (no new agent starts; agents already at work finish their step), `!resume`, and `!dashboard` (replies with the dashboard's address); the factory answers each in the command's thread. A command sent inside a thread is answered in that thread. A reply in a decision's thread answers the decision with or without a mention, unless it is a command.

## Design Tokens and Component Map

### Environment Constraints

- Target browsers: current Chrome, Safari and Firefox on macOS and Linux.
- Theme support: light, dark, and system preference; the choice is remembered per browser.

#### Responsive Behavior

| Breakpoint | Width | Key Changes |
|-----------|-------|-------------|
| Rows | < 960px | GlanceRow stacks phase · status · meta under the title |
| Narrow | < 760px | 16px side padding; settings navigation becomes a wrapped row; two-column definition lists become one column |
| Default | ≥ 760px | Content column 1200px, centered; 40px page side padding |

### Existing Component Reuse Map

| UI Element | Decision | Existing Component | Notes |
|-----------|----------|-------------------|-------|
| AppHeader, Wordmark | Reuse | `components/AppHeader`, `components/Wordmark` | Navigation Now, Activity, Sprints, Settings; the theme in the header |
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

### Screen Reader

| Component | Role | Accessible Name | Live Region |
|-----------|------|-----------------|-------------|
| GoalBand | region | "Product goal" | none |
| FactoryStatus | status, heading level 1 | its title | polite (on change) |
| GlanceSection | section | its heading | none |
| ● / ▲ marks | hidden | (the word beside each carries the meaning) | none |
| RunPanel | dialog (modal) | the run's sentence | none |
| External links | link | its text and "(opens in a new tab)" | none |

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
