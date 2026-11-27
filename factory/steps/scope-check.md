# Step: scope-check

You make the orchestrator's decisions that follow the first codebase analysis of the issue: the Structural Scale when the Small Evidence Gate was unresolved, the requirement reopening, whether a UI Spec applies, and which technical decisions qualify for an ADR. The sources' text for each follows the two sections below.

## Required skills

Load and read each skill completely before acting on your item: `ovenspring-requirement-convergence`, `ovenspring-documentation-criteria`, `ovenspring-llm-friendly-context`. When you ask the owner, also load `ovenspring-writing-for-the-owner` before writing the question.

## In the factory

- Your item names the issue as written (the user's wording), the requirements output, the `codebase-analysis` output and the owner's answers.
- The owner's part (Factory Rules, When the owner decides) and a decision these texts give the user go to the owner in your preparation turn's message (Factory Rules, Talking with the owner), with the evidence and what each answer changes; a point first found during the work sets `decision` to `ask`, with the message in `question`. This covers the requirement reopening, a qualifying ADR decision point whose choice is the owner's, and a preparation the codebase analysis first shows.
- When a UI Spec applies and the issue and the owner's answers do not say how far its prototype is to be followed (`binding`: its rendering is the implementation's target; `reference`: only what the UI Spec records reaches the implementation), include that in the question.
- Write to the path in your item's `out` field: `{"decision": "proceed | ask", "structuralScale": "small | medium | large", "uiSpec": <true when documentation-criteria requires a UI Spec>, "adrDecisionPoints": [<each qualifying decision point the owner answered, with the answer>], "question": "<for ask>"}`.

## Small Evidence Gate

When the requirements output left the Structural Scale `unresolved`, the codebase-analysis output decides it: it supports Small when `analysisScope` and `currentPath` establish one repository-supported route inside one responsibility and every `candidateDecisionPoints`, `unknowns`, and `limitations` item is proven classification-invariant. Otherwise assign Medium or Large by ovenspring-documentation-criteria § Structural Scale. ADR qualification occurs after the codebase analysis's output gives credible technical options and the scope has passed the requirements stop.

## Requirement reopening

**Requirement reopening after codebase analysis**: When the first codebase analysis output after requirement confirmation shows existing behavior that serves a requirement's need and falls short only of extra behavior that requirement specifies, whichever field reports it, present it with its evidence and cost effect at one requirements stop before UI Spec, ADR, or Design Doc work, and let the user keep or change the requirement. The user selects requirements before any code is read, and every later phase builds and reviews against them without questioning them, so this is the one point where repository evidence can reduce them; judging such evidence as ruled out by the requirement removes the user's only chance to weigh it. Apply the answer as a requirement change (Requirement Change Detection below) and continue; later analyses do not reopen requirements this way.

## Requirement Change Detection

After an analysis writes its output, compare each user-facing or operational responsibility it exposes against the scope in the convergence record. When either leaving that responsibility as it is or changing it would alter the outcome or an exclusion, send it through the requirements gate before dependent design work; a technical choice about how to satisfy that scope continues through the design owners instead.

Treat a proposed change to the outcome, desired-future requirements, or non-goals as a requirement change. When evidence shows those value boundaries cannot all remain true, stop at the requirements gate and ask the user which boundary changes. A technical design or implementation correction that preserves them is not a requirement change, including removal of a technical choice that works but is no longer necessary; passage through an earlier phase does not establish that its means remain necessary.

## UI Spec and ADR qualification

Apply `ovenspring-documentation-criteria`: its UI Spec entry under What Each Document Fixes decides whether a UI Spec applies, and its ADR Decision Filters decide which decision points qualify. Before ADR qualification, use the governing source plus `reuse` and `invalidations` to remove questions that already have one sufficient approach. Apply ovenspring-documentation-criteria Choice then Durability filters only to the remaining `candidateDecisionPoints`.
