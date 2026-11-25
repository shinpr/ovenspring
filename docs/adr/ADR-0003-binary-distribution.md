# ADR-0003 Ship the factory inside the binary, and update by replacing it

## Status

Accepted

`Accepted` records the currently selected means, not an obligation to retain it. When later evidence supports a smaller sufficient choice, update or supersede this decision while the outcome, desired-future requirements, and non-goals remain true.

## Context

The factory's rules, role files, step files and skills will change often, and every repository using the factory must follow those changes (PRD R2). The owner customizes a repository only through settings, extra skills and appended instructions (R3), never by editing the factory's own prompts. Agents must receive the factory's skills as skills, through each CLI's own skill loading, and the repository's commits must not carry them (AC-002).

## Decision Point

- **Question**: Where do the factory's prompts and skills live, and how does a repository follow new versions?
- **Why a decision exists**: Copying them into the repository with a manifest that preserves local edits is proven in the owner's earlier tools; a git submodule or subtree is another way; keeping them inside the binary is a third.
- **Scope boundary**: AC-001, AC-002, AC-004.

## Decision

### Decision Details

| Item | Content |
|------|---------|
| **Decision** | The binary contains the factory's rules, role files, step files, skills and dashboard. Rules, role files and step files are assembled into each launch's prompt from the binary. Skills are written into each worktree the factory creates, at `.agents/skills/ovenspring-<name>/` and `.claude/skills/ovenspring-<name>/`, each with a `.gitignore` of `*` so git never sees them. A repository holds only `.ovenspring/config.toml`, its working agreements (`.ovenspring/agreements/`), the factory's knowledge folder (`[knowledge] path`) and its own skills. Binaries are published as GitHub Releases with an install script; running the script again updates. |
| **Why this** | Every CLI loads the factory's skills as skills from the worktree it works in; nothing of the factory enters a commit; updating is replacing one file, after which every repository runs the new version. |
| **Known unknowns** | None that change the choice. |
| **Reconsider when** | Owners need to change the factory's own prompts per repository, beyond appended instructions. |

## Rationale

### Options Considered

| Option | Product value from the requirements | Repository fit | Total complexity | Maintainability | Material trade-offs | Reversibility |
|--------|-------------------------|----------------|------------------|-----------------|---------------------|---------------|
| In the binary, skills written into worktrees | AC-001, AC-002, AC-004 | New | Writing skill files into a worktree it already creates | Updates are atomic | Prompts are not editable in place; customization only through R3 | Can later add a copy-out command |
| Copied into the repository with a manifest | AC-004 with merge logic | Matches earlier tools | Hash manifest, preserved edits, retirement of moved files | Large diffs on every update | Factory files mix with the repository's | Moderate |
| Git submodule or subtree | AC-004 partly | None | Submodule handling in every clone and worktree | Familiar to few | Worktrees and CI need submodule care | Moderate |

**Selected**: In the binary, skills written into worktrees.

## Consequences

### Positive Consequences

- The factory's skills exist only in the factory's worktrees, so people's own checkouts and agent sessions never see them.

### Negative Consequences

- An owner who wants to change a factory prompt must ask for a release or use appended instructions.

### Neutral Consequences

- Releases follow semantic versioning.

## Architecture Impact

Adds an embedded-asset module, the writing of skills into each worktree, release automation producing per-platform archives, and an install script.

## Related Information

- ADR-0001
- PRD: R1, R2
