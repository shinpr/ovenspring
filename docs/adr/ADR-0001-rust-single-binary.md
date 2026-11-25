# ADR-0001 Implement the factory as one Rust binary

## Status

Accepted

`Accepted` records the currently selected means, not an obligation to retain it. When later evidence supports a smaller sufficient choice, update or supersede this decision while the outcome, desired-future requirements, and non-goals remain true.

## Context

Oven Spring is brought into repositories from outside and runs next to whatever toolchain each repository uses (PRD R1). It must install as one executable that needs no runtime beyond the agent CLIs, `git` and the repository's own tools (AC-001), and it carries a deterministic merge gate and state transitions whose failure cannot be undone (PRD Reliability). The factory itself will be developed largely by coding agents.

## Decision Point

- **Question**: Which language and packaging deliver the factory's daemon, command line and embedded dashboard server?
- **Why a decision exists**: Rust, Go, and TypeScript compiled to a standalone executable all produce one file; a Node package installed with `npx` is the pattern the owner's earlier workflow tools use. They differ in runtime dependency, type guarantees, build speed and library maturity.
- **Scope boundary**: AC-001, the deterministic gate of AC-026, and the guard hooks of AC-054, which run as subcommands of the same binary.

## Decision

### Decision Details

| Item | Content |
|------|---------|
| **Decision** | Write the daemon, command line, hook commands and dashboard server in Rust and ship them as one self-contained executable per platform (statically linked on Linux; on macOS linking only system libraries). |
| **Why this** | One file with no runtime, independent of the Node or Python versions a target repository pins; exhaustive enums and ownership make the item state machine and merge gate hard to get subtly wrong; agents write current Rust well. |
| **Known unknowns** | Settled: Slack has no official Rust SDK, so Socket Mode is implemented directly over a WebSocket client (ADR-0004). |
| **Reconsider when** | A required integration has no workable Rust client and its protocol is too large to implement directly. |

## Rationale

### Options Considered

| Option | Product value from the requirements | Repository fit | Total complexity | Maintainability | Material trade-offs | Reversibility |
|--------|-------------------------|----------------|------------------|-----------------|---------------------|---------------|
| Rust binary | Meets AC-001 fully | New repository | Async runtime and slower builds | Strongest compile-time checks on states and contracts | Slower agent edit-build loops; Slack Socket Mode written in-house | Rewrite only |
| Go binary | Meets AC-001 fully | New repository | Simplest concurrency, fast builds | Weaker exhaustiveness on state enums | Mature Slack library; less type help on the gate | Rewrite only |
| TypeScript compiled with Bun | Meets AC-001 with a 60–100 MB file | New repository | One language with the dashboard | Runtime-level guarantees only | Official Slack, Linear and GitHub SDKs; bundles a runtime | Rewrite only |
| Node package via `npx` | Fails AC-001: needs Node in a version that may conflict with the repository's | Matches earlier tools | Low to write | As TypeScript | Install-time supply chain on every machine | Rewrite only |

**Selected**: Rust binary. Go is close; Rust's compiler checks more of what agents write, such as exhaustive matches on states, and the gate and state machine are where stronger types pay.

## Consequences

### Positive Consequences

- Installing and updating handle one file (ADR-0003).
- Guard hooks are subcommands of the same binary, so their version always matches the daemon's.

### Negative Consequences

- Builds are slower than Go or TypeScript, which lengthens agents' edit-build loops while developing the factory.
- The dashboard needs a second toolchain (Node) at development time (ADR-0002).

### Neutral Consequences

- Cross-compilation targets: macOS (arm64, x86_64) and Linux (x86_64, arm64).

## Architecture Impact

One Cargo workspace produces the `ovenspring` executable. The dashboard's built assets are embedded at compile time. No component of the factory runs in another language at run time.

## Related Information

- PRD: `docs/prd/oven-spring-prd.md` (R1, R11)
- ADR-0002, ADR-0003, ADR-0004
