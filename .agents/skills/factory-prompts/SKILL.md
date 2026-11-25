---
name: factory-prompts
description: Edits and reviews Oven Spring's embedded factory prompts (rules, roles, steps, skills) against their change history and what each launch receives. Use when changing or reviewing files under factory/.
---

# Factory Prompts

`factory/` is the text Oven Spring gives the agents it launches, embedded in the binary.

## What a launch receives

- One prompt per launch, assembled by `crates/agents/src/prompt.rs`: `rules.md`, the role's file for the steps `takes_role_prompt` selects, `steps/<step>.md` (`Step::text_file`: `review-response` reads `implement.md`, `fix-ci` and `merge-base` read `quality.md`; what differs is in the item's request, `crates/daemon/src/issues.rs`), the working agreements, the context, and the owner's additional instructions.
- A step's inputs are the entries its item names, built in `crates/daemon/src/handoff.rs`; its output is the type `crates/core/src/output.rs` parses for that step.
- `skills/` are written into each worktree under `.agents/skills/ovenspring-*` and `.claude/skills/ovenspring-*`; a step names the ones its launch reads under `## Required skills`.

A passage written for a mode, input, or invocation that none of these delivers never reaches a decision. Decide which side is wrong: change the launch when the step needs what the passage expects, and remove the passage when it does not, rather than override it elsewhere in the prompt.

## Change history

- Before removing or rewording a passage, read the commits that added or changed it; their messages record what it is for. A passage that looks redundant can be a countermeasure for one model's behavior. When the history records no reason, judge the passage by the decision it changes in the factory's launches.
- In the commit message of a prompt change, state what the change is for and the principle that decides it, in its own words, so the next change can read them.
