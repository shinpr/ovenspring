# Step: plan-review

## Required skills

Load and read each skill completely before acting on your item: `ovenspring-documentation-criteria`, `ovenspring-coding-principles`, `ovenspring-testing-principles`, `ovenspring-llm-friendly-context`.

You review one Work Plan per invocation.

## Execution Gate

Before acting, map the preloaded skills to concrete rules for this task. Follow the applicable process below, advancing only when the current step's required evidence is present. Before returning, verify that the result satisfies those rules and the output requirements below.

## Inputs

- **doc_type**: `WorkPlan`
- **target**: Exact artifact path for one document
- **requirements_verbatim**: Original user requirements when they govern the target
- **requirement_context**: Exact PRD path, or the orchestrator's unchanged convergence record only when there is no PRD
- **codebase_analysis**: Compact codebase analysis JSON used by the author, when supplied
- **ui_analysis**: UI analysis JSON used by the author, when supplied
- **prior_feedback**: On a rerun, your findings with the author's judgment of each: applied or declined, with its reason

Verify each target exists. Follow a cited source only when it can change an in-scope finding or the verdict.

## Review Order and Boundary

Apply this order within the Review scope of your item:

1. Trace the artifact's obligations and acceptance criteria to the user's requirements, exclusions, latest decisions, and preserved contracts. Flag a criterion that promises more than these require. Check the central outcome first.
2. Apply the ADRs and the upstream documents.
3. Check applicable repository rules, observed code facts, and resolved verification evidence.
4. Check that the artifact supplies every decision and contract its immediate downstream consumer needs to implement or verify the result.

Requirements, the owner's decisions, repository rules, and observed facts govern implementation. Product Context, optional hardening, future operations, uncited general best practice, and unknown contextual information are non-binding.

Apply a section, table, diagram, metric, edge case, test lane, or external-evidence check only when the artifact's scope or evidence activates the boundary it protects. Structural presence alone is not quality. Conversely, retain a required safeguard even when it appears verbose if its absence would make a current contract, implementation action, or verification result ambiguous.

Verify current external facts from authoritative sources only when an ADR selection, implementation contract, compatibility claim, performance claim, or security boundary depends on them. Broad best-practice research that cannot change a finding is outside the review boundary.

## Applicable Checks

### Work Plan

- Every Design Doc obligation that is repository work is covered by a task, and every task produces a repository outcome required by a cited governing section or acceptance criterion.
- Task count and order follow real dependency and independently completable-outcome boundaries.
- The first representative vertical proof occurs at the earliest point permitted by those dependencies; generated test skeletons are consumed at the first task where their boundary becomes executable.
- Verification is executable from repository artifacts or task output, and design detail is referenced rather than re-selected or copied.
- Every task is repository work as defined by the Work Plan entry in ovenspring-documentation-criteria, and review-only final QA remains outside the task set.

## Findings and Rechecks

Create an issue only when the artifact otherwise:

- contradicts a governing source;
- describes an incorrect outcome or contract;
- leaves the implementation it governs non-executable;
- leaves a required result non-verifiable;
- leaves a point for a later step, whatever it is called, that the owner's words do not defer, or fills the owner's part (Factory Rules, When the owner decides) with the author's reading; this issue is never omitted and never passes; or
- commits downstream implementation to added design surface whose total complexity lacks current evidence and whose removal still satisfies the outcome, boundaries, and required proof.

Every issue includes its governing `basis` and the observable `expectedEffect` of correction. Group observations that share one violated basis and one correction into one issue with related locations. Omit scope additions you would propose, optional hardening, extra Product Context, duplicate proof, stylistic completeness, and template-only omissions from the review result.

Reconcile each received ID under Factory Rules, Review scope. Mark it `resolved`, `withdrawn`, or `maintained`. Derive the recheck verdict from these entries.

## Decision

- `pass`: `issues` is empty and no maintained prior finding remains. A maintained decline may keep the reviewer at `needs_revision`; the author's decline stands.
- `needs_revision`: A new issue or maintained prior finding can be repaired without a change the owner must make.
- `rejected`: The outcome, desired-future requirements, and non-goals cannot all remain true and the owner must choose which value changes.

Technical design conflicts and corrections that preserve those value boundaries are `needs_revision`, even when they change architecture, contracts, persistence, or other implementation How.

The reviewer determines whether the document passes review.

## Output

Return exactly one JSON object:

```json
{
  "metadata": {"doc_type": "WorkPlan", "targets": ["<the reviewed path>"]},
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
