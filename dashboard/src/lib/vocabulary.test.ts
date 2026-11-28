import { describe, expect, it } from 'vitest'
import type { ErrorNow, EventLine, LaunchRow, Position, Step } from '../api/types.gen.ts'
import {
  activityWords,
  errorName,
  eventSentence,
  plain,
  runSentence,
  statusRun,
} from './vocabulary.ts'

const none: LaunchRow['detail'] = {
  round: null,
  found: null,
  applied: null,
  declined: null,
  judging: null,
  scale: null,
  tasks: null,
  task: null,
  closed: false,
  documents: null,
}

const run = (change: Partial<LaunchRow>): LaunchRow => ({
  id: 'x.1',
  startedAt: '2026-11-21T14:00:00Z',
  endedAt: '2026-11-21T14:10:00Z',
  issue: 'ACME-141',
  role: 'reviewer',
  step: 'code-review',
  phase: 'review',
  cli: 'codex',
  result: 'done',
  costUsd: null,
  preparation: null,
  detail: none,
  ...change,
})

describe('runSentence', () => {
  it.each([
    [
      'a run under way, in the present',
      run({ role: 'executor', step: 'review-response', result: 'running', endedAt: null }),
      { who: 'Executor', what: 'is judging the findings' },
    ],
    [
      'a judgment under way with the findings it judges',
      run({
        role: 'executor',
        step: 'review-response',
        result: 'running',
        endedAt: null,
        detail: { ...none, judging: 4 },
      }),
      { who: 'Executor', what: 'is judging the 4 findings' },
    ],
    [
      'a check with what it found',
      run({ result: 'findings', detail: { ...none, found: 3 } }),
      { who: 'Reviewer', what: 'reviewed the code', result: '3 findings' },
    ],
    [
      'a second round',
      run({ result: 'pass', detail: { ...none, round: 2 } }),
      { who: 'Reviewer', what: 'reviewed the code again (round 2)', result: 'pass' },
    ],
    [
      'a check whose pass has its own word',
      run({ step: 'design-verify', result: 'pass' }),
      { who: 'Reviewer', what: 'checked the design against the code', result: 'consistent' },
    ],
    [
      'a judgment with how it came out',
      run({
        role: 'executor',
        step: 'review-response',
        detail: { ...none, applied: 1, declined: 1 },
      }),
      { who: 'Executor', what: 'judged the 2 findings', result: '1 applied · 1 declined' },
    ],
    [
      'an implement with its task',
      run({
        role: 'executor',
        step: 'implement',
        detail: { ...none, task: { number: 2, of: 3, name: 'query' } },
      }),
      { who: 'Executor', what: 'implemented task 2 of 3: query' },
    ],
    [
      'the requirements with the scale they judged',
      run({ role: 'lead', step: 'requirements', detail: { ...none, scale: 'medium' } }),
      // The sentence already says the scale; no result repeats it.
      { who: 'Lead', what: 'read the issue and judged it Medium' },
    ],
    [
      'a scope check with the documents it chose',
      run({
        role: 'lead',
        step: 'scope-check',
        detail: { ...none, documents: ['prototype', 'ui-spec', 'design-doc'] },
      }),
      {
        who: 'Lead',
        what: 'decided which documents it needs',
        result: 'prototype, UI Spec, Design Doc',
      },
    ],
    [
      'a scope check that chose the design alone',
      run({ role: 'lead', step: 'scope-check', detail: { ...none, documents: ['design-doc'] } }),
      { who: 'Lead', what: 'decided which documents it needs', result: 'Design Doc only' },
    ],
    [
      'the requirements that found nothing to change',
      run({
        role: 'lead',
        step: 'requirements',
        detail: { ...none, closed: true, scale: 'small' },
      }),
      { who: 'Lead', what: 'read the issue and found it needs no change' },
    ],
    [
      'a run that failed on an overload and ran again',
      run({ result: 'retried' }),
      { who: 'Reviewer', what: 'reviewed the code', result: 'stopped: provider overloaded' },
    ],
    [
      'a preparation turn under way, before the step it prepares',
      run({
        role: 'lead',
        step: 'codebase-analysis',
        result: 'running',
        endedAt: null,
        preparation: 'unsettled',
      }),
      { who: 'Lead', what: 'is checking whether to ask you anything before reading the code' },
    ],
    [
      'a preparation turn that asked the owner',
      run({ role: 'lead', step: 'codebase-analysis', preparation: 'asked' }),
      { who: 'Lead', what: 'asked you a question before reading the code' },
    ],
    [
      'a preparation turn that told the owner without waiting',
      run({ role: 'lead', step: 'scope-check', preparation: 'told' }),
      { who: 'Lead', what: 'told you something before deciding which documents it needs' },
    ],
    [
      'a preparation turn that found nothing to ask',
      run({ role: 'lead', step: 'requirements', preparation: 'nothing-to-ask' }),
      { who: 'Lead', what: 'found nothing to ask you before reading the issue' },
    ],
    [
      'a preparation turn that failed',
      run({ role: 'lead', step: 'design', result: 'failed', preparation: 'unsettled' }),
      {
        who: 'Lead',
        what: 'checked whether to ask you anything before writing the design',
        result: 'failed',
      },
    ],
  ])('%s', (_, given, expected) => {
    expect(runSentence(given)).toEqual(expected)
  })
})

const line = (kind: string, fields: EventLine['fields'] = {}): EventLine => ({
  seq: 0,
  at: '2026-11-27T08:55:00Z',
  kind,
  fields,
  issue: null,
  sprint: null,
  error: null,
})

describe('eventSentence', () => {
  it.each([
    ['a kind with no sentence of its own, as its code', line('replay-reset'), 'Replay reset.'],
    [
      'an issue marked done, naming the tracker',
      line('closed-on-tracker', { issue: 'ACME-141', state: 'done', tracker: 'linear' }),
      'Marked done on Linear.',
    ],
    [
      'an issue canceled on the tracker',
      line('closed-on-tracker', { issue: 'ACME-141', state: 'canceled', tracker: 'github' }),
      'Canceled on GitHub Issues.',
    ],
    [
      'GitHub Issues answering again, apart from GitHub',
      line('error-cleared', {
        tracker: 'github',
        count: 2,
        failure: { key: 'tracker-unreachable', since: '2026-11-27T04:00:00Z' },
      }),
      'GitHub Issues answers again.',
    ],
    [
      'GitHub answering again',
      line('error-cleared', {
        count: 2,
        failure: { key: 'github-unreachable', since: '2026-11-27T04:00:00Z' },
      }),
      'GitHub answers again.',
    ],
    [
      'a close that records neither state nor tracker',
      line('closed-on-tracker', { issue: 'ACME-141' }),
      'Closed on the tracker.',
    ],
    [
      'error notices turned off',
      line('config-reloaded', { changes: [{ setting: 'notices.errors', value: false }] }),
      'Settings changed: Error notices are now off.',
    ],
    [
      'a number setting, by its Settings label',
      line('config-reloaded', { changes: [{ setting: 'limits.issues_at_once', value: 3 }] }),
      'Settings changed: Issues at once is now 3.',
    ],
    [
      'a text setting, which records no value',
      line('config-reloaded', { changes: [{ setting: 'tracker.filter' }] }),
      'Settings changed: The issue filter command was changed.',
    ],
    [
      'a setting the dashboard has no label for, by its path',
      line('config-reloaded', { changes: [{ setting: 'future.knob', value: true }] }),
      'Settings changed: future.knob is now on.',
    ],
    [
      'several changes in one reload',
      line('config-reloaded', {
        changes: [{ setting: 'notices.errors', value: true }, { setting: 'merge.by' }],
      }),
      'Settings changed: Error notices are now on. Who merges was changed.',
    ],
    [
      'a merge with its pull request and base',
      line('merged', { issue: 'ACME-141', pr: 86, base: 'main', sha: 'a', merge_commit: 'b' }),
      'Merged pull request #86 into main.',
    ],
    [
      'a merge whose read named no base',
      line('merged', { issue: 'ACME-141', pr: 86, sha: 'a', merge_commit: 'b' }),
      'Merged pull request #86.',
    ],
    [
      'a merge recorded before the number was',
      line('merged', { issue: 'ACME-141', sha: 'a', merge_commit: 'b' }),
      'Merged its pull request.',
    ],
    [
      "a sprint's merge recorded before the number was",
      {
        ...line('merged', { sprint: '20261127-0902', sha: 'a', merge_commit: 'b' }),
        sprint: { id: '20261127-0902', purpose: null, listed: true },
      },
      "Merged the working agreements' pull request.",
    ],
    [
      'a review round with findings',
      line('review-round-ended', { issue: 'ACME-142', round: 1, findings: 3 }),
      'Review round 1 ended with 3 findings.',
    ],
    [
      'a review round with one finding',
      line('review-round-ended', { issue: 'ACME-142', round: 2, findings: 1 }),
      'Review round 2 ended with 1 finding.',
    ],
    [
      'a review round with none',
      line('review-round-ended', { issue: 'ACME-142', round: 3, findings: 0 }),
      'Review round 3 ended with 0 findings.',
    ],
    [
      'a step finishing, as an issue fact still reads',
      line('fact', {
        issue: 'ACME-142',
        fact: 'Done',
        from: { stage: 'running', step: 'code-review' },
        to: { stage: 'reviewing' },
      }),
      'Code review finished.',
    ],
    [
      'a sprint proposal Slack accepted, with its summary',
      line('decision', {
        id: 'sprint-20261127-0902-sprint-1',
        kind: 'sprint',
        issue: null,
        sprint: '20261127-0902',
        posted: true,
        proposal: { issues: 1, purpose: 'Search works.' },
      }),
      'Proposed the sprint in Slack: 1 issue toward “Search works.”',
    ],
    [
      'a posted sprint decision without a summary',
      line('decision', { id: 'x', kind: 'sprint', issue: null, sprint: 'y', posted: true }),
      'Proposed the sprint in Slack.',
    ],
    [
      'another posted decision, by its kind',
      line('decision', { id: 'x', kind: 'design', issue: 'ACME-144', posted: true }),
      'Asked you in Slack to approve its design.',
    ],
    [
      'a sprint proposal recorded before it was sent',
      line('decision', { id: 'x', kind: 'sprint', issue: null }),
      'Prepared a sprint proposal.',
    ],
    [
      'another decision recorded before it was sent',
      line('decision', { id: 'x', kind: 'design', issue: 'ACME-144' }),
      'Prepared a decision for you.',
    ],
    [
      "the owner's pause with their words",
      line('paused', { reason: 'lunch', user: 'U0123456789' }),
      'Paused. You sent !pause in Slack: “lunch”.',
    ],
    [
      "the owner's pause without words",
      line('paused', { reason: 'paused by the owner', user: 'U0123456789' }),
      'Paused. You sent !pause in Slack.',
    ],
    [
      "the factory's own pause, which names no owner",
      line('paused', { reason: 'the agent on ACME-1 (requirements) stopped with an error: x' }),
      'Paused: “the agent on ACME-1 (requirements) stopped with an error: x”.',
    ],
  ])('words %s', (_, given, expected) => {
    expect(plain(eventSentence(given))).toBe(expected)
  })
})

describe('errorName', () => {
  const going = (key: ErrorNow['key']): ErrorNow => ({
    key,
    since: '2026-11-27T04:00:00Z',
    last: '2026-11-27T04:05:00Z',
    times: 2,
    firstIssue: null,
    cause: 'no answer',
    tracker: 'github',
    cli: null,
    command: null,
  })

  it.each([
    [
      'GitHub Issues out of reach, by the tracker it is',
      'tracker-unreachable',
      "Can't reach GitHub Issues",
    ],
    ['GitHub out of reach', 'github-unreachable', "Can't reach GitHub"],
  ] as const)('names %s', (_, key, expected) => {
    expect(errorName(going(key))).toBe(expected)
  })
})

const inProgress: Position = {
  section: 'in-progress',
  phase: 'design',
  activity: null,
  task: null,
  next: null,
  problem: null,
  onHold: false,
  since: '2026-11-21T14:12:00Z',
  wait: null,
  ended: null,
  endedAt: null,
  feedback: [],
}

const unsettled = (feedback: Step[]): Position => ({
  ...inProgress,
  problem: { cause: 'unsettled', step: feedback[0] ?? 'design-review' },
  feedback,
})

describe('activityWords for reviews whose findings the lead addresses', () => {
  it.each<[Step[], string]>([
    [['design-review'], 'Addressing design review feedback'],
    [['prd-review'], 'Addressing PRD review feedback'],
    [['ui-spec-review'], 'Addressing UI Spec review feedback'],
    [['design-verify'], 'Addressing design check feedback'],
    [['security-review'], 'Addressing security review feedback'],
    [['code-review', 'security-review'], 'Addressing code and security review feedback'],
  ])('%j reads "%s"', (feedback, words) => {
    expect(activityWords(unsettled(feedback))).toBe(words)
  })
})

describe('statusRun', () => {
  const review = run({ id: 'review.2', step: 'design-review', result: 'running', endedAt: null })
  const lead = run({
    id: 'exception.2',
    role: 'lead',
    step: 'exception',
    result: 'running',
    endedAt: null,
  })
  const asked = run({ id: 'exception.1', role: 'lead', step: 'exception' })
  const reviewed = run({ id: 'review.1', step: 'design-review', result: 'findings' })

  it("names the lead's running exception run for reviews whose findings remain", () => {
    expect(statusRun(unsettled(['design-review']), [review, lead, reviewed])?.id).toBe(
      'exception.2'
    )
  })

  it("names no run while only a review runs before the lead's run starts", () => {
    expect(statusRun(unsettled(['design-review']), [review, asked, reviewed])).toBeUndefined()
  })

  it("names no run while every run has ended, the lead's earlier one newest", () => {
    expect(statusRun(unsettled(['design-review']), [asked, reviewed])).toBeUndefined()
  })

  it('names the running run of any other status, else the newest', () => {
    const now: Position = { ...inProgress, activity: 'reviewing-the-design' }
    expect(statusRun(now, [asked, review, reviewed])?.id).toBe('review.2')
    expect(statusRun(now, [asked, reviewed])?.id).toBe('exception.1')
  })
})
