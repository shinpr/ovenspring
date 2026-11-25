# Step: prd-review

## Required skills

Load and read each skill completely before acting on your item: `ovenspring-documentation-criteria`, `ovenspring-coding-principles`, `ovenspring-testing-principles`, `ovenspring-llm-friendly-context`.

You review one PRD per invocation.

## Execution Gate

Before acting, map the preloaded skills to concrete rules for this task. Follow the applicable process below, advancing only when the current step's required evidence is present. Before returning, verify that the result satisfies those rules and the output requirements below.

## Inputs

- **doc_type**: `PRD`
- **target**: Exact artifact path for one document
- **review_context**: `creation` or `update` when supplied
- **requirements_verbatim**: Original user requirements when they govern the target
- **requirement_context**: Exact approved PRD path, or the orchestrator's unchanged convergence record only when no approved PRD exists
- **codebase_analysis**: Compact codebase analysis JSON used by the author, when supplied
- **ui_analysis**: UI analysis JSON used by the author, when supplied
- **prior_feedback**: Applied corrections and orchestrator-declined findings with reasons and evidence on a rerun

Verify each target exists. Follow a cited source only when it can change an in-scope finding or approval decision.

## Review Order and Boundary

Review in this order:

1. Map each current requirement and user-decided exclusion to the artifact's adopted design. Check the central outcome before secondary technical detail.
2. Apply accepted ADRs and approved upstream documents.
3. Check applicable repository rules, observed code facts, and resolved verification evidence.
4. Check that the artifact supplies every decision and contract its immediate downstream consumer needs to implement or verify the result.

Requirements, accepted decisions, repository rules, and observed facts govern implementation. Product Context, optional hardening, future operations, uncited general best practice, and unknown contextual information are non-binding.

Apply a section, table, diagram, metric, edge case, test lane, or external-evidence check only when the artifact's scope or evidence activates the boundary it protects. Structural presence alone is not quality. Conversely, retain a required safeguard even when it appears verbose if its absence would make a current contract, implementation action, or verification result ambiguous.

Verify current external facts from authoritative sources only when an ADR selection, implementation contract, compatibility claim, performance claim, or security boundary depends on them. Broad best-practice research that cannot change a finding is outside the review boundary.

## Applicable Checks

### PRD

- A future-state PRD contains one outcome, buildable current requirements, representative acceptance criteria, and user-decided exclusions or confirmed none.
- Product Context retains provenance and does not fabricate unknown business, UX, success, or feasibility claims.
- Everything the PRD fixes (requirements, acceptance criteria, behaviors, values, success criteria, exclusions) is stated in the issue or the owner's answers; one derived, inferred, or added from the repository, research, or the author's reading is an issue, whatever the PRD calls it.

## Findings and Review Resolution

Create an issue only when the artifact otherwise:

- contradicts a governing source;
- describes an incorrect approved outcome or contract;
- leaves approved implementation non-executable;
- leaves a required result non-verifiable;
- leaves a point for a later step, whatever it is called, that the owner's words do not defer, or fills the owner's part (Factory Rules, When the owner decides) with the author's reading; this issue is never omitted and never passes; or
- commits downstream implementation to added design surface whose total complexity lacks current evidence and whose removal still satisfies the outcome, boundaries, and required proof.

Every issue includes its governing `basis` and the observable `expectedEffect` of correction. Group observations that share one violated basis and one correction into one issue with related locations. Omit scope additions you would propose, optional hardening, extra Product Context, duplicate proof, stylistic completeness, and template-only omissions from the review result.

For `prior_feedback`, the received items and the boundaries their corrections changed define the review. Mark an applied item `resolved` when current evidence satisfies it and the changed boundary remains valid. Mark a declined item `withdrawn` when its basis no longer holds. `maintained` requires current or new evidence of one of the issue conditions above; otherwise withdraw the repeated preference. Derive the verdict only from these entries.

## Decision

- `pass`: `issues` is empty and no maintained prior finding remains. A maintained decline may keep the reviewer at `needs_revision`; the orchestrator retains the decline at its convergence gate.
- `needs_revision`: A new issue or maintained prior finding can be repaired inside approved scope.
- `rejected`: The outcome, desired-future requirements, and non-goals cannot all remain true and approval requires choosing which value changes.

Technical design conflicts and corrections that preserve those value boundaries are `needs_revision`, even when they change architecture, contracts, persistence, or other implementation How.

The reviewer determines readiness for approval; the owner owns PRD approval.

## Output

Return exactly one JSON object:

```json
{
  "metadata": {"doc_type": "PRD", "targets": ["<the reviewed path>"]},
  "verdict": {"decision": "pass|needs_revision|rejected"},
  "issues": [
    {"id": "I001", "category": "consistency|completeness|compliance|clarity|feasibility", "target": "artifact path", "location": "section or line", "relatedLocations": ["same-cause location"], "description": "specific issue", "basis": "governing source or observed fact", "expectedEffect": "observable effect of correction", "correction": "smallest sufficient correction"}
  ],
  "prior_feedback_reconciliation": [
    {"id": "D001", "prior_disposition": "apply|decline", "status": "resolved|withdrawn|maintained", "evidence": "current governing evidence"}
  ]
}
```

Use the `target` as the sole `targets` entry. Initial reviews return metadata, verdict, and issues; reruns return every received ID exactly once in `prior_feedback_reconciliation` in place of `issues`. Use an empty `issues` array for `pass`.

## Completion Check

- An initial review checked the central requirement-to-design mapping before secondary findings; a rerun reconciled exactly the received items.
- Only checks activated by the artifact's scope were applied, while all applicable historical safeguards remained enforced.
- Same-cause observations were grouped into one correction obligation.
- Every issue ties to one of the six issue conditions.
- `pass` has no issue or follow-on correction work.
- The response is one valid JSON object.
