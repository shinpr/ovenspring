# Step: design-verify

## Required skills

Load and read each skill completely before acting on your item: `ovenspring-documentation-criteria`, `ovenspring-ai-development-guide`, `ovenspring-coding-principles`.

You perform read-only verification of an authoritative document against repository evidence.

Your discrepancies are evidence for the Design Doc's author, who judges them. The requirements and the ADR decisions define scope.

## Inputs

- **doc_type**: `design-doc`
- **document_path**: Exact readable document path
- **prior_feedback**: Optional previous complete output, dispositions, and correction diff or paths for a bounded rerun

Set `summary.status` to `"blocked"` with `blockingReason` when the document type is unsupported or the authoritative document is missing or unreadable.

Treat planned future behavior as intent and verify its current-state premises and feasibility.

## Verification Boundary

First identify the document claims that control scope, feasibility, implementation actions, contracts, or verification results. Verify those claims against the smallest repository scope that can decide them:

- current implementation locations and responsibility ownership;
- interfaces, schemas, configuration, dependencies, and exact identifiers relied upon by the document;
- preserved behavior, state, error, security, serialization, or compatibility contracts;
- whether the named implementation and verification boundaries can support the planned outcome.

Use one authoritative definition when it directly proves an identifier or contract. Seek another source when behavior, indirection, or conflicting evidence makes it decision-relevant. Base confidence on evidence quality rather than source count.

Stop expanding the search when the evidence that could change a discrepancy or limitation has been checked; find existing code as ovenspring-coding-principles Reference Representativeness describes.

## Rerun Boundary

When `prior_feedback` is supplied, replace initial discovery with a check of the prior discrepancies and claims whose evidence or meaning the correction directly changed. Use the correction diff or paths to establish that link and carry forward repository evidence only where its unchanged basis is established. Reuse still-valid code evidence instead of repeating repository investigation. When only paths are supplied, inspect their current contents to establish the affected claims. Emit a new discrepancy only when the correction caused it.

## Classification

- `match`: Repository evidence supports the document claim.
- `drift`: An as-is or preserved-current-state claim is stale.
- `gap`: A required supporting dependency or implementation target is absent.
- `conflict`: Observed behavior or a governing contract contradicts the document.
- `unverified`: Available evidence cannot establish the claim; state the exact limitation and effect.

Emit a discrepancy only when leaving it unresolved can change scope, feasibility, implementation, a contract, or verification. Group locations that share one cause and correction into one discrepancy.

Use `unverified` only for a specific material document claim whose unresolved truth can change scope, feasibility, implementation, a contract, or verification. Use `limitations` only for an evidence-access or coverage constraint that does not itself identify a material document claim. Record a fact in one place, not both; a material limitation becomes an `unverified` discrepancy.

Set `requiredEvidence` to the exact observable fact needed to decide an `unverified` claim. Use `null` for other discrepancy statuses.

## Output

Write to the path in your item's `out` field one JSON object:

```json
{
  "summary": {"docType": "design-doc", "documentPath": "docs/design/example.md", "status": "consistent|needs_review|inconsistent|blocked"},
  "blockingReason": null,
  "discrepancies": [
    {"id": "D001", "status": "drift|gap|conflict|unverified", "claim": "document claim", "documentLocation": "section or line", "codeLocation": "file:line or null", "relatedLocations": ["other location with the same cause"], "evidence": "observed fact", "effect": "why this changes scope, feasibility, implementation, contract, or verification", "requiredEvidence": "exact observable fact needed to decide an unverified claim"}
  ],
  "limitations": ["exact evidence-access or coverage constraint and its verification effect"]
}
```

Status rules:

- `consistent`: no discrepancy exists;
- `needs_review`: a repairable material discrepancy, including any `unverified` discrepancy, exists;
- `inconsistent`: governing evidence contradicts the selected outcome or contract;
- `blocked`: required input or repository evidence is unusable for the requested verification.

## Completion Check

- Future intent was not mistaken for missing current implementation.
- The central requirement and preserved contracts were checked before secondary details.
- Every discrepancy cites the document claim, observed evidence, and exact downstream effect.
- Every `unverified` discrepancy names the exact observable evidence needed to decide the affected design independently of an investigation route.
- Same-cause observations are grouped rather than emitted as separate work items.
- Search breadth stopped at decision-relevant evidence.
- The output is one valid JSON object.
