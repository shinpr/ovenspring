// One issue in review, its runs, and the sprints that ended, as the server answers them.

import type { EndedSprint, IssueDetail, LaunchDetail, LaunchRow } from '../api/types.gen.ts'

export const inReview: IssueDetail = {
  key: 'ACME-141',
  title: 'Search products by their attributes',
  tracker: 'https://linear.app/acme/issue/ACME-141',
  pullRequest: 'https://github.com/acme/shop/pull/88',
  prototype: null,
  now: {
    section: 'in-progress',
    phase: 'review',
    activity: 'applying-findings',
    task: null,
    problem: null,
    onHold: false,
    since: '2026-11-21T14:17:00Z',
    wait: null,
    ended: null,
    endedAt: null,
    feedback: [],
  },
  decision: null,
  canceled: null,
  scale: 'medium',
  scaleEvidence: [
    'Search reads products through the catalog API, which has no attribute filter.',
    'A new index changes the search service and its tests.',
  ],
  outcome: 'Shoppers find a product by an attribute, such as "waterproof", not only by its name.',
  nonGoals: ['Ranking by popularity'],
  path: [
    {
      phase: 'requirements',
      state: 'done',
      startedAt: '2026-11-21T09:10:00Z',
      endedAt: '2026-11-21T09:22:00Z',
    },
    {
      phase: 'design',
      state: 'done',
      startedAt: '2026-11-21T09:22:00Z',
      endedAt: '2026-11-21T11:05:00Z',
    },
    {
      phase: 'build',
      state: 'done',
      startedAt: '2026-11-21T11:05:00Z',
      endedAt: '2026-11-21T13:58:00Z',
    },
    { phase: 'review', state: 'now', startedAt: '2026-11-21T14:00:00Z', endedAt: null },
    { phase: 'merge', state: 'ahead', startedAt: null, endedAt: null },
  ],
  findings: [
    {
      source: { kind: 'review', review: 'code-review', round: 1 },
      text: 'The attribute lookup runs once per product. Load them in one query.',
      disposition: 'apply',
      answer: 'One query now loads every attribute of the page.',
    },
    {
      source: { kind: 'review', review: 'security-review', round: 1 },
      text: 'The attribute name is put into the SQL text. Bind it.',
      disposition: null,
      answer: null,
    },
    {
      source: { kind: 'comment', author: 'acme-dev' },
      text: 'Also show the matching attribute under each result.',
      disposition: 'apply',
      answer: 'The owner asked for it on the pull request.',
    },
  ],
  launches: 14,
  costUsd: 3.42,
  uncosted: 4,
}

const run = (
  id: string,
  startedAt: string,
  endedAt: string | null,
  change: Partial<LaunchRow>
): LaunchRow => ({
  id,
  startedAt,
  endedAt,
  issue: 'ACME-141',
  role: 'reviewer',
  step: 'code-review',
  phase: 'review',
  cli: 'codex',
  result: endedAt === null ? 'running' : 'done',
  costUsd: null,
  preparation: null,
  detail: NO_DETAIL,
  ...change,
})

const NO_DETAIL: LaunchRow['detail'] = {
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

/** ACME-141's runs, newest first. */
export const runs: LaunchRow[] = [
  run('20261121-141700-review-response-ACME-141.1', '2026-11-21T14:17:00Z', null, {
    role: 'executor',
    step: 'review-response',
    cli: 'claude',
    detail: { ...NO_DETAIL, judging: 4 },
  }),
  run(
    '20261121-140000-security-review-ACME-141.1',
    '2026-11-21T14:00:00Z',
    '2026-11-21T14:14:00Z',
    {
      step: 'security-review',
      result: 'findings',
      detail: { ...NO_DETAIL, found: 1 },
    }
  ),
  run('20261121-140000-code-review-ACME-141.1', '2026-11-21T14:00:00Z', '2026-11-21T14:14:00Z', {
    result: 'findings',
    detail: { ...NO_DETAIL, found: 3 },
  }),
  run('20261121-134000-quality-ACME-141.1', '2026-11-21T13:40:00Z', '2026-11-21T13:58:00Z', {
    role: 'executor',
    step: 'quality',
    phase: 'build',
    cli: 'claude',
    costUsd: 0.41,
  }),
  run('20261121-112000-implement-ACME-141.2', '2026-11-21T12:31:00Z', '2026-11-21T13:20:00Z', {
    role: 'executor',
    step: 'implement',
    phase: 'build',
    cli: 'claude',
    result: 'failed',
    costUsd: 0.9,
  }),
]

export const running: LaunchDetail = {
  ...(runs[0] ?? run('x', '2026-11-21T14:17:00Z', null, {})),
  model: 'claude-opus-5-5',
  effort: 'high',
  failure: null,
  messages: [
    'Reading the 4 findings and the change they are about.',
    'The per-product lookup is real; checking whether the SQL is built from the attribute name.',
  ],
}

export const ended: EndedSprint[] = [
  {
    id: '20261117-0900',
    purpose: 'Shoppers see why an order is late.',
    startedAt: '2026-11-17T09:00:00Z',
    endedAt: '2026-11-20T18:00:00Z',
    keyResults: [
      { text: 'A delayed order shows its reason.', met: true, evidence: 'ACME-120 merged' },
      {
        text: 'Support gets fewer "where is my order" tickets.',
        met: false,
        evidence: 'No data yet',
      },
    ],
    agreements: ['common.md'],
    merged: ['ACME-120', 'ACME-121'],
    dropped: [],
    held: ['ACME-125'],
    launches: 38,
    costUsd: 9.8,
    uncosted: 12,
  },
]

/**
 * ACME-148 after its design review ended with findings still outstanding: the lead addresses
 * them. Its runs are newest first in each situation below.
 */
export const addressing: IssueDetail = {
  ...inReview,
  key: 'ACME-148',
  title: 'Save a search to run it again',
  tracker: 'https://linear.app/acme/issue/ACME-148',
  pullRequest: null,
  now: {
    ...inReview.now,
    phase: 'design',
    activity: null,
    problem: { cause: 'unsettled', step: 'design-review' },
    feedback: ['design-review'],
    since: '2026-11-21T14:12:00Z',
  },
  findings: [],
}

const designReviewed = run(
  '20261121-135000-design-review-ACME-148.1',
  '2026-11-21T13:50:00Z',
  '2026-11-21T14:11:00Z',
  { issue: 'ACME-148', step: 'design-review', phase: 'design', result: 'findings' }
)

/** The lead's exception run, under way. */
export const addressingRun = run(
  '20261121-141200-exception-ACME-148.1',
  '2026-11-21T14:12:00Z',
  null,
  {
    issue: 'ACME-148',
    role: 'lead',
    step: 'exception',
    phase: null,
    cli: 'claude',
  }
)

/** The lead addresses the feedback in its running exception run. */
export const addressingRuns: LaunchRow[] = [addressingRun, designReviewed]

/**
 * The lead's current run has not started: the next review started between the two reads, an
 * earlier exception run ended by asking the owner, and the review it followed ended.
 */
export const notStartedRuns: LaunchRow[] = [
  run('20261121-141800-design-review-ACME-148.2', '2026-11-21T14:18:00Z', null, {
    issue: 'ACME-148',
    step: 'design-review',
    phase: 'design',
  }),
  {
    ...addressingRun,
    id: '20261121-141200-exception-ACME-148.2',
    endedAt: '2026-11-21T14:15:00Z',
    result: 'done',
  },
  designReviewed,
]

/** The lead retried, and the next design review is under way. */
export const reviewing: IssueDetail = {
  ...addressing,
  now: {
    ...addressing.now,
    activity: 'reviewing-the-design',
    problem: null,
    feedback: [],
    since: '2026-11-21T14:18:00Z',
  },
}

export const reviewingRuns: LaunchRow[] = notStartedRuns.slice(0, 2)

/** The design review could not run. */
export const reviewFailed: IssueDetail = {
  ...addressing,
  now: { ...addressing.now, problem: { cause: 'step', step: 'design-review' }, feedback: [] },
}

/** A run's detail as the server answers it. */
export const detailOf = (row: LaunchRow): LaunchDetail => ({
  ...row,
  model: null,
  effort: null,
  failure: null,
  messages: [],
})
