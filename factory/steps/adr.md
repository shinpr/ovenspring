# Step: adr

## Required skills

Load and read each skill completely before acting on your item: `ovenspring-documentation-criteria`, `ovenspring-coding-principles`, `ovenspring-testing-principles`, `ovenspring-ai-development-guide`, `ovenspring-implementation-approach`, `ovenspring-llm-friendly-context`, `ovenspring-external-resource-context`, `ovenspring-requirement-convergence`.

## In the factory

- The `decision_points` are the scope check's `adrDecisionPoints`, each with the owner's answer: record that answer as the accepted decision.

You create one complete ADR batch per invocation.

You own how this codebase is built and you will keep building on it, so what you leave behind is yours to maintain: the work you spend now is how what remains stays simple. What it is for — the outcome, requirements, and non-goals — stays with the user's recorded decisions.

## Execution Gate

Before acting, map the preloaded skills to concrete rules for this task. Follow the applicable process below, advancing only when the current step's required evidence is present. Before finishing, verify that your work satisfies those rules and the requirements below.

## Inputs

- **Operation Mode**: `create` (default) or `update`
- **requirement_context**: Exact PRD path, or the orchestrator's unchanged convergence record only when there is no PRD
- **codebase_analysis**: The codebase analysis output; its `decisionMaterials.candidateDecisionPoints` are the `decision_materials` below
- **decision_points**: Orchestrator-selected decision points for an ADR batch, copied unchanged
- Existing document path or paths in update mode
- **correction_findings**: Verification or review findings as their check wrote them, to judge (update mode)

Use the outcome, scope, and exclusions from the requirement context and the orchestrator-selected Structural Scale and document route. Write a contradiction with a governing source as the blocking condition (Output) instead of silently changing that classification.

Create/update mode requires a current PRD carrier or convergence record. A scope-preserving update may preserve its existing carrier.

## Evidence Boundary

Use supplied `decision_materials` as the primary repository evidence:

- `decision_materials[].options` supplies repository-backed choices, repository fit, lifecycle cost drivers, and maintainability evidence for ADR selection; the requirements supply product value.

Inspect only gaps that can change reuse, option validity, a selected decision, an implementation contract, or verification. Cite repository paths, commands, merged documents, or supplied authoritative sources for material claims.

## ADR Batch — Create Mode

Create one ADR per supplied decision point and finish the complete batch before writing the output. Follow `references/adr-template.md` in the ovenspring-documentation-criteria skill. If evidence no longer supports the scope check's Choice or Durability filter result, write the contradiction as the blocking condition (Output).

Before the first ADR write, Glob `docs/adr/ADR-[0-9][0-9][0-9][0-9]-*.md`, parse valid numeric prefixes, and assign contiguous numbers from `max + 1` in the supplied `decision_points` order. Use `0001` when no numbered ADR exists. Make every assigned path unique, confirm it is still absent immediately before its write, and stop with a blocking collision instead of overwriting. Use the assigned path order in the output.

For each ADR:

1. Keep one technical question inside the scope of the requirements.
2. Compare every credible, materially distinct option using product value from the requirements, repository fit, total complexity, maintainability, material trade-offs, and reversibility. Use relative evidence rather than fabricated estimates. Compare options at the stated quality levels, or at the ovenspring-implementation-approach selected level where the requirements leave one unset, and record that level in the decision.
3. Record the owner's answer to the decision point as the selected option.
4. Record only the selected decision as a downstream technical constraint. The requirements remain implementation scope.
5. Keep end-to-end implementation design out of the ADR. Repository-owned implementation details go to the Design Doc only when the requirements activate them; external release execution and organizational rollout remain outside both artifacts.

## Update Mode

Update requested sections and dependent statements. Judge `correction_findings` as Factory Rules, Your item describes. If the outcome cannot otherwise be achieved, write the failing condition and evidence as the blocking condition (Output) instead of changing that design. Preserve unaffected decisions and safeguards. Re-check only identifiers or contracts whose meaning the update changes. An update operates on one existing ADR.

## ADR Files

- ADR batch: contiguous `docs/adr/ADR-[4-digit number]-[title].md` paths allocated by the create-mode rule above
- Follow `references/adr-template.md`; remove only non-applicable optional content.

## Output

Write to the path in your item's `out` field one of:

- ADR batch: `{"status":"completed","documentType":"ADRBatch","paths":["path"]}`
- Update: `{"status":"completed","documentType":"ADR","path":"existing path"}`
- Blocking condition: `{"status":"blocked","reason":"contradiction or exact unusable input"}`

## Completion Check

- Every change, including revisions to existing code, keeps the outcome, requirements, and non-goals unchanged.
- Every created ADR passes both filters and records the owner's answer as its selected option.
- The output is one valid JSON object.
