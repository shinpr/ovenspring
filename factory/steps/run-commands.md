# Step: run-commands

The factory runs its own copy of the product for each issue: agents check their changes against it with `ovenspring app run`, and the owner opens it as a preview before passing a held merge. You find, in this repository, how to start that copy. The factory then starts the product once with your commands, and the owner keeps them, changes them, or records that there is nothing to run.

## Required skills

Load and read each skill completely before acting on your item: `ovenspring-writing-for-the-owner`.

## Inputs

- the repository at the base branch, in your worktree: its README and contributing guides, package manifests and their scripts, lockfiles, task runners, container and compose files, environment examples, and CI workflows

## The commands

Each issue's copy runs beside the others, so every command gets two variables:

- `PORT`: the port this copy listens on.
- `OVENSPRING_DATA_DIR`: this copy's own directory for its data, kept across its starts (empty at the first). Where the repository lets the product's data (a local database, uploads, files it writes) be placed, place it there, so copies never share data.

| Field | What it does | When |
|-------|--------------|------|
| `setup` | Prepares the copy, such as installing dependencies or creating its data; runs without input before every start of a stopped copy, on the data its earlier starts left | When the product needs it |
| `start` | Starts the product on `$PORT` in the foreground and keeps running; the factory stops it | Always, for `commands` |
| `ready` | A URL, with the port written as `$PORT`, that answers 2xx or 3xx once the product is up; without it, the factory waits for the port to accept connections | When the product has such a URL |

Write them the way the repository's own documentation and scripts start the product for development on a laptop, and take each from what the repository says; passing `PORT` and `OVENSPRING_DATA_DIR` through the options of the tools the repository uses is part of that. The factory's one start is the check of your answer: read the repository rather than starting the product yourself.

## Choosing the answer

- `commands`: the repository shows how the product starts.
- `nothing`: the product has nothing to start, such as a library or a command-line tool that other projects install. Agents then check with tests only, and no preview is offered.
- `unknown`: the product starts, but the repository does not show how, or starting it needs what only the owner can supply (a service account, a secret). The owner then enters the commands.

In `reason`, name the files and what they say that decide the answer; for `unknown`, also what is missing. The owner reads `reason` to decide whether to keep your answer, so a guessed command must be `unknown` instead.

Leave the product worktree unchanged.

## Output

Write to the path in your item's `out` field:

```json
{
  "runs": "commands | nothing | unknown",
  "setup": "<command, or null>",
  "start": "<command, or null>",
  "ready": "<URL, or null>",
  "reason": "<what in the repository decides the answer, in one or two sentences for the owner>"
}
```

Only `commands` gives commands; `start` is required with it.
