# ADR-0007 The daemon alone talks to the tracker, GitHub and Slack

## Status

Accepted

`Accepted` records the currently selected means, not an obligation to retain it. When later evidence supports a smaller sufficient choice, update or supersede this decision while the outcome, desired-future requirements, and non-goals remain true.

## Context

The lead is the only agent that speaks to the owner and writes to the tracker (design article, "Running it"). Review comments from accounts without write access must not steer agents (AC-023). Pushes and merges happen only when the gate allows them (AC-026). The tracker is Linear or GitHub Issues (AC-012), read for open issues, dependencies and comments.

## Decision Point

- **Question**: Do agents reach the tracker, GitHub and Slack themselves, or does the daemon do it for them?
- **Why a decision exists**: Agents could be given MCP servers or credentials for these services, or the daemon could hold all credentials and exchange data through items and the factory's command line.
- **Scope boundary**: AC-012, AC-022, AC-023, AC-026, AC-032, AC-033.

## Decision

### Decision Details

| Item | Content |
|------|---------|
| **Decision** | The daemon holds every credential and is the only process that reads from or writes to the tracker, GitHub and Slack. It polls the tracker and GitHub, writes what a step needs into its item or a source file, and carries out an agent's outward action (a tracker comment, a question to the owner, a reply to a review comment) only from a validated step output. Agents get no MCP server or credential for these services. They still run as the owner's user, so ambient `gh` and git credentials stay reachable; the guard, not their absence, stops commits, pushes, merges and approvals (ADR-0006). |
| **Why this** | The trust boundary is one process; filtering of untrusted comments happens before any agent sees them; commits, pushes and merges have one gate. |
| **Known unknowns** | None that change the choice. |
| **Reconsider when** | A step needs interactive exploration of a service that cannot be prepared in advance. |

## Rationale

### Options Considered

| Option | Product value from the requirements | Repository fit | Total complexity | Maintainability | Material trade-offs | Reversibility |
|--------|-------------------------|----------------|------------------|-----------------|---------------------|---------------|
| Daemon owns external access | All | New | Typed step outputs for each outward action | One client per service | Agents cannot browse a service freely | Add MCP later per role |
| Agents use MCP servers or CLIs | All, if each agent obeys | New | Less daemon code | Behavior depends on prompts | Untrusted comments reach agents unfiltered; credentials on every launch; harder to guard | Moderate |

**Selected**: Daemon owns external access. Polling is used for the tracker and GitHub because webhooks need a public endpoint (as in ADR-0004).

## Consequences

### Positive Consequences

- Every outward action is a recorded event with its source item, which the audit trail needs (AC-046).

### Negative Consequences

- Each new kind of outward action needs a step output schema and a daemon handler.

### Neutral Consequences

- Polling intervals bound how quickly a new issue or review comment is seen.
- The owner can allow specific MCP servers for every launch (AC-052). An allowed server that reaches the tracker, GitHub or Slack is the owner's explicit exception to this decision, and the settings screen says so.

## Architecture Impact

Adds `tracker` (Linear, GitHub Issues adapters behind one trait), `github` (pull requests, checks, reviews, branch rules, merge), and the outward-action handlers that consume step outputs.

## Related Information

- ADR-0004, ADR-0005, ADR-0006
