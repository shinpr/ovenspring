# Step: test-design

## Required skills

Load and read each skill completely before acting on your item: `ovenspring-testing-principles`, `ovenspring-documentation-criteria`, `ovenspring-integration-e2e-testing`, `ovenspring-llm-friendly-context`.

You generate comment-only integration/E2E test skeletons for required proof that cannot be supplied more cheaply. Apply ovenspring-testing-principles for proof selection and safety, ovenspring-integration-e2e-testing for skeleton and lane contracts, and repository conventions for placement.

## Inputs

- **design_docs**: One or more readable Design Doc paths containing acceptance criteria and Test Boundaries.
- **ui_spec**: Optional UI Spec whose interactions constrain those criteria.
- **requirement_context**: Optional approved PRD or unchanged convergence record; otherwise use the Design Doc's carrier.

Accept equivalent labels and concise prose with the same meaning. Follow linked sources only when they determine an in-scope proof obligation. If required criteria or documents remain unavailable after local resolution, return `status: "blocked"` with the exact missing input.

## Selection

1. Read the accepted criteria, Design Doc verification requirements, and applicable UI Spec interactions, states and transitions. Apply ovenspring-testing-principles to establish the required proof boundary, reading its `references/frontend.md` for UI integration proof and `references/e2e.md` for UI E2E proof, then inspect actual existing test assertions and setup for coverage.
2. For uncovered integration/E2E proof, write the smallest sufficient skeleton set. Record the production entry/composition to traverse, real dependencies and allowed substitutes in the existing proof obligation. Before implementation exists, derive that obligation from the governing design; a test that could be written is planned proof, not existing coverage.
3. Explain each empty lane from sufficient existing proof, a selected skeleton in another lane, or why the accepted behavior requires no proof at that lane's boundary. Return an exact limitation for a required but unavailable boundary.

## Skeleton Contract

Use the repository's language, directory and naming conventions. Each lane has a separate file; preserve existing content and add only non-duplicated obligations. Follow Design Doc patterns where they determine the actual consumer contract.

A skeleton is committed before implementation, so its committed form contains only comments: no imports of absent modules or executable test-runner blocks. Implementers add imports, setup and assertions in the earliest task where the boundary is executable. Keep static gates valid at the skeleton stage without claiming that comments prove behavior.

Each selected test records:

- original AC text and source Design Doc (or UI journey plus its governing AC);
- trigger and observable result;
- `@lane`, `@dependency` using ovenspring-integration-e2e-testing annotations;
- selection evidence: uncovered claim and why existing or cheaper proof is insufficient;
- primary failure mode: the regression this test must detect;
- proof obligation: boundary to traverse, expected state/results, pass criteria, real dependencies and permissible substitutes;
- a distinct branch, state, lifecycle or fallback path when the main path could remain green through a material regression.

File-level comments hold only what the completed tests keep, such as the lane declaration and setup the tests share; each test's record carries the rest.

Keep executable assertions and mock implementation for the implementing task. Test Boundaries marking a dependency as real are carried as `@real-dependency: [component]`.

## Output

Return a selection explanation followed by the result JSON. Explain empty lanes and unresolved limitations according to Selection.

```json
{
  "status": "completed",
  "feature": "feature name",
  "generatedFiles": {
    "integration": "path or null",
    "fixtureE2e": "path or null",
    "serviceE2e": "path or null"
  }
}
```

Each lane value is an existing file path when generated, otherwise JSON `null`. Selection evidence stays in each skeleton; the parent checks empty lanes against accepted proof obligations. When blocked, return `{"status":"blocked","reason":"exact missing prerequisite and affected claim"}` instead of an invented completed result.

## Completion Check

- Every required integration/E2E boundary has sufficient existing proof, a selected skeleton, or an explicit limitation; unresolved coverage is not a completed claim.
- Every new skeleton is necessary for its observable boundary; repeated proof is consolidated.
- Required real dependencies remain real in the proof contract.
- Skeletons contain only comments and preserve original criteria, dependencies and exact paths for planning.
- Output includes all three lane keys when completed.
