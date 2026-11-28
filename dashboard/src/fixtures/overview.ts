// The overview in the situations the golden states show, as the server answers it.

import type { IssueRow, Overview, Position, Status } from '../api/types.gen.ts'

/** The moment the fixtures are seen at, so durations read the same every run. */
export const NOW = new Date('2026-11-21T14:20:00Z')

export const GOAL = 'Shoppers find and buy the right product without having to ask support.'

const status: Status = {
  state: 'working',
  reason: null,
  prerequisites: [],
  unreachable: null,
  configError: null,
  reviewPart: null,
}

const place: Position = {
  section: 'in-progress',
  phase: null,
  activity: null,
  task: null,
  next: null,
  problem: null,
  onHold: false,
  since: null,
  wait: null,
  ended: null,
  endedAt: null,
}

const row = (key: string, title: string, now: Partial<Position>): IssueRow => ({
  key,
  title,
  now: { ...place, ...now },
})

const issues: IssueRow[] = [
  row('ACME-141', 'Search products by their attributes', {
    phase: 'review',
    activity: 'applying-findings',
    next: { follows: 'push', ifApplied: 'quality-then-rechecks' },
    since: '2026-11-21T14:17:00Z',
  }),
  row('ACME-142', 'Results page for attribute search', { section: 'you', phase: 'design' }),
  row('ACME-143', 'Index product descriptions for search', {
    section: 'done',
    ended: 'merged',
    endedAt: '2026-11-21T12:48:00Z',
  }),
  row('ACME-144', 'Match search words despite typos', {
    phase: 'merge',
    activity: 'waiting-for-ci',
    since: '2026-11-21T14:16:00Z',
  }),
  row('ACME-145', 'Filter results by price and size', {
    section: 'waiting-to-start',
    wait: { reason: 'after', keys: ['ACME-142'] },
  }),
  row('ACME-146', 'Remove the old search endpoint', {
    section: 'done',
    ended: 'closed',
    endedAt: '2026-11-21T10:40:00Z',
  }),
  row('ACME-147', 'Record what shoppers search for', {
    section: 'waiting-to-start',
    wait: { reason: 'place', heldBy: ['ACME-141', 'ACME-144'] },
  }),
]

export const working: Overview = {
  goal: GOAL,
  mention: 'Oven Spring',
  status,
  sprint: {
    id: '20261121-0905',
    purpose: 'Shoppers can find a product by what it does, not only by its name.',
    startedAt: '2026-11-21T09:05:00Z',
    done: 2,
    total: 7,
  },
  decisions: [
    {
      issue: 'ACME-142',
      ask: 'approve-prototype',
      title: 'Results page for attribute search',
      link: 'https://acme.slack.test/archives/C01/p1',
      delivered: true,
    },
  ],
  issues,
  tracker: 'linear',
  openIssues: 9,
  lastSprintEndedAt: '2026-11-20T15:40:00Z',
  toldWaiting: null,
  agreements: null,
}

/** The lead has a problem with one issue; the rest goes on. */
export const problem: Overview = {
  ...working,
  issues: issues.map((i) =>
    i.key === 'ACME-144'
      ? row(i.key, i.title, {
          phase: 'merge',
          problem: { cause: 'step', step: 'fix-ci' },
          since: '2026-11-21T14:05:00Z',
        })
      : i
  ),
}

export const paused: Overview = {
  ...working,
  status: {
    ...status,
    state: 'paused',
    reason: "the agent on ACME-141 (implement) stopped with an error: You've hit your usage limit",
  },
  issues: issues.map((i) =>
    i.now.section === 'in-progress' ? { ...i, now: { ...i.now, onHold: true } } : i
  ),
}

export const missing: Overview = {
  ...working,
  status: {
    ...status,
    state: 'prerequisites',
    prerequisites: [
      {
        what: 'Slack is not connected.',
        fix: 'Decisions wait, unsent, until it is. Enter the Slack tokens in Settings.',
        setting: 'slack',
      },
      {
        what: 'Linear refused the connection.',
        fix: 'Its key may have been revoked (HTTP 401). No new issue is read and no sprint is proposed until it is fixed.',
        setting: 'tracker',
      },
    ],
  },
}
