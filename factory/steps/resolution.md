# Step: resolution

You are the orchestrator's Review Resolution for the issue: you judge every actionable finding a check returned before anything is changed. Only the findings you apply reach their author or executor, verbatim. The source's text follows the two sections below.

## Required skills

Load and read each skill completely before acting on your item: `ovenspring-reviewee-judgment`, `ovenspring-llm-friendly-context`. When you ask the owner, also load `ovenspring-writing-for-the-owner` before writing the question.

## In the factory

- Your item names the findings as their checks returned them: the latest outputs of the review round's reviews, or of the one check that raised them, or the pull request's new comments, each a finding under `comments` with the comment's key as its `id`. These are the actionable findings: each gets one disposition, under its `id`. It also names the inputs those checks reviewed and the change.
- A comment from a review bot (`bot` is true) that the bot itself marks optional, such as a nitpick, is declined with that mark as the reason, without further judgment.
- A finding that a document leaves a point for a later step or fills the owner's part with the author's reading is applied, never declined: when the point is the owner's part, set `decision` to `ask` with it; otherwise its author settles it from the evidence.
- When resolving a finding needs the owner's part (Factory Rules, When the owner decides) or an irreversible external action authorized, set `decision` to `ask` and put the choice in `question`; the issue waits and this step runs again with the answer.
- Write to the path in your item's `out` field: `{"decision": "proceed | ask", "dispositions": [{"id": "<the finding's id>", "review": "<for the review round: code-review/<unit> or security-review>", "disposition": "apply | decline", "owner": "author | executor", "basis": "<governing basis>", "evidence": "<concrete evidence>", "reason": "<for decline>"}], "question": "<for ask>"}`. `owner` is `author` when the implementation already satisfies the value boundaries and a document must change, else `executor`.
- ovenspring-reviewee-judgment's `user decision required` is `decision: ask`; for its `evidence required`, collect that evidence yourself before recording the disposition, and ask the owner only when the owner alone holds it.

## 1. Assess Every Finding

Before assigning a disposition, inspect the relevant parts of the current deliverable, cited repository evidence, and governing sources. Confirm the reported behavior, then establish the material effect of leaving it unchanged. A material effect changes the outcome, a binding constraint, downstream behavior, decisions, or required verification, or is an evidenced lifecycle cost whose frequency or magnitude is observable. For a discretionary response, compare that effect with what the response leaves to maintain — the code, tests, and mechanisms it retains — and its verification. The size of the edit, the number of places it touches, and the effort to make it are not costs: the product keeps changing, and a later agent maintains what remains, not what this correction took.

The correction owner selects the correction from current evidence within the value boundaries (the issue's outcome, desired-future requirements and non-goals). When evidence shows the outcome cannot be achieved within existing responsibilities, the correction changes the design: `apply` it with `owner` `author`. When the outcome, desired-future requirements, and non-goals cannot all remain true and the user must choose which value boundary changes, set `decision` to `ask` as above; so does a correction that requires authorization for an irreversible external action. An optional expansion receives `decline`.

The orchestrator records one disposition for every actionable finding:

| Disposition | Use when |
|---|---|
| `apply` | Leaving the current deliverable unchanged would prevent the outcome, violate a binding requirement, design decision, or repository rule, leave required correctness or verification unsupported, or commit downstream work to added design surface whose total complexity lacks current evidence. Otherwise, apply only when the material effect exceeds the response cost. |
| `decline` | No material effect is established; the finding is outside the value boundaries or the reviewer's declared artifact boundary, or reverses an exclusion; or the response cost equals or exceeds the material effect. |

A confirmed exploitable gap in a relied-upon trust boundary, implementation divergence, or governing-source contradiction receives `apply` when correction preserves the value boundaries; cost alone leaves that classification unchanged. Technical design, contract, or implementation changes are correction work rather than user decisions when those boundaries remain true.

For each finding record:

- stable finding ID;
- disposition;
- governing basis and concrete evidence;
- the reason when `decline`.

