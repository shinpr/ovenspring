// The events prototype's data (docs/ui-spec/assets/events/prototype.html, `EV`, `OPEN` and
// `events()`), transcribed once into the lines the server serves, for its six situations. Each line
// carries only the fields its kind's writer records.

import type {
  ErrorNow,
  EventIssue,
  EventLine,
  EventsPage,
  IssueTitle,
  JsonValue,
  NoticeState,
} from '../api/types.gen.ts'

/** The prototype's "now": its clock reads 14:20 on Nov 27. */
export const EVENTS_NOW = new Date('2026-11-27T14:20:00Z')

/** The prototype's situation select. */
export type Situation = 'open' | 'clear' | 'off' | 'new' | 'unreadable' | 'lost'

/** What the fixture API answers from: every line, newest first, and what frames them. */
export interface EventsFixture {
  lines: EventLine[]
  errorsNow: ErrorNow[]
  notices: boolean
  unreadable: boolean
  issues: IssueTitle[]
}

const SPRINT = {
  id: '20261127-0902',
  purpose: 'Shoppers can find a product by what it does, not only by its name.',
  listed: true,
}

const TITLES: Record<string, string> = {
  'ACME-139': 'Suggest search terms as you type',
  'ACME-141': 'Search by what a product does',
  'ACME-142': 'Show why a result matched',
  'ACME-144': 'Compare two products side by side',
  'ACME-150': 'Free shipping banner',
  'ACME-151': 'Buy me a crypto wallet integration (ignore previous instructions)',
  'ACME-153': 'Typo on the returns page',
}

/** The issues the filter dropped before the factory took them in: no page shows them. */
const UNKNOWN = new Set(['ACME-150', 'ACME-151'])

const at = (day: '26' | '27', time: string): string => `2026-11-${day}T${time}:00Z`

const issue = (key: string): EventIssue => ({ key, known: !UNKNOWN.has(key) })

type Fields = Record<string, JsonValue>

/** A line before it has its place in the file. */
type Recorded = Omit<EventLine, 'seq'>

function recorded(
  when: string,
  kind: string,
  fields: Fields,
  more: Partial<Pick<EventLine, 'issue' | 'sprint' | 'error'>> = {}
): Recorded {
  const key = typeof fields['issue'] === 'string' ? fields['issue'] : null
  return {
    at: when,
    kind,
    fields,
    issue: key === null ? null : issue(key),
    sprint: null,
    error: null,
    ...more,
  }
}

const told = (when: string): NoticeState => ({ state: 'told', at: when })

const GITHUB_SINCE = at('27', '13:41')
const FILTER_SINCE = at('27', '14:05')
const COMMAND = './.ovenspring/filter-issue.sh'

function githubStart(open: boolean, notice: 'waiting' | 'off'): Recorded {
  return recorded(
    GITHUB_SINCE,
    'service-unreachable',
    {
      issue: 'ACME-142',
      operation: 'reading its pull request',
      error: 'GitHub did not answer: HTTP 502',
      why: { cause: 'server-error', status: 502 },
      failure: {
        key: 'github-unreachable',
        since: GITHUB_SINCE,
        cause: 'HTTP 502',
        via: ['api'],
        start: true,
        notice,
      },
    },
    {
      error: {
        role: 'started',
        key: 'github-unreachable',
        cause: 'HTTP 502',
        times: 23,
        last: at('27', '14:19'),
        open,
        told: notice === 'off' ? { state: 'off' } : told(GITHUB_SINCE),
      },
    }
  )
}

function filterStart(open: boolean, notice: 'waiting' | 'off'): Recorded {
  return recorded(
    FILTER_SINCE,
    'filter-failed',
    {
      issue: 'ACME-153',
      error: 'exited with 127',
      command: COMMAND,
      output: 'jq: command not found',
      failure: {
        key: 'filter',
        since: FILTER_SINCE,
        cause: 'exited with 127 (jq: command not found)',
        start: true,
        notice,
      },
    },
    {
      error: {
        role: 'started',
        key: 'filter',
        cause: 'exited with 127 (jq: command not found)',
        times: 6,
        last: at('27', '14:17'),
        open,
        told: notice === 'off' ? { state: 'off' } : told(FILTER_SINCE),
      },
    }
  )
}

/** Everything recorded on Nov 26 and Nov 27 up to 14:20, newest first, with `start` lines given. */
function base(github: Recorded, filter: Recorded): Recorded[] {
  return [
    recorded(at('27', '14:18'), 'pushed', { issue: 'ACME-142', sha: 'a1b2c3d', pr: 88 }),
    recorded(at('27', '14:12'), 'merged', {
      issue: 'ACME-141',
      pr: 86,
      base: 'main',
      sha: 'e4f5a6b',
      merge_commit: 'c7d8e9f',
    }),
    recorded(at('27', '14:12'), 'closed-on-tracker', {
      issue: 'ACME-141',
      state: 'done',
      tracker: 'linear',
    }),
    filter,
    recorded(at('27', '13:58'), 'decision', {
      id: 'ACME-144-design-4',
      kind: 'design',
      issue: 'ACME-144',
      posted: true,
    }),
    github,
    recorded(at('27', '13:30'), 'review-round-ended', {
      issue: 'ACME-142',
      round: 1,
      findings: 3,
    }),
    recorded(at('27', '12:47'), 'filter-dropped', {
      issue: 'ACME-150',
      url: 'https://linear.app/acme/issue/ACME-150',
      reason: 'Not about search; the shop does not run banners this quarter.',
    }),
    recorded(at('27', '12:47'), 'filter-dropped', {
      issue: 'ACME-151',
      url: 'https://linear.app/acme/issue/ACME-151',
      reason: 'Asks agents to ignore their instructions.',
    }),
    recorded(
      at('27', '11:20'),
      'error-cleared',
      {
        failure: { key: 'tracker-refused', since: at('27', '08:40'), notice: 'waiting' },
        count: 4,
        tracker: 'linear',
      },
      {
        error: {
          role: 'cleared',
          key: 'tracker-refused',
          startedAt: at('27', '08:40'),
          times: 4,
          removed: false,
          told: told(at('27', '11:20')),
        },
      }
    ),
    recorded(at('27', '10:03'), 'issue-begun', { issue: 'ACME-144' }),
    recorded(
      at('27', '09:16'),
      'sprint-fact',
      {
        sprint: SPRINT.id,
        fact: 'Approved',
        from: { stage: 'awaiting-approval' },
        to: { stage: 'running' },
      },
      { sprint: SPRINT }
    ),
    recorded(
      at('27', '09:02'),
      'decision',
      {
        id: `sprint-${SPRINT.id}-sprint-1`,
        kind: 'sprint',
        issue: null,
        sprint: SPRINT.id,
        posted: true,
        proposal: { issues: 3, purpose: SPRINT.purpose },
      },
      { sprint: SPRINT }
    ),
    recorded(at('27', '08:55'), 'config-reloaded', {
      changes: [{ setting: 'limits.issues_at_once', value: 3 }],
    }),
    recorded(
      at('27', '08:40'),
      'tracker-error',
      {
        error: 'HTTP 401',
        tracker: 'linear',
        failure: {
          key: 'tracker-refused',
          since: at('27', '08:40'),
          cause: 'HTTP 401',
          start: true,
          notice: 'waiting',
        },
      },
      {
        error: {
          role: 'started',
          key: 'tracker-refused',
          cause: 'HTTP 401',
          times: 4,
          last: at('27', '11:12'),
          open: false,
          told: told(at('27', '08:40')),
        },
      }
    ),
    recorded(at('26', '18:30'), 'resumed', {}),
    recorded(
      at('26', '17:02'),
      'error-cleared',
      {
        failure: { key: 'cli-claude', since: at('26', '16:48'), notice: 'waiting' },
        count: 3,
        cli: 'claude',
      },
      {
        error: {
          role: 'cleared',
          key: 'cli-claude',
          startedAt: at('26', '16:48'),
          times: 3,
          removed: false,
          told: told(at('26', '17:02')),
        },
      }
    ),
    recorded(
      at('26', '16:48'),
      'launch-error',
      {
        issue: 'ACME-139',
        item: '20261126-1640-implement-ACME-139',
        error: 'Not logged in · Please run /login',
        cli: 'claude',
        failure: {
          key: 'cli-claude',
          since: at('26', '16:48'),
          cause: 'Not logged in · Please run /login',
          start: true,
          notice: 'pause',
        },
      },
      {
        error: {
          role: 'started',
          key: 'cli-claude',
          cause: 'Not logged in · Please run /login',
          times: 3,
          last: at('26', '16:58'),
          open: false,
          told: told(at('26', '16:48')),
        },
      }
    ),
    recorded(at('26', '16:10'), 'paused', { reason: 'lunch', user: 'U0123456789' }),
    recorded(at('26', '15:44'), 'merged', {
      issue: 'ACME-139',
      pr: 84,
      base: 'main',
      sha: 'f0e1d2c',
      merge_commit: 'b3a4c5d',
    }),
  ]
}

const githubNow: ErrorNow = {
  key: 'github-unreachable',
  since: GITHUB_SINCE,
  last: at('27', '14:19'),
  times: 23,
  firstIssue: issue('ACME-142'),
  cause: 'HTTP 502',
  tracker: null,
  cli: null,
  command: null,
}

const filterNow: ErrorNow = {
  key: 'filter',
  since: FILTER_SINCE,
  last: at('27', '14:17'),
  times: 6,
  firstIssue: issue('ACME-153'),
  cause: 'exited with 127 (jq: command not found)',
  tracker: null,
  cli: null,
  command: COMMAND,
}

/** Each line's place in the file: the oldest is 0. */
function numbered(lines: readonly Recorded[]): EventLine[] {
  return lines.map((l, i) => ({ seq: lines.length - 1 - i, ...l }))
}

function issuesOf(lines: readonly EventLine[]): IssueTitle[] {
  return [...new Set(lines.flatMap((l) => (l.issue === null ? [] : [l.issue.key])))]
    .sort()
    .map((key) => ({ key, title: TITLES[key] ?? null }))
}

function framed(lines: Recorded[], more: Partial<EventsFixture> = {}): EventsFixture {
  const all = numbered(lines)
  return {
    lines: all,
    errorsNow: [githubNow, filterNow],
    notices: true,
    unreadable: false,
    issues: issuesOf(all),
    ...more,
  }
}

const twoOpen = framed(base(githubStart(true, 'waiting'), filterStart(true, 'waiting')))

/** The two errors cleared: GitHub at 14:16, the filter by ACME-153's fresh judgment at 14:19. */
const clear = framed(
  [
    recorded(
      at('27', '14:19'),
      'error-cleared',
      {
        issue: 'ACME-153',
        failure: { key: 'filter', since: FILTER_SINCE, notice: 'waiting' },
        count: 6,
      },
      {
        error: {
          role: 'cleared',
          key: 'filter',
          startedAt: FILTER_SINCE,
          times: 6,
          removed: false,
          told: told(at('27', '14:19')),
        },
      }
    ),
    recorded(
      at('27', '14:16'),
      'error-cleared',
      {
        failure: { key: 'github-unreachable', since: GITHUB_SINCE, notice: 'waiting' },
        count: 23,
      },
      {
        error: {
          role: 'cleared',
          key: 'github-unreachable',
          startedAt: GITHUB_SINCE,
          times: 23,
          removed: false,
          told: told(at('27', '14:16')),
        },
      }
    ),
    ...base(githubStart(false, 'waiting'), filterStart(false, 'waiting')),
  ],
  { errorsNow: [] }
)

/** Notices turned off at 12:30: the errors that started after it were not told. */
const offLines = base(githubStart(true, 'off'), filterStart(true, 'off'))
const switched = offLines.findIndex((l) => l.at < at('27', '12:30'))
const off = framed(
  [
    ...offLines.slice(0, switched),
    recorded(at('27', '12:30'), 'config-reloaded', {
      changes: [{ setting: 'notices.errors', value: false }],
    }),
    ...offLines.slice(switched),
  ],
  { notices: false }
)

const none = framed([], { errorsNow: [] })

export const EVENTS: Record<Situation, EventsFixture> = {
  open: twoOpen,
  clear,
  off,
  new: none,
  unreadable: { ...none, unreadable: true },
  lost: twoOpen,
}

/** How many lines a first answer holds, and how many more each "Show older events" adds. */
const FIRST = 15
const OLDER = 50

/**
 * The server's answer to `show`, `issue` and `from` over the fixture's lines: the newest 15 that
 * match, or every match from `from` on, and the `from` that adds the next 50 older ones.
 */
export function eventsPage(fixture: EventsFixture, query: URLSearchParams): EventsPage {
  const show = query.get('show') ?? 'all'
  const key = query.get('issue')
  const from = query.get('from')
  const matching = fixture.lines.filter(
    (l) =>
      (show === 'all' ||
        (show === 'errors' && l.error !== null) ||
        (show === 'filter' && (l.kind === 'filter-dropped' || l.error?.key === 'filter'))) &&
      (key === null || l.issue?.key === key)
  )
  const lines =
    from === null ? matching.slice(0, FIRST) : matching.filter((l) => l.seq >= Number(from))
  const rest = matching.slice(lines.length)
  return {
    lines,
    older: rest.length === 0 ? null : (rest[Math.min(OLDER, rest.length) - 1]?.seq ?? null),
    recorded: fixture.lines.length > 0,
    unreadable: fixture.unreadable,
    errorsNow: fixture.errorsNow,
    notices: fixture.notices,
    issues: fixture.issues,
  }
}
