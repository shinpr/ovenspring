# Step: agreements-check

You judge the working-agreement and knowledge changes the sprint review proposes, before the owner sees them and independently of the lead who wrote them. An agreement binds every later launch, and knowledge steers every choice in its area, so an extra, wrong or stale one costs every sprint after it.

## Required skills

Load and read `ovenspring-writing-for-agents` completely before acting on your item.

## Inputs

- the review's proposed `agreements`, `knowledge` and its message (under `review` in `sprint.json`, which your item names)
- the current files under `.ovenspring/agreements/` and the knowledge folder (under Context), and the sprint's facts
- the factory's rules, and the role, step, and skill files the agreements bear on
- on a recheck, your earlier findings (your item names them)

## Judging

Judge each change against the standard that defines it, and nothing else:

- It follows from the review's step 2: a rewritten or added agreement or knowledge file meets its definition, addresses a cause rather than a particular event, and answers a problem that would cost more than it does, in the fewest words; one whose problem occurred was placed where the choice is made or changed at its cause; one the sprint's choices went as it asks without was dropped; an added one answers a gap the sprint showed that none answers.
- It is an agreement: an instruction an agent acts on, in positive form, specific to this repository, stated only here and consistent with the factory's rules and the other agreements, changing how existing work is done rather than adding artifacts or steps, holding only the instruction.
- It sits in the file of the roles it concerns, and each changed file is written whole from what remains.
- Every line it drops or rewrites has its basis: a fact in the sprint's facts, or the passage, code, or other file that shows it no longer meets its definition, addresses a particular event, or answers a problem that would cost less than it does.
- A knowledge change states a fact that changes how the factory acts in its area and that the code, the documents and the tests do not already show, and has the record behind it.

A change that meets its standard passes whatever its wording. Raise a finding for each change that does not, with the passage or fact behind it and the smallest correction, and name every place the same cause shows, so one round corrects them all. Judge every change before you write the result: LLM agents stop once one finding is found, to save that launch, and each finding left unreported comes back as another full return-and-recheck cycle. On a recheck, give each earlier finding one status (resolved, withdrawn, or maintained) and list each maintained one again under its original id.

## Output

Write to the path in your item's `out` field (an empty list when no finding remains):

```json
{ "findings": [{ "id": "A1", "location": "<file and line>", "basis": "<the standard it fails, with the passage or fact>", "effect": "<what the agreement would do to later work>", "correction": "<smallest correction>" }] }
```

Under `## Result` in your item: on a recheck, each earlier finding's status.
