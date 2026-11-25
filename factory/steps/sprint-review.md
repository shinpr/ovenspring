# Step: sprint-review

You settle the sprint that just ended with the owner: you judge whether it reached its purpose, and change how the factory works and what it knows where the sprint showed it should. The owner reads the review and replies, or closes the sprint, which lets your agreement and knowledge changes merge.

## Required skills

Load and read each skill completely before acting on your item: `ovenspring-writing-for-the-owner`, `ovenspring-writing-for-agents`.

## Inputs

- the sprint's facts (your item names the file), its purpose, key results and plan
- the working agreements under `.ovenspring/agreements/`, and the knowledge folder (under Context)
- the product goal
- the owner's reply to an earlier review, when your item names one: what it says about how the factory works or what it knows is yours to review with the sprint's facts; what it says about what to build next reaches the next sprint's plan, which reads it
- the agreements check's findings, when your item names them: drop or correct each agreement they name

## 1. Judge the sprint

Judge first whether the state the sprint's purpose states was reached, by its key results and by what they leave out: the purpose is met or missed. Then judge each key result met or not, with the observation behind it: a merged pull request, a test, a fact. A key result the records cannot show is not met, and its evidence names what is missing.

## 2. Review the working agreements

Find what in the way the factory works led to the gaps between its plan and its results: what helped, what hindered, and what did not contribute, including what the failed attempts have in common; start from what the factory itself added, its working agreements.

1. Review each working agreement by what its problem did in the sprint:
   - The problem it answers occurred: the problem still needs an answer, and whether this agreement is that answer is in question. Find why it did not prevent the problem. When it was not applied where the choice is made, place it there. Otherwise what it asks does not reach the cause; an agreement already rewritten for this problem shows this. Change what produces the problem, then keep, rewrite, or drop the agreement by what remains for it to answer.
   - The problem did not occur and its occasion arose: it stays when the choices there applied it, and is dropped when they did not.
   - Its occasion did not arise: drop it when it names a particular event, or when the problem it answers would cost little if it occurred.
2. Add only for a gap the sprint showed that no agreement answers: at its cause rather than the particular event, where the problem would cost more than the rule, in the fewest words and rules; every rule is another branch to keep.
3. Name what was removed and why, and for each agreement added or rewritten, the problem it answers and where its occasion arises. With nothing to remove or add, say so.

When a problem's cause lies in what only the owner changes (the factory itself: its rules, roles, steps, and skills), say so in the review for the owner rather than writing an agreement around it.

An agreement is an instruction an agent acts on, in positive form, specific to this repository, stated only here and consistent with the factory's rules and the other agreements; it changes how existing work is done rather than adding artifacts or steps. The agreement holds only the instruction.

Put each agreement in the file of the roles it concerns: `common.md` for everyone; `lead.md`, `executor.md`, `reviewer.md`, `analyzer.md` for one role. Give each changed file's full new content, written whole from what remains; give a file left with nothing an empty content, which deletes it; leave unchanged files out.

## 3. Review the knowledge

Knowledge is a fact about this product, its code, or the environment it runs in that changes how the factory acts there, and that the code, the documents, and the tests do not show: a cause the sprint's failures traced, something the owner told the factory that no record held, or what a step's research settled. Its title states the fact; under it, when it applies and where it stops applying, and the sprint record that showed it.

1. Review each knowledge file by what the sprint's facts show of it:
   - The sprint worked in its area: keep it when the work there read it and it held; rewrite it when what it states proved wrong or incomplete; when the work there did not read it and needed it, rewrite its title so a reader in that area finds it.
   - What it describes is no longer in the code or the environment (the cause was fixed, the code it names changed): drop it.
   - The sprint did not work in its area: keep it unchanged.
2. Add only what the sprint showed later work in an area needs and no file holds, as the fact rather than the event; a decision about one issue stays in that issue's pull request.
3. Name what was removed and why, and for each file added or rewritten, the record behind it. With nothing to remove or add, say so.

Give each changed file's full new content, by its path under the knowledge folder (a folder per area when it helps); give a file to delete an empty content; leave unchanged files out.

Your output is the only change; leave the worktree as it is.

## Output

Write to the path in your item's `out` field:

```json
{
  "key_results": [{ "text": "<key result>", "met": true, "evidence": "<what shows it>" }],
  "agreements": [{ "file": "common.md | lead.md | executor.md | reviewer.md | analyzer.md", "content": "<the file's full new content, or empty to delete it>" }],
  "knowledge": [{ "file": "<path under the knowledge folder, ending in .md>", "content": "<the file's full new content, or empty to delete it>" }],
  "message": "<the review for the owner: whether the sprint reached its purpose and each key result, what merged, was held or dropped, the agreement and knowledge changes with the problem or record behind each, changes to the factory itself the owner may make, and, when either changes, that the next sprint is proposed once their pull request merges>"
}
```
