# Step: sprint-plan

You propose the next sprint: the state of the product or its users it reaches toward the product goal, the key results that judge whether that state was reached, and the open issues it takes, in order. A plan is the factory's forecast; the sprint review compares what happened with it.

## Required skills

Load and read each skill completely before acting on your item: `ovenspring-writing-for-the-owner`, `ovenspring-writing-for-agents`.

## Inputs

- the product goal under Context
- every open issue under `sources/issues/` as its requester wrote it, with what blocks it, and the factory's record of each under `issues/`, if any
- the owner's reply to an earlier proposal, when your item names one
- the record of the sprint that ended last, with its review, and the owner's replies to that review, when your item names them

## Plan

- Choose one purpose: a state of the product or its users that moves toward the goal and that the open issues can reach, compared by its value to the product's users. State it in one sentence as that state, not how it is reached; a means written into it leaves no other way to reach it when results call for another.
- Take the issues that serve the purpose, and nothing more: a sprint ends when its issues settle, so its size is the purpose's. An issue the factory already holds or dropped (its record says so) returns only through the owner's reply. Order them by dependency and risk, each one's `depends_on` naming only issues in this sprint; an issue blocked by one outside the sprint waits for a later sprint.
- Key results are the indicators that judge whether the purpose's state was reached: each states what is then true of the product or its users, not the work, the method, or the issue that gets there, so it holds whichever way the issues are carried out. Each is judged met or not at what the goal's and the issues' words state; where those words leave a choice open (a result's extent, content or form), the choice is the issue's own steps', with the owner, and the key result holds for any answer they give. All met means the state is reached.
- An owner's reply, to an earlier proposal or to the last review, decides what it addresses; revise the rest only as the reply requires.
- When no open issue serves the goal now, propose no issue and say in `message` what is missing; the owner reads it, and the factory proposes again once an issue opens.

Leave the product worktree unchanged.

## Output

Write to the path in your item's `out` field:

```json
{
  "purpose": "<the state this sprint reaches toward the goal>",
  "key_results": ["<what is then true of the product or its users>"],
  "issues": [{ "key": "<tracker key>", "depends_on": ["<key in this sprint>"] }],
  "message": "<the proposal for the owner: what you propose and ask, the points (purpose, key results, issues in order), and the details: why this purpose, these key results and these issues; a fact about the product in them is one you checked, and an issue's words are given as its requester's>"
}
```

## Done

- The purpose and every key result name a state of the product or its users, never an issue, a way, or a piece of work.
- Each issue in the sprint serves the purpose, and the plan decides nothing an issue's own steps settle.
