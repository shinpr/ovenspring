# ADR-0005 Keep the board as files, and the factory's runtime state outside the repository

## Status

Accepted

`Accepted` records the currently selected means, not an obligation to retain it. When later evidence supports a smaller sufficient choice, update or supersede this decision while the outcome, desired-future requirements, and non-goals remain true.

## Context

Each step of the factory is one item handed to one agent launch. Agents read their item and the sources it names, write a prose result for the next reader, and set a few fields the daemon acts on; the daemon never interprets prose (design article, "Running it"). The state must survive restarts (AC-039), produce logs and downloads (AC-045, AC-046), and stay out of the repository's working tree so that worktrees, `git clean` and people's sessions are unaffected.

## Decision Point

- **Question**: How are items, events and runtime state stored, and where?
- **Why a decision exists**: Markdown files with front matter, an embedded SQLite database, or both, are credible; the state can live inside the repository (ignored) or in a per-repository directory under the user's state path.
- **Scope boundary**: AC-036 to AC-039, AC-045, AC-046; the agents' contract for reading and finishing items.

## Decision

### Decision Details

| Item | Content |
|------|---------|
| **Decision** | Items are Markdown files with front matter in a board directory; agents change front matter only through `ovenspring item set`, which validates fields and values; machine-read step outputs are JSON files at the path an item names and are validated against a schema; every state change and launch is appended to JSON Lines logs. All of it lives in a per-repository directory under the user's state path, not in the repository. That directory holds two: the state directory, which every agent launch is given, and an isolated directory, given to no launch and denied by the launch hook, for what the factory trusts or runs on the owner's word (decisions, approvals, recovery procedures, the owner's cancel requests, the dashboard's address). |
| **Why this** | Agents read and write items with their ordinary file tools; the record is inspectable with `cat` and `jq`; JSON Lines is already the audit format the owner downloads; nothing appears in the working tree. |
| **Known unknowns** | None that change the choice. |
| **Reconsider when** | The number of items makes reading the directory a measured bottleneck. |

## Rationale

### Options Considered

| Option | Product value from the requirements | Repository fit | Total complexity | Maintainability | Material trade-offs | Reversibility |
|--------|-------------------------|----------------|------------------|-----------------|---------------------|---------------|
| Files outside the repository | All | Keeps the tree clean | Writes divided between the CLI and the daemon | Inspectable with standard tools | No queries; the dashboard reads the records on each request | Can be imported into a database later |
| Files inside the repository, ignored | All | Visible to every clone's tooling and to `git clean -X` | Same | Same | Agents in worktrees must reach the main tree; `git clean -X` deletes state | Moderate |
| SQLite | All | Outside the tree | Every agent interaction through the CLI; schema migrations | Queries are easy | Agents cannot read their item as a file; harder to inspect | Export to files |

**Selected**: Files outside the repository.

## Consequences

### Positive Consequences

- The download formats of AC-046 are views over records that already exist.
- A restart reconstructs everything from the directory (AC-039).

### Negative Consequences

- The dashboard reads the records from the directory on each request; no index is kept.

### Neutral Consequences

- Issue worktrees live in the same per-repository directory.
- The isolated directory keeps the records the gate and the owner's approvals rely on out of every launch's reach with the launch scope and the hook that already exist.

## Architecture Impact

Adds a `store` module owning the per-repository directory (board, outputs, events, launches, worktrees, approvals); the CLIs keep their sessions in their own homes and validating outputs by their step's type.

## Related Information

- Design article: https://www.norsica.jp/blog/how-i-design-a-software-factory.md
- ADR-0006
