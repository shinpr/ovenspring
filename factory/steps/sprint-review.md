# Step: sprint-review

You settle the sprint that just ended with the owner: you judge whether it reached its purpose, and change how the factory works and what it knows where the sprint showed it should. The owner reads the review and replies, or closes the sprint, which lets your agreement and knowledge changes merge.

## Required skills

Load and read each skill completely before acting on your item: `ovenspring-writing-for-the-owner`, `ovenspring-writing-for-agents`.

## Inputs

- the sprint's facts (your item names the file), its purpose, key results and plan
- the working agreements and knowledge folders (under Context)
- the product goal
- the owner's reply to an earlier review, when your item names one: what it says about how the factory works or what it knows is yours to review with the sprint's facts; what it says about what to build next reaches the next sprint's plan, which reads it
- the agreements check's findings, when your item names them: drop or correct each agreement they name

## 1. Judge the sprint

Judge each key result against the approved plan and the relevant issue’s agreed criteria, using evidence about the product version being judged. Mark it met or not met and cite the observation behind the judgment. If the required evidence is missing, name it. Use those results to judge whether the sprint reached its purpose.

## 2. Review the working agreements and the knowledge

Find what in the way the factory works and in what it knows led to the gaps between its plan and its results: what helped, what hindered, and what did not contribute, including what the failed attempts have in common; start from what the factory itself added, its working agreements and its knowledge.

An agreement is an instruction an agent acts on, in positive form, specific to this repository, stated only here and consistent with the factory's rules and the other agreements; it changes how existing work is done rather than adding artifacts or steps. The agreement holds only the instruction.

Knowledge is a fact about this product, its code, or the environment it runs in that changes how the factory acts there, and that the code, the documents, and the tests do not show: a cause the sprint's failures traced, something the owner told the factory that no record held, or what a step's research settled. Its title states the fact; under it, when it applies and where it stops applying, and the sprint record that showed it.

1. Keep an agreement or knowledge file only while it meets its definition, addresses a cause rather than a particular event, and answers a problem that would cost more than it does; whatever the sprint did, rewrite or drop one that does not.
2. Where the sprint's work met its occasion (the choice an agreement governs, a choice a knowledge file's fact bears on):
   - The problem it answers occurred: the problem still needs an answer, and whether this one is that answer is in question. When the choice did not apply or read it, place it where the choice is made (a knowledge file, by its title). Otherwise change what produces the problem, then keep, rewrite, or drop it by what remains for it to answer.
   - The choices went as it asks without it: drop it.
3. Add only for a gap the sprint showed that none answers, meeting step 1, in the fewest words and files; every one is another branch to keep.
4. Name what was removed and why, and for each one added or rewritten, the problem it answers and the record behind it. With nothing to remove or add, say so.

When a problem's cause lies in what only the owner changes (the factory itself: its rules, roles, steps, and skills), say so in the review for the owner rather than writing an agreement around it.

Put each agreement in the file of the roles it concerns: `common.md` for everyone; `lead.md`, `executor.md`, `reviewer.md`, `analyzer.md` for one role; and each knowledge file by its path under the knowledge folder (a folder per area when it helps). Give each changed file's full new content, written whole from what remains; give a file left with nothing an empty content, which deletes it; leave unchanged files out.

Leave the product worktree unchanged.

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
