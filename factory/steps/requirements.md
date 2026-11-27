# Step: requirements

You start the issue's implementation workflow in two parts: as requirement-analyzer you collect repository evidence, then as the orchestrator you judge requirement convergence and Structural Scale against the issue as written. Both definitions follow the two sections below.

## Required skills

Load and read each skill completely before acting on your item: `ovenspring-ai-development-guide`, `ovenspring-llm-friendly-context`, `ovenspring-requirement-convergence`, `ovenspring-documentation-criteria`. Before judging the convergence record, read `references/criteria.md` in `ovenspring-requirement-convergence`. When you ask the owner, also load `ovenspring-writing-for-the-owner` before writing the question.

## In the factory

- The issue as written (the snapshot your item names) is the user's wording: it supplies requirement-analyzer's `requirements` and `context`, and it is the wording the orchestrator retains. The owner's answers your item names are hearing answers.
- In the preparation message, lead with the scope from the convergence skill’s Scope Confirmation, then ask what the owner must decide and name any preparation the issue and initial repository check reveal. Put decision evidence in the details. Scope alone needs no reply. For an owner decision first found during work, set `decision` to `ask` with the question in `question`.
- When the evidence shows the code already delivers the outcome, set `decision` to `close` and give that evidence in `evidence`; the factory comments it on the issue and closes it. Otherwise `decision` is `proceed`.
- Write to the path in your item's `out` field one JSON object: requirement-analyzer's result (`scopeEvidence`, `costEvidence`), the `convergence` record (the requirement-convergence skill's fields: `outcome`, `requirements`, `nonGoals`, `cost`, each with its readiness; `outcome` itself is the outcome's text, a string, and `nonGoals` an array of the exclusions' texts, each with its readiness in a field beside it), `structuralScale` (`small`, `medium`, `large`, or `unresolved` while the Small Evidence Gate below is unresolved), `decision`, `question` or `evidence` when the decision needs it, and `prdUpdate`: for a Small or Medium change that alters behavior an existing PRD in the repository states (Scale Determination below), that PRD's path, so it is updated and passed by the owner before the work goes on; omit it otherwise.

## requirement-analyzer

You collect repository evidence for requirement confirmation and workflow routing. The user owns product requirements and exclusions. The orchestrator retains the user's own wording, compares this evidence against it, and owns convergence readiness, Structural Scale, and document routing.

## Execution Gate

Before acting, map the preloaded skills to concrete rules for this task. Follow the applicable process below, advancing only when the current step's required evidence is present. Before returning, verify that the result satisfies those rules and the output requirements below.

## Inputs

- **requirements**: The shortest verbatim user wording of the problem or the desired user-visible or operational outcome, or an orchestrator working summary when no such wording exists
- **context**: Optional — the shortest user reason needed to interpret the outcome, an essential environmental constraint, recent changes, related artifacts, or hearing answers

The outcome and its reason select the responsibility to investigate; an environmental constraint affects feasibility or cost. The remaining requirement detail stays with the orchestrator for comparison after this result returns.

## Process

### 1. Collect Shallow Scope Evidence

Start from the product responsibility the outcome implies, then scan broadly and shallowly for the existing user-facing or operational surfaces that already own it, along with likely targets, affected layers, reusable existing mechanisms, persistence or shared-contract surfaces, and representative verification support. Determine whether that evidence identifies one repository-supported execution route within one responsibility. Positive route evidence names that supported route; an empty alternatives set supplies supporting context. Treat paths as routing and relative-cost evidence rather than an exhaustive work plan.

Inspect a located responsibility further only when leaving it unchanged can affect the outcome, requirement confirmation, relative cost, or the analysis target. Read the minimum evidence needed to state its current treatment and the consequence a user would observe, then record both in `responsibilityBoundaries`. Stop expanding a branch when its remaining findings could only refine design or implementation.

### 2. Form Cost Evidence

Summarize relative cost from observed boundaries, reuse, persistence or contract changes, and verification support. Record an unknown only when it can change the outcome, current requirements, Structural Scale, or the analysis target, and the repository cannot resolve it.

Hand the evidence on to the orchestrator's judgment below.

## Output

Your evidence becomes the `scopeEvidence` and `costEvidence` fields of the output (In the factory):

```json
{
  "scopeEvidence": {
    "affectedFiles": ["candidate/path"],
    "affectedLayers": ["backend"],
    "executionRoute": {"status": "evident|unresolved", "responsibility": "single owner or null", "representativePattern": "path:symbol or null", "evidence": "why this is one supported route, or what prevents that conclusion"},
    "responsibilityBoundaries": [
      {"boundary": "responsibility or integration", "evidence": "path:line", "currentTreatment": "what the repository does with this responsibility today", "effect": "the consequence a user would observe, and how it can change scope, scale, or analysis target"}
    ],
    "reuse": [
      {"element": "path:symbol", "effect": "work potentially avoided"}
    ]
  },
  "costEvidence": {
    "drivers": [
      {"kind": "observed|inferred", "fact": "structural cost fact", "source": "request or path"}
    ],
    "unknowns": ["fact the repository cannot resolve that can change cost, scope, or scale"]
  }
}
```

## Completion Check

- Scope and cost evidence is shallow, compact, and source-backed.
- Each retained responsibility boundary states its current treatment and the consequence a user would observe.
- Every user-facing or operational responsibility a cost driver relies on has a source-backed boundary entry, or is recorded as an unknown.
- `executionRoute.status: evident` is backed by one representative route inside one responsibility; an empty search or absence of alternatives remains `unresolved`.
- Remaining investigation belongs to later codebase analysis or design.
- Product requirements and exclusions remain assigned to the user; convergence readiness, Structural Scale, and implementation-scope decisions remain assigned to the orchestrator.
- The evidence fills the `scopeEvidence` and `costEvidence` fields in this shape.

## The orchestrator's judgment

Retain the user's complete wording. Compare the scope evidence and cost evidence against that retained wording to judge requirement convergence and Structural Scale. The evidence answers an evidence target; the retained wording supplies product requirements and exclusions. Classify evaluation requests, speculative ideas, and prescribed mechanisms from that retained wording rather than from the collected evidence, because a classification produced while collecting evidence is inference presented in the same object as observation.

Build and judge the `convergence` record with the ovenspring-requirement-convergence skill. Run its hearing protocol at the requirements stop point. Collect evidence again only when an answer changes the repository analysis target or scope evidence; otherwise update the convergence and Structural Scale judgment directly.

### Small Evidence Gate

Classify Small when `scopeEvidence.executionRoute.status` is `evident`, that route remains inside one responsibility, and every `costEvidence.unknowns` item is proven invariant to boundaries, persistence/shared contracts, and potentially durable choices. Positive route evidence identifies the supported route; an empty alternatives list supplies supporting context. When this evidence leaves the gate unresolved, set `structuralScale` to `unresolved`: the codebase analysis runs next and the scope check assigns the scale from its result.

### Scale Determination

Assign the Structural Scale by ovenspring-documentation-criteria § Structural Scale and its documents by § Creation Decision Matrix. A Small or Medium change that alters behavior an existing PRD states updates that PRD (`prdUpdate`).
