# Step: facts

You collect the facts the sprint review needs about the sprint that just ended: the gaps between what was planned and what happened, and what the working agreements did. Leave judgment and recommendations to the review that reads your report.

## Required skills

Load and read `ovenspring-writing-for-agents` completely before acting on your item.

## Inputs

In the state directory (the directory holding `sprint.json`, which your item names): the sprint record with its purpose, key results and plan; each of its issues' records under `issues/`; `events.jsonl` and `launches.jsonl`; the archived items under `board/archive/` and the outputs they name; and the pull requests they opened. The factory's settings as the owner set them (`config.toml` among your item's references), which account for choices made outside the sprint, such as the base branch and who merges. In the repository: the git history of the period and `.ovenspring/agreements/`. The CLIs' session records (Claude Code under `~/.claude/projects/`, Codex under `~/.codex/sessions/`) when a question needs what a launch did.

## Collect

Cover each question:

- **Gaps.** For the sprint and each of its issues, what the plan expected and what happened, in result and in time: each issue's end (merged, held, dropped) and its pull request; what each key result named and what merged toward it.
- **Owner's judgment.** Each point where the owner's answer differed from the factory's proposal or question: replies to proposals, holds, answers to questions, and how long each waited.
- **Flow.** Per issue, the steps in order with their rounds; review findings by disposition (applied, declined, returned); CI failures and their causes; each `exception` with its action; pauses and their causes.
- **Working agreements.** For each agreement in force during the sprint: whether the problem it answers occurred, whether its occasion arose, and whether the choices there applied it.
- **Knowledge.** For each file in the knowledge folder (under Context): whether the sprint worked in its area, whether the launches there read it, whether what it states held, and whether what it describes is still in the code or the environment.

Report each observation with where it came from (an item, an output, an event line, a commit, a session), and count where a count answers the question. Report what repeated across issues separately from what happened once. State what the records cannot show.

## Output

Write the facts as Markdown at the path your item names, then write to the path in your item's `out` field:

```json
{ "facts_path": "<the path you wrote>" }
```
