# Step: exception

A step of the issue could not go on: it failed or was blocked, a review loop reached the owner's limit, a finding was returned, a dependency was lost, the factory's own operation failed twice in a row, or someone closed the pull request. It is work that fell short; handle it as your role describes.

## Required skills

Load and read `ovenspring-writing-for-agents` completely before acting on your item. When the action is `ask` or `recover`, also load `ovenspring-writing-for-the-owner` before writing the question or the summary.

## Inputs

- the item that could not go on, its result, and its launch log (your item names them)
- when the factory's own operation stopped the issue, that operation and what it answered (your item's request); after a recovery procedure did not succeed, what it printed
- the issue's history: its items and outputs, its design document and plan

## Way forward

Choose one action by what it makes the factory do:

- `retry`: the step or the factory's operation that could not go on runs again; for a lost dependency, the issue starts again from requirements.
- `replan`: the plan is written again.
- `redesign`: the design is written again.
- `drop`: the issue ends, is closed on the tracker, and the issues that depend on it learn of it.
- `ask`: the owner receives `question`; the issue waits for the answer.
- `recover`: the way forward needs what agents may not do, such as pushing, rewriting or deleting a branch, calling GitHub, changing the factory's records, or rebuilding a worktree that holds uncommitted work. Write `procedure` as a script that runs as it is written: its first line names its interpreter (such as `#!/usr/bin/env bash`), since the factory runs it as it is; write in `summary` what happened, what the procedure changes and whether it can be undone. The owner passes it in Slack, and only the factory runs it, in the issue's worktree. What an agent may fix in the worktree, fix yourself and choose `retry`.

## Output

Write to the path in your item's `out` field:

```json
{ "action": "retry | replan | redesign | drop | ask | recover", "note": "<the cause, its evidence, and why this action>", "question": "<for ask>", "procedure": "<for recover>", "summary": "<for recover>" }
```
