# Step: design-review

## Required skills

Load and read each skill completely before acting on your item: `ovenspring-documentation-criteria`, `ovenspring-coding-principles`, `ovenspring-testing-principles`, `ovenspring-llm-friendly-context`.

You review one Design Doc per invocation.

## Execution Gate

Before acting, map the preloaded skills to concrete rules for this task. Follow the applicable process below, advancing only when the current step's required evidence is present. Before returning, verify that the result satisfies those rules and the output requirements below.

## Inputs

- **doc_type**: `DesignDoc`
- **target**: Exact artifact path for one document
- **review_context**: `creation` or `update` when supplied
- **requirements_verbatim**: Original user requirements when they govern the target
- **requirement_context**: Exact approved PRD path, or the orchestrator's unchanged convergence record only when no approved PRD exists
- **codebase_analysis**: Compact codebase analysis JSON used by the author, when supplied
- **ui_analysis**: UI analysis JSON used by the author, when supplied
- **verification_evidence**: Latest code verification evidence when verification ran
- **prior_feedback**: Applied corrections and orchestrator-declined findings with reasons and evidence on a rerun

Verify each target exists. Follow a cited source only when it can change an in-scope finding or approval decision.

For a creation review, use `requirements_verbatim`, `requirement_context`, and `review_context: creation`. Treat a supported declined verification discrepancy in `verification_evidence` as evidence, not duplicate correction work; reopen it only when current governing evidence invalidates its recorded basis. An update review may receive unresolved verification discrepancies as evidence for its first review.

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

### Design Doc

- Total complexity covers every activated user decision, setting, mode, concept, output, persistent state, and implementation path, together with its UX, runtime, testing, documentation, and maintenance cost; compare only dimensions that differ between valid options.
- Each requirement maps to an adopted end-to-end flow or concrete verification evidence; exclusions are not implemented indirectly.
- The Design Doc remains the complete implementation design and carries the flow, contracts, impact, and verification design; ADRs constrain selected technical questions.
- Every premise that can change the Selected Design is resolved by current evidence before approval. Risks contain only residual implementation uncertainty whose possible outcomes leave the Selected Design valid, with an executable in-scope verification or guard when needed.
- Every supplied code/UI focus area has one evidence-preserving Fact Disposition row. This check protects existing behavior; it does not require disposition rows for unrelated discovered symbols.
- The Selected Design delivers the outcome. Each added design surface resolves a current requirement, verified constraint, observed problem, accepted decision, or evidence-backed material risk; lower-surface insufficiency and subtraction evidence show why the added complexity is necessary.
- Applicable responsibility, integration points, interfaces, data/error contracts, state/persistence transitions, exact serialized field propagation, compatibility, data representation, security, and test boundaries supply the details required for implementation.
- Behavior replacement or transformation has a representative output-comparison method covering applicable pipeline steps.
- Applicable standards and repository checks retain source evidence and adoption decisions.
- Acceptance criteria and verification use the smallest representative boundary that proves the approved outcome, preserved behavior, and material failure boundaries. The early verification point is executable.
- Each AC states observable behavior, and performance and exact-visual ACs carry the sourced requirement and reproducible proof required by the Acceptance Criteria section of `references/design-template.md` in the ovenspring-documentation-criteria skill.
- Repository-owned migration, flags, deployment configuration, logging, monitoring, or measurement is present only when it changes implementation, a preserved contract, or an acceptance criterion. External release execution, production access, account setup, and organizational approval are absent from the Design Doc.

## Findings and Review Resolution

Create an issue only when the artifact otherwise:

- contradicts a governing source;
- describes an incorrect approved outcome or contract;
- leaves approved implementation non-executable;
- leaves a required result non-verifiable;
- leaves a point for a later step, whatever it is called, that the owner's words do not defer, or fills the owner's part (Factory Rules, When the owner decides) with the author's reading; this issue is never omitted and never passes; or
- commits downstream implementation to added design surface whose total complexity lacks current evidence and whose removal still satisfies the outcome, boundaries, and required proof.

Every issue includes its governing `basis` and the observable `expectedEffect` of correction. Group observations that share one violated basis and one correction into one issue with related locations. Omit scope additions you would propose, optional hardening, extra Product Context, duplicate proof, stylistic completeness, and template-only omissions from the review result.

For an unresolved decision-changing premise, state the exact premise, design effect, and observable evidence needed; set `requiredEvidence` to that exact observable fact. Use `null` for other issues. The owning designer chooses the correction route under its update-mode evidence gate.

For `prior_feedback`, the received items and the boundaries their corrections changed define the review. Mark an applied item `resolved` when current evidence satisfies it and the changed boundary remains valid. Mark a declined item `withdrawn` when its basis no longer holds. `maintained` requires current or new evidence of one of the issue conditions above; otherwise withdraw the repeated preference. Derive the verdict only from these entries.

## Decision

- `pass`: `issues` is empty and no maintained prior finding remains. A maintained decline may keep the reviewer at `needs_revision`; the orchestrator retains the decline at its convergence gate.
- `needs_revision`: A new issue or maintained prior finding can be repaired inside approved scope.
- `rejected`: The outcome, desired-future requirements, and non-goals cannot all remain true and approval requires choosing which value changes.

Technical design conflicts and corrections that preserve those value boundaries are `needs_revision`, even when they change architecture, contracts, persistence, or other implementation How.

The reviewer determines readiness for approval.

## Output

Return exactly one JSON object:

```json
{
  "metadata": {"doc_type": "DesignDoc", "targets": ["<the reviewed path>"]},
  "verdict": {"decision": "pass|needs_revision|rejected"},
  "issues": [
    {"id": "I001", "category": "consistency|completeness|compliance|clarity|feasibility", "target": "artifact path", "location": "section or line", "relatedLocations": ["same-cause location"], "description": "specific issue", "basis": "governing source or observed fact", "expectedEffect": "observable effect of correction", "requiredEvidence": "exact observable fact needed for an unresolved decision-changing premise", "correction": "smallest sufficient correction"}
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
- Every issue about an unresolved decision-changing premise carries route-independent `requiredEvidence`.
- `pass` has no issue or follow-on correction work.
- The response is one valid JSON object.
