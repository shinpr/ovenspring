# Step: prototype

You show the owner what the new screen the issue adds could be, before its UI Spec is written: one HTML page they open from the dashboard and approve or answer. It is a step of the design, not its result: the UI Spec written from it is canonical, and the screen can still change as the design and the implementation learn more.

## Required skills

Load and read each skill completely before acting on your item: `ovenspring-coding-principles`, `ovenspring-ai-development-guide`, `ovenspring-documentation-criteria`, `ovenspring-llm-friendly-context`. Then read `references/frontend.md` in `ovenspring-ai-development-guide` and in `ovenspring-coding-principles`; they hold the frontend criteria the screen is built by.

**Decision purpose**: Deliver the user-required result with the least justified complexity; unnecessary mechanisms create downstream implementation, test, and maintenance obligations. Establish necessity before detailing a mechanism. Previously passed technical choices remain revisable; preserve user outcomes and explicit constraints.

## Inputs

- the approved PRD when the issue has one, otherwise the requirements output with its convergence record and exclusions; the codebase analysis; and the scope check
- the application's existing screens, components, and design tokens, where the repository has them

Keep the issue's non-goals outside the screen.

## Applicability

Add a screen element, interaction, state, responsive rule, visual constraint, or accessibility behavior only when supported by:

- a requirement or acceptance criterion;
- preserved existing behavior; or
- an applicable repository or design-system rule.

Include loading, empty, error, keyboard, screen-reader, contrast, token, breakpoint, and browser behavior when a source makes them part of the current UI outcome; otherwise omit them.

## Process

1. Extract UI-relevant requirements and ACs plus explicit exclusions.
2. Identify the affected views, entry/transition conditions, and observable interactions.
3. Follow the look and structure of the existing screens and their design tokens where the repository has them; where it has none, keep the page plain.
4. Before building extra interactions, states, or outputs, establish what required UI result the direct path cannot deliver and whether the addition is worth its total cost.
5. Write the screen as one self-contained HTML file at `docs/ui-spec/assets/<feature-name>/prototype.html`: styles, scripts, and sample data inline, nothing loaded from the network, and every screen and state the requirements need reachable within the page. Map each interaction and state to its governing requirement or AC.
6. Take its reference strength, how far the implementation follows it, from the owner's words (the issue and their answers): `binding` when they make its rendering the implementation's target, `reference` when only what the UI Spec records is to reach the implementation.

## Output

Write to the path in your item's `out` field:

```json
{ "path": "<the HTML file, relative to the worktree>", "referenceStrength": "binding | reference", "basis": "<the owner's words the strength rests on, quoted>" }
```

Under `## Result` in your item: each state and interaction built with its governing requirement or AC.
