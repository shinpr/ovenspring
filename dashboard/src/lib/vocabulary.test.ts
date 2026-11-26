import { describe, expect, it } from 'vitest'
import type { LaunchRow } from '../api/types.gen.ts'
import { runSentence } from './vocabulary.ts'

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
  ])('%s', (_, given, expected) => {
    expect(runSentence(given)).toEqual(expected)
  })
})
