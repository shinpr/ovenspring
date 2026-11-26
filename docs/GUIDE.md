# Oven Spring guide

This guide is for the owner: the person who points Oven Spring at a repository, sets its product goal and makes the decisions it brings to them. Setup records you as the owner, and you can add more in Settings. The factory answers every owner and counts each one's approval the same way, so "you" below means any of them.

The guide covers what to prepare, the setup and where it tends to go wrong, what a sprint looks like from your side, and the safeguards around the agents. The [README](../README.md) has the overview and the install command.

## Before you start

No one has to review the factory's pull requests before they merge. Two things in the repository therefore decide how far you can trust what it merges.

**End-to-end tests among the required checks.** The merge gate waits for the checks the base branch requires and adds none of its own. Without end-to-end tests, the gate cannot catch a regression that none of the other checks cover, such as a change that breaks how a page, its API and the database work together. If the base branch requires no checks at all, the gate has nothing to wait for. Set up CI with end-to-end tests as a required check before the first sprint.

**Production credentials kept off the machine.** The [guard](#the-guard) stops honest mistakes, such as an agent running a deploy command you denied. It cannot stop an agent that finds another way to reach production, so keep production credentials where the factory's machine can't use them.

Two more things are worth doing.

**Write down what the product must keep true.** Agents weigh every change, test and review finding by the harm a failure would do to the people who use the product, in the order the repository's agent instructions file (`AGENTS.md` or `CLAUDE.md`) gives. Without one, they weigh by direct harm to users. A short section is enough:

```markdown
## What This Product Must Keep True

Weigh every change, test, and review by the harm a failure does to the people who
use it, in this order:

1. Exposure: nobody sees what they must not, nobody acts as someone else, and
   nothing that authenticates appears in logs or responses.
2. Stored data: what people saved is not lost, or overwritten by anyone but its
   owner.
3. What others rely on: links, APIs and integrations already in use keep working.
4. Daily use and appearance.
```

**Keep your existing gates.** Branch rules, required approvals, review bots and command-blocking hooks all apply to the factory's pull requests. The factory waits for the approvals the rules require and for the review bots you list in Settings, and answers reviewers the way it answers its own reviews.

The factory runs on your machine, one background process per repository, and works while that machine is awake. After a reboot, run `ovenspring start` again in the repository and the work picks up where it stopped.

## Setting up

Run `ovenspring start` in the repository. The dashboard opens on setup, which has three steps: Connect, Run the product, and Set the goal. Closing the page loses nothing you saved, and the next `ovenspring start` opens the first step not yet done.

On a remote machine, `ovenspring start` prints the dashboard's address instead of opening it. The dashboard listens on `127.0.0.1` only, so reach it through an SSH tunnel.

### Connect

Connect has four blocks. GitHub, Issues and Slack are checked against the real service as you fill them in, and a ✗ shows what the service answered. After fixing something outside the page, such as inviting the Slack app to a channel, press **Check again**.

**GitHub.** The base branch (where issue branches start and merge), who merges (the factory, or you), and a token. Leave the token empty to use the token from your `gh` login (`gh auth login`).

- The repository's `origin` must be a GitHub address, `git@github.com:owner/name` or `https://github.com/owner/name`.
- A classic token needs the `repo` and `workflow` scopes. Without `workflow`, the factory can't push an issue that changes `.github/workflows`.
- A fine-grained token needs, on this repository: **Contents**, **Pull requests** and **Issues** with read and write, **Checks** and **Commit statuses** with read, and **Workflows** with read and write. **Administration** with read is optional: with it the factory also reads classic branch protection, and without it GitHub's own merge check still holds back a pull request the protection blocks. No Actions permission is needed.
- The token's account opens the pull requests and pushes the branches. If your branch rules require an approval from someone other than the author, use a token of an account that doesn't approve, such as a machine account. Otherwise nobody can give the approval the rule needs.

**Issues.** GitHub Issues (the repository's own) or Linear (a team key and an API key).

- A Linear API key needs read and write access to the team: the factory moves issues between states and comments on them.
- With Linear, the factory reads the team's issues in unstarted and started states, such as Todo and In Progress. Issues in Backlog or Triage are not read, so move an issue to Todo when it is ready for the factory.
- With GitHub Issues, every open issue is a candidate. The lead takes into a sprint only the issues its purpose needs.
- Dependencies come from the tracker: Linear's "blocked by" relations, and GitHub's blocked-by relationships between issues. An issue waits until the issues it depends on have merged.

**Slack.** Oven Spring talks to Slack over Socket Mode, so it needs no public address.

1. Copy the manifest from the setup page and create a Slack app from it (Create New App, From an app manifest).
2. Install the app to the workspace, and copy the bot token (`xoxb-`) from OAuth & Permissions.
3. Under Basic Information, generate an app-level token (`xapp-`) with the `connections:write` scope.
4. Copy the ID of the channel the factory should use (in the channel's details), and invite the app there with `/invite @Oven Spring`. A private channel works too.
5. Copy your member ID from your Slack profile.

"The app is not in channel …" means the invitation in step 4 is missing.

**Agents.** Choose Claude Code, Codex or OpenCode. Every role starts on that CLI, and you can give each role a different one later in Settings. The CLI runs as it is set up on this machine, so install it and sign in before you continue. Setup doesn't test it: a run that fails pauses the factory and shows the CLI's error.

### Run the product

If the repository is a library or a command-line tool, or the product can't run on this machine (it needs credentials for a shared staging service, for example), choose **There is nothing to run**. Agents then check their work with tests only, and merge requests offer no preview.

Otherwise each issue gets a separate copy of the product in its worktree. Agents check their work against it, and you open it from Slack before approving a held merge. In this step the lead reads the repository (its README, package manifests, compose files, CI workflows) and proposes three commands, and the factory starts the product once with them to make sure they work. This takes a few minutes, and closing the page doesn't stop it.

| Command | What it does |
|---|---|
| Setup | Prepares a fresh worktree, for example by installing dependencies |
| Start | Starts the product, listening on the port in `$PORT` |
| Ready | A URL that responds once the product is ready, such as `http://localhost:$PORT/health` |

Each copy gets its own `PORT` and data folder (`OVENSPRING_DATA_DIR`), so several issues can run their copies side by side. A product that keeps its local database or files in a fixed place will collide with itself. Point that place at `OVENSPRING_DATA_DIR`.

When the check fails, the page shows where and the last output. Edit the commands and continue to try again.

If the lead's agent fails here, the factory pauses and the page shows the CLI's error. Fix the cause (usually signing in to the CLI), then mention the bot in Slack with `!resume` ([commands](#talking-to-the-lead)), and the lead reads the repository again.

### Set the goal

Write where the product is heading, in a few sentences. The lead plans every sprint from it, so describe the state you want the product or its users to reach. A list of tasks gives the lead nothing to choose by. For example: "Shop owners can run a sale without asking us: they set the discount, the dates and the products themselves, and customers see the right price everywhere."

**Start** saves the goal and opens Now. Work starts when you approve the first sprint proposal in Slack. You can rewrite the goal any time from the band at the top of every dashboard screen. The next proposal uses the new text, and the running sprint carries on.

## A sprint from your side

1. The lead proposes a sprint in Slack: the goal it serves, one purpose, the issues it needs in dependency order, and the key results that will show whether the purpose was reached. Approve it, or reply in the thread and the lead revises it. When no open issue fits, you are told what is missing, and the lead proposes again once an issue appears.
2. Issues run in parallel, up to three at once by default. While they run, your decisions reach you in Slack, and only the work that depends on a decision waits for it.
3. Each issue ends in one of four ways:
   - **Merged.**
   - **Closed, already done.** The code already did what the issue asks, and the lead closed it with the evidence.
   - **Dropped.** The lead judged the issue can't go on, from an [exception](#when-something-goes-wrong). It posts the reason on the tracker and marks the issue canceled.
   - **Held for the review.** You pressed **Hold** on its merge request. The pull request stays open, and a later sprint can take the issue up, which puts it back at the merge gate and asks for your approval again.
4. When every issue has ended, the sprint review arrives: each key result met or not with its evidence, what merged, was held or dropped, and the proposed changes to the working agreements and the knowledge. Reply with what you think, or press **Close sprint**.

A reply about how the factory works revises the review. A reply about what to build next reaches the next proposal.

**Close sprint** ends the sprint without approving the next one. It opens a pull request with the agreement and knowledge changes. Those changes are [always held](#when-the-factory-merges) for you, and closing the sprint counts as that approval. With `merge.by = "factory"`, the pull request merges once its checks pass. With `merge.by = "owner"`, the thread links it for you to merge on GitHub. The next sprint is proposed after it merges, or at once when nothing changed.

### Working agreements and knowledge

Both are how the factory learns from one sprint to the next, and both live in the repository: in `.ovenspring/agreements/` and `.ovenspring/knowledge/` unless you set other folders in Settings. Runs read them from the base branch and each sprint review commits to them, so the folders you choose must be ones git tracks. That is also how you keep `.ovenspring/` out of git. Point both folders elsewhere, ignore `.ovenspring/`, and the configuration stays on your machine.

**Working agreements** are what the factory learned about its own work: one file shared by every role, and one per role. Every run reads the shared file and its role's file from the base branch. At each sprint review the lead goes through every agreement and asks whether the problem it answers came up. An agreement that didn't prevent its problem gets rewritten, or moved to the file of the role that makes the choice it is about. One whose occasion never arose is dropped when it was about a single event. A new agreement is added only for a gap the sprint showed and nothing else covers, because every agreement is one more thing each run has to keep in mind. An agreement is a line or two:

```markdown
<!-- .ovenspring/agreements/executor.md -->
- Run `pnpm typecheck` before the test suite: in this repository a type error
  surfaces as unrelated test failures first.
```

**Knowledge** holds facts about the product, its code or its environment that change how the factory acts there and that the code and documents don't show: a quirk of the deploy target, why a module is shaped the way it is, what you told the lead in a thread. It is a folder of Markdown notes, and runs read the ones that bear on a choice before making it:

```markdown
<!-- .ovenspring/knowledge/search-index.md -->
# Search sees product edits only after the nightly rebuild

Edits reach the search index at the 02:00 rebuild (`jobs/reindex.ts`). A test
that edits a product and then searches for it must run the rebuild itself.
```

Each review keeps a note that the sprint's work read and found true, rewrites one that proved wrong or incomplete, and drops one whose subject no longer exists.

### What reaches you in Slack

| Message | When | How you answer |
|---|---|---|
| Sprint proposal | No sprint is running | **Approve**, or reply |
| Question | Product behavior that nothing in the repository decides, a requirement the code shows could be smaller, or a review finding that would change the outcome | Reply in the thread, or **Cancel issue** |
| PRD approval | An issue contains several independently valuable outcomes; the PRD is attached | **Approve**, or reply |
| Prototype approval | An issue adds a new screen; the message links to the prototype on the dashboard | **Approve**, or reply |
| Design approval | A design adds an external connection or changes infrastructure, CI, the schema or an external contract; the design is attached | **Approve**, or reply |
| Merge request | A pull request touches a held category or a change a person reviews | **Preview**, **Approve merge**, or **Hold** |
| Ready to merge | You merge (`merge.by = "owner"`) and a pull request is ready | **Preview**, then merge on GitHub |
| Recovery | The way forward needs a command the agents may not run | **Approve**, **Resolved**, **Cancel issue**, or reply |
| Stopped | An issue or the sprint stopped, and the lead could not be started to find the way forward | **Resolved** once it is fixed, or **Cancel issue** |
| Paused | The factory paused itself | `!resume` |
| Preparation | A design step found something you should know or prepare, and the work goes on | Nothing to answer |
| Sprint review | Every issue of the sprint has ended | **Close sprint**, or reply |
| Reminder | Once an hour for items that have waited at least an hour | Follow its links |

**Preview** starts the issue's copy of the product and replies with its local address, so you can check the behavior without reading the diff. **Approve merge** approves that exact commit; if the branch changes afterwards, the factory asks again. **Hold** keeps the pull request unmerged until the sprint review.

A recovery shows the procedure exactly as it will run, what it changes, and whether it can be undone. **Approve** runs that procedure and nothing else. If you fixed the problem yourself, press **Resolved** instead. A reply asks the lead for another way.

### Talking to the lead

Mention the bot in the channel to talk with the lead about anything; it answers in the thread. The lead is the only agent that speaks to you. Commands work the same way, as a mention followed by the command, and the answer comes in the thread:

- `@Oven Spring !pause [reason]`: agents already at work finish their step, and nothing new starts.
- `@Oven Spring !resume`: work starts again.
- `@Oven Spring !dashboard`: the dashboard's address.

Use the bot's name as your workspace shows it: the name on the factory's messages in the channel.

### The dashboard

The dashboard shows state, settings and records. Approvals and answers happen in Slack, and where one waits, the dashboard links to its message. On the dashboard itself you rewrite the goal, change settings and cancel issues.

- **Now**: whether the factory is working or paused and why, what waits on you, how far the sprint has come, and what each issue is doing.
- **Issue**: the issue's outcome and non-goals, where it is in Requirements, Design, Build, Review and Merge, every review finding with how it was answered, its runs and their cost, and **Cancel issue…**.
- **Activity**: every agent run, filterable by sprint and issue, with its model, duration, cost and log.
- **Sprints**: the current sprint and the ended ones, with their key results and cost.
- **Settings**: everything in `.ovenspring/config.toml`, plus the credentials.

A run's cost is its tokens at API prices, worked out the same way on every CLI. A run on a model the factory has no price for shows none.

The dashboard keeps the same address across restarts while its port stays free, so a bookmark keeps working. To find it again, mention the bot with `!dashboard`, or run `ovenspring start`, which reopens it when the factory is running.

## How an issue travels

The lead first reads the issue as its requester wrote it and defines one observable outcome, the non-goals, and each requirement with its source. It then judges the issue's scale from the code. If the code already does what the issue asks, the lead closes the issue with that evidence.

| Scale | When | What runs |
|---|---|---|
| Small | The change and its reach are clear from the code | Implementation, then review |
| Medium | The change needs a design | The lead writes a design, a reviewer checks it against the issue, the relevant code and existing designs, the lead plans tasks, and the executor implements them one at a time, each ending in a check that proves it |
| Large | The issue contains several independently valuable outcomes | A PRD that you approve, then the same as Medium |

Design documents are written in the issue's branch and merge with the code in one pull request. If the design conflicts with another design, the author fixes the conflict or records why the two agree. The factory commits the design and asks for approval if the change requires it. When implementation reveals a mistake in the design, both are corrected on the issue branch.

Agents don't commit. The guard stops `git commit` and `git push`, and the factory commits at fixed points, including when a document review ends and when the implementation passes the repository's quality checks.

A code review and a security review run on every issue, as separate runs. The executor judges each finding against the issue and records what it fixed or why it declined the change. A finding that calls for a design change goes back to the author. Comments on the pull request get the same treatment when they come from an account with write access to the repository or from a review bot listed in Settings. Each gets a reply in its thread, with the commit that applied it or the reason it was declined. Comments from anyone else get no response and change nothing.

When you list any review bot in Settings, the factory waits after CI passes for the two bots it can track, Claude Code Review and Codex. It waits up to 30 minutes for one that is running. One that shows no sign of running within 3 minutes counts as not installed for that push. With no bot listed, there is no wait.

## When the factory merges

With `merge.by = "factory"`, the gate merges one pull request at a time, and only when all of these hold for its current head commit:

- every check reported on that commit finished and succeeded, including every required one
- the branch contains the base branch's head
- GitHub reports no required approval missing, and no reviewer's latest review requests changes
- GitHub doesn't report the merge as blocked
- you approved that exact commit for each held category it touches and each change a person reviews that it falls under

Right before merging, the gate reads the base and the head again. If either moved, it decides again.

**Held categories** are changes you approve before they merge: `schema`, `external-connection`, `infrastructure`, `ci` and `external-contract`. A change falls into one when its design declared it, or when a changed path matches the patterns you set for that category in Settings. A change under `.ovenspring/` at the repository root, in the agreements or knowledge folder, or to an `AGENTS.md` or `CLAUDE.md` at any depth, is always held. For the pull request a sprint review opens, [Close sprint](#a-sprint-from-your-side) is that approval.

**Changes a person reviews** cover what paths can't name. Under Merging in Settings, describe each kind in your words, for example `payments: changes to how prices, discounts or refunds are calculated`. The design and the code review judge which kinds a change falls under, and such a change waits for your approval of its commit as a held one does.

With `merge.by = "owner"`, the factory never merges. It tells you in Slack when a pull request is ready, naming each change a person reviews that it falls under, and continues once you merge it on GitHub.

## When something goes wrong

When an issue or the sprint can't go on, the lead handles it, and you hear about it when it needs you. Unrelated work keeps going.

A step that fails, is blocked, or reaches a round limit you set becomes an **exception**: the lead reads what happened, looks for the cause at the level it sits (the issue's outcome, the design, the plan, the code or the environment), and then retries, replans, redesigns, drops the issue, or asks you.

- **The provider is overloaded.** The run is retried a few times.
- **Any other agent CLI failure**, such as a usage limit or an expired sign-in. A failure like this usually repeats on the next run, so the factory pauses itself and tells you why in Slack. Fix the cause, then `!resume`.
- **The way forward needs a command agents may not run.** The lead writes a recovery procedure and you decide on it in Slack ([Recovery](#what-reaches-you-in-slack)).
- **You act outside the factory.** Merging or closing a pull request yourself, pushing to its branch, or repairing the environment is fine. The factory reads the current state and continues from there.
- **You want an issue gone.** Cancel it from its page on the dashboard, or with **Cancel issue** on a question, recovery or stop about it in Slack. The factory stops its work, closes the pull request but keeps the branch, and records the cancellation on the tracker, with your words when you cancel from the dashboard.
- **Reviews or CI fixes keep repeating.** Rounds have no limit by default. Set **Round limits** in Settings, and reaching one makes it an exception.

`ovenspring stop` ends the agents at work and the issues' running copies of the product. On the next `ovenspring start`, the steps that were running run again, and an implementation step first checks whether its work is already done.

## Safeguards

### Agent configuration and tool access

Each run uses the repository's configuration for its CLI. Your user-level skills are left out, and so are connectors that act in your name, image generation, computer use and your browser. Claude Code and Codex also leave out your plugins. OpenCode loads them as it does in your own sessions, so install there only plugins you would let run unattended.

No MCP server reaches a run until you allow it, including the ones in the repository's `.mcp.json` and in your own configuration. Allow each by name under Agents in Settings. The factory finds it in the repository's `.mcp.json`, your `~/.claude.json` or your Codex configuration, and gives it to every run on every CLI. For a web product, a browser server such as chrome-devtools lets the agents look at the pages they change while they test and review them.

### The guard

On Claude Code and Codex, a hook stops an agent the factory started from:

- running `git commit` or `git push` (the factory commits and pushes)
- merging, approving, or calling the GitHub API to do either
- running a command that matches your denied commands in Settings, such as `terraform apply *` or `kubectl * --context *prod*`
- touching the files where the factory keeps decisions, approvals, recovery procedures and credentials

The hook matches commands as written. It catches honest mistakes, and it is not a boundary against an agent that tries another way, which is why production credentials belong elsewhere. It applies only to agents the factory started, so your sessions in the same repository are unaffected.

OpenCode runs no hook. Its permissions, which the factory writes for each run in the form the installed version reads, deny the same commands and paths.

### Permission mode

By default every run uses its CLI's auto mode, which checks risky actions. When it refuses one a step needs, the lead finds another way or asks you.

**Bypass** skips the permission checks of Claude Code and Codex for every run. Agents can then delete files, rewrite history in their worktree, and run any command your denied commands don't name. The guard still stops pushes, merges, approvals and your denied commands. OpenCode runs the same in either mode.

### Dashboard access

The dashboard listens on `127.0.0.1` only, and answers only requests addressed to `127.0.0.1` or `localhost`, so a page on another site can't reach it through your browser. An SSH port forward still works.

### Tracker issue filter

When anyone can open issues in your tracker, the filter keeps the issues your own command rejects away from every agent. The factory runs the command on each open issue the lead has not yet put in a sprint proposal, and only the issues it admits reach the agents.

Set the command under Connections → Issue tracker in Settings, and leave the field empty for no filter. It is stored as `[tracker] filter` in `.ovenspring/config.toml`.

This example uses `jq` and admits only issues with the `factory-ready` label. Save it as `.ovenspring/filter-issue.sh`, make it executable with `chmod +x .ovenspring/filter-issue.sh`, and enter `./.ovenspring/filter-issue.sh` as the command:

```sh
#!/bin/sh
jq -e '(.labels | index("factory-ready")) != null' >/dev/null
status=$?
case "$status" in
  0) exit 0 ;;
  1) printf '%s\n' 'missing required label: factory-ready'; exit 2 ;;
  *) exit 1 ;;
esac
```

If `jq` is missing or the input cannot be read, the script exits 1 and the issue goes on as if no filter were set.

The factory runs the command once for each new or changed issue; an unchanged issue is not checked again. It runs it through `sh -c` from the repository root, in the factory's environment, and sends one UTF-8 JSON object without a trailing newline on stdin:

```json
{"key":"...", "title":"...", "body":"...", "url":"...", "labels":["..."], "author":"...", "comments":[{"author":"...", "body":"..."}]}
```

`author` is the GitHub login or Linear full name; it is empty when the tracker provides none. Each comment has `author` and `body`.

The exit code decides what happens to the issue:

- Exit 0 admits it.
- Exit 2 drops it. The first line of stdout is the reason.
- Anything else is a failure: another exit code, a command that cannot start, or a run longer than 60 seconds.

A failure means the issue goes on as if no filter were set, so a broken or missing command lets every issue through. A failure event records the issue key and what failed. The command's stderr is discarded. If the command is still running after 60 seconds or when the factory stops, the factory stops its whole process group.

For a drop, the factory records the issue key, URL and reason in `events.jsonl`. It replaces its own tokens and keys, and anything in a common token format, with `[masked]`. Agents read this event log when they collect sprint facts, so write a reason that names the rule the issue failed without quoting the issue's text, as the example's reason does.

After you set or change the command, issues it has not judged yet stay visible to agents until it does, and the factory checks every open issue once, even one that has not changed.

Once the lead puts an issue in a sprint proposal, the factory keeps the version the lead read and stops checking it. If a revised proposal leaves the issue out, it is checked again.

## Configuration

Setup writes `.ovenspring/config.toml`, and Settings edits it. The factory reads the file from your checkout, so you can also edit it directly, and a change applies from the next run. Committing it is optional; the factory works the same either way, and committing lets your team see the settings. Unknown keys are errors: the factory keeps running on the last valid file, and Settings shows what is wrong with the new one.

```toml
[repo]
base_branch = "main"

[merge]
by = "factory"                     # factory | owner

[held]                             # paths that hold a change for you, beyond what designs declare
schema = { paths = ["db/migrations/**"] }
ci = { paths = [".github/workflows/**"] }

[person_reviews]                   # changes a person reviews, in your words
payments = "changes to how prices, discounts or refunds are calculated"

[agents]
permissions = "auto"               # auto | bypass
mcp = ["playwright"]

[roles.lead]
cli = "claude"                     # claude | codex | opencode
model = "claude-opus-5-5"          # the CLI's default when unset
effort = "high"
skills = ["storefront-domain"]
append = "Prices are stored in cents."

[roles.executor]
cli = "codex"

[roles.reviewer]
cli = "codex"

[roles.analyzer]
cli = "codex"

[tracker]
kind = "linear"                    # linear | github
team = "ACME"
filter = "./.ovenspring/filter-issue.sh"

[slack]
channel = "C0123456789"
owners = ["U0123456789"]

[app]
setup = "pnpm install --frozen-lockfile"
start = "pnpm dev --port $PORT"
ready = "http://localhost:$PORT/health"
# nothing_to_run = true            # instead of the three above

[guard]
deny = ["terraform apply *"]

[review]
bots = ["review-helper[bot]"]

[limits]
issues_at_once = 3
# review_rounds = 3                # no limit when unset
# fix_ci_rounds = 3

[agreements]
path = ".ovenspring/agreements"

[knowledge]
path = ".ovenspring/knowledge"
```

### Choosing agents for each role

| Role | Does |
|---|---|
| lead | Plans sprints, defines requirements, writes designs and prototypes, plans tasks, handles exceptions, writes the sprint review, talks to you |
| executor | Implements tasks with their tests, applies review findings, fixes CI, resolves merge conflicts |
| reviewer | Reviews documents, code and security, and checks the sprint review |
| analyzer | Collects the facts a sprint review needs, without judging them |

Choosing models is a cost decision. If cost matters, give the strong models to design and review, the lead first. The executor can run on a cheaper model. In my runs the price was time: implementation ran slower. If cost doesn't matter, run every role on the same strong model. If you worry that a design and its implementation share the same blind spots, give only the reviewer a different model. A change applies from the role's next run.

A role can also load skills from the repository, in the folder its CLI reads (`.claude/skills` for Claude Code, `.agents/skills` for Codex, either for OpenCode), on top of the factory's skills. The factory's skills exist only in its worktrees and never enter a commit. Additional instructions are appended to the end of every prompt of the role.

## Records and downloads

The Sprints page and each issue's page download the records of that sprint or issue as one JSON Lines file, every record in time order, one per line. It covers each agent run with its usage and cost, state changes, decisions and approvals with who gave them in Slack, each step's result, commits and merges, and goal changes. Hand it to an agent to analyze where the time went, or keep it as an audit trail. Configured credentials and values in common token formats are masked.

## Where things live

| What | Where |
|---|---|
| The executable | `~/.local/bin/ovenspring` |
| Configuration | `.ovenspring/config.toml` in the repository |
| Working agreements | `.ovenspring/agreements/`, or the folder you set |
| Knowledge | `.ovenspring/knowledge/`, or the folder you set |
| Issue branches | `ovenspring/<issue key>` |
| Credentials | `~/.config/ovenspring/credentials/` (`$XDG_CONFIG_HOME` when set), readable by you only |
| State, worktrees and logs | `~/Library/Application Support/ovenspring/repos/` on macOS, `~/.local/state/ovenspring/repos/` on Linux (`$XDG_STATE_HOME` when set) |
| The factory process's log | `daemon.log` in the repository's state folder |

## Removing Oven Spring

To stop using it in one repository:

1. Run `ovenspring stop` there.
2. Delete its credentials file and state folder. Both are named after the repository's folder and a hash of its path, such as `storefront-3fa29c01b2d4`: `storefront-3fa29c01b2d4.toml` under the credentials location and `storefront-3fa29c01b2d4/` under the state location above.
3. Run `git worktree prune` in the repository, so git forgets the worktrees that lived in the state folder.
4. Close any open pull requests from `ovenspring/` branches you don't want, and delete those branches, locally and on GitHub.
5. Delete `.ovenspring/` if you no longer want the configuration, agreements or knowledge, and the agreements and knowledge folders too if you set others.

To remove it from the machine, do that for each repository, then delete `~/.local/bin/ovenspring`. You can also remove the Slack app from your workspace.
