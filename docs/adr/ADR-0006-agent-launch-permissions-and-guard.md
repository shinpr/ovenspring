# ADR-0006 Launch agents in their CLI's auto mode, project-scoped, with guard hooks

## Status

Accepted

`Accepted` records the currently selected means, not an obligation to retain it. When later evidence supports a smaller sufficient choice, update or supersede this decision while the outcome, desired-future requirements, and non-goals remain true.

## Context

Agents run non-interactively on Claude Code, Codex or OpenCode with the same prompts (AC-051), project-scoped (AC-052), within the permissions the organization already allows an agent, and are stopped by hooks from merging, pushing, recording approvals or touching production, without affecting people's own sessions (AC-054). OpenCode has no hooks; its permission rules carry the same denials, and the guide explains the difference (AC-053). Both Claude Code and Codex now ship a mode in which a model judges each risky action instead of a person (Claude Code's auto mode, the default starting mode of its interactive sessions; Codex's auto-review), which is the automated judgment the design article points to for replacing routine approval prompts.

## Decision Point

- **Question**: How does the factory start an agent: with which permission model, which isolation, and which guard?
- **Why a decision exists**: Each CLI's auto mode, each CLI's manual mode with a generated allow-list, bypassing permission checks, and running every agent in a container are all credible; they differ in safety, configuration burden and portability.
- **Scope boundary**: AC-051 to AC-054, AC-058.

## Decision

### Decision Details

| Item | Content |
|------|---------|
| **Decision** | By default every launch runs in its CLI's auto mode: Claude Code with `--permission-mode auto`; Codex with the `workspace-write` sandbox, `approval_policy = "on-request"` and `approvals_reviewer = "auto_review"`; OpenCode with its permission rules allowing edits and commands inside the worktree and denying what the hooks deny on the other CLIs. The owner may set the factory to `bypass`, which runs Claude Code with `--dangerously-skip-permissions` and Codex with `--dangerously-bypass-approvals-and-sandbox`, at the owner's responsibility and with a warning in the dashboard. In every mode, Claude Code and Codex launches carry per-launch `PreToolUse` hooks calling `ovenspring hook`, and exclude user-level settings, skills, plugins, MCP servers and connectors. Skills reach agents through each CLI's own skill loading, from the worktree (ADR-0003). |
| **Why this** | Auto mode is the CLIs' own answer to running without a person approving each action; it blocks irreversible and out-of-environment actions by judgment and needs no allow-list per repository. The guard hooks stay deterministic for the few boundaries that must not depend on judgment (commit, push, merge, approval, the isolated directory and credentials file, and the commands the owner denies, such as ones reaching production). Bypass serves owners whose environment is already contained. |
| **Known unknowns** | Whether Codex's auto-review answers approval requests in `codex exec`, where no person is present; whether hooks fire in bypass mode on both CLIs; whether auto mode is available for the owner's plan or provider. Settled by probes (factory core design, Assumed Behaviors) for the first two. The factory does not check the third: a launch that fails stops the factory with the CLI's own error. |
| **Reconsider when** | A CLI's auto mode is unavailable in non-interactive runs, or blocks routine repository work often enough that steps regularly end blocked. |

## Rationale

### Options Considered

| Option | Product value from the requirements | Repository fit | Total complexity | Maintainability | Material trade-offs | Reversibility |
|--------|-------------------------|----------------|------------------|-----------------|---------------------|---------------|
| Auto mode with per-launch hooks; bypass optional | AC-051 to AC-054, AC-058 | New | No allow-list; one setting | CLIs improve the classifier | Depends on each CLI's auto mode in non-interactive runs | Switch the setting |
| Manual mode with a generated allow-list and hooks | AC-051 to AC-054 | New | Allow-list per repository to keep current | Owner maintains the list | Unlisted commands block steps; drifts from how the CLIs are used today | Switch the setting |
| Bypass only, with hooks | AC-051 to AC-054 | New | Least setup | One adapter per CLI | Exceeds what many organizations allow agents; hooks are the only barrier | Easy |
| Container or VM per launch | Stronger isolation | New | Images per repository, toolchains, port forwarding | High | Heavy setup for every repository | Hard |

**Selected**: Auto mode with per-launch hooks, with bypass available as one setting.

## Consequences

### Positive Consequences

- No per-repository list of allowed commands.
- The factory runs the way people already run these CLIs.

### Negative Consequences

- Auto mode may refuse an action the step needs; the step ends `blocked` with the refusal, and the lead raises it.

### Neutral Consequences

- Hooks catch honest mistakes; keeping production credentials off the machine remains the boundary against production (PRD Security).
- The hook matches every tool, not only the shell, to deny any call that names the isolated directory or the credentials file (AC-064); it is the rule that holds in `bypass` and for shell commands, where the launch scope does not.

## Architecture Impact

Adds a `backend` module with the launch description, three adapters, the permission mode (`auto` or `bypass`), failure classification per CLI, and the `ovenspring hook` subcommand.

## Related Information

- Claude Code permission modes: https://code.claude.com/docs/en/permission-modes
- Claude Code auto mode: https://www.anthropic.com/engineering/claude-code-auto-mode
- Codex auto-review: https://developers.openai.com/codex/auto-review
- Codex hooks: https://developers.openai.com/codex/hooks
- ADR-0005
