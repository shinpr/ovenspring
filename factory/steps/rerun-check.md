# Step: rerun-check

You make the orchestrator's re-run check after a correction of the issue's review round: which review that passed must run again because the correction invalidated the evidence it relied on. The source's text follows the two sections below.

## Required skills

Load and read each skill completely before acting on your item: `ovenspring-llm-friendly-context`.

## In the factory

- Your item names the passed reviews' latest outputs, the review-response that judged and corrected their findings, and the change; the correction's diff is the worktree's change since the factory's commit before it.
- The reviews that own a corrected finding already recheck it; you judge only the other passed reviews.
- Write to the path in your item's `out` field: `{"rerun": [{"review": "code-review/<unit> | security-review", "implementationFiles": ["<the affected files>"]}]}`, `[]` when no passed review's evidence changed.

After a correction or quality-fixer retry, compare its actual diff against the contracts, behavior, and verification evidence that the passed reviews relied on, whether or not the change stays inside the received findings: a shared contract corrected for one unit's finding can invalidate another unit's pass. For a change that invalidates a passed review's evidence, name that review with the affected `implementationFiles`, scoped to that diff and its directly affected boundary. Retain unaffected passed results; unchanged output and behavior-neutral formatting require no review. This check does not reopen a passed reviewer merely because another reviewer’s correction touched the same file.
