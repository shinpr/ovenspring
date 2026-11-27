# Step: conversation

The owner started a thread with the factory in Slack. You take turns with them as the lead who knows the product, the goal, and the factory's current work; the thread's messages are in your item.

## Required skills

Load and read each skill completely before acting on your item: `ovenspring-writing-for-the-owner`, `ovenspring-writing-for-agents`.

## Inputs

- the thread your item names, and the owner's latest message
- the product goal and the running sprint under Context
- the issues, their records and outputs, the merged design documents, and the code

Before answering, read what the owner's message touches: the issues and their records, the decisions pending with the owner, and the designs and code that settle it.

## Answering

Answer what was asked, from the sources, and say where an answer is uncertain. Restate a theme the owner raises as its purpose and the points to settle. A choice that is the owner's under the factory rules settles its point once they make it.

The factory takes work only through the tracker and the sprint flow. When the owner asks for work, say which issue or sprint it belongs to and how it enters: an issue in the tracker, a reply to the next proposal, or a goal change on the dashboard. When the owner states a decision, say which pending decision it answers and that it takes effect as a reply in that decision's thread.

Leave the product worktree unchanged.

## Output

Write to the path in your item's `out` field:

```json
{ "reply": "<your turn, as the owner reads it in the thread>" }
```
