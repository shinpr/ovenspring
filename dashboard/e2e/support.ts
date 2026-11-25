// The fixture API the e2e suites run the production bundle against. The fixtures are typed by the
// server's generated types, so they are sent as they are.

import type { Page, Route } from '@playwright/test'
import type {
  IssueDetail,
  LaunchDetail,
  LaunchRow,
  Overview,
  Settings,
  SetupState,
  SetupStep,
  VersionState,
} from '../src/api/types.gen.ts'
import { ended, inReview, running, runs } from '../src/fixtures/issue.ts'
import { NOW, working } from '../src/fixtures/overview.ts'
import { settings, skills } from '../src/fixtures/settings.ts'
import { started } from '../src/fixtures/setup.ts'

export interface Fixtures {
  /** The overview, or `null` for a factory that does not answer. */
  overview: Overview | null
  issue: IssueDetail
  launches: LaunchRow[]
  launch: LaunchDetail
  settings: Settings
  /** What a save answers, by settings section; a section left out saves. */
  refuse: Record<string, Record<string, string>>
  /** The setup step the repository is at; each step's save moves it to the next. */
  step: SetupStep
  /** Why the configuration cannot be used, as setup and the overview both report it. */
  configError: string | null
  /** The version running, and a newer one to move to. */
  version: VersionState
}

export interface Sent {
  method: string
  /** Path and query under `/api/v1`. */
  path: string
  body: unknown
}

/**
 * Answers `/api/v1/**` from fixtures and records every request the app sends. A saved goal is
 * served back from then on, as the server would.
 */
export async function serve(page: Page, given: Partial<Fixtures> = {}): Promise<Sent[]> {
  const f: Fixtures = {
    overview: working,
    issue: inReview,
    launches: runs,
    launch: running,
    settings,
    refuse: {},
    step: 'done',
    configError: null,
    version: { running: '0.1.0', update: null },
    ...given,
  }
  let goal = f.overview?.goal ?? ''
  let step = f.step
  const setup = (): SetupState => ({
    step,
    repository: 'storefront',
    origin: 'acme/storefront',
    error: f.configError,
    slackManifest: '{}',
    run: step === 'run' ? started : null,
  })
  const NEXT: Record<string, SetupStep> = { '/setup': 'run', '/setup/run': 'goal', '/goal': 'done' }
  const sent: Sent[] = []
  const answers: Record<string, (query: URLSearchParams) => unknown> = {
    '/setup': setup,
    '/setup/check': () => ({ found: 'Connected.' }),
    [`/issues/${f.issue.key}`]: () => f.issue,
    '/sprints': () => ended,
    '/launches': (query) => {
      const issue = query.get('issue')
      return issue === null ? f.launches : f.launches.filter((r) => r.issue === issue)
    },
    [`/launches/${f.launch.id}`]: () => f.launch,
    '/settings': () => f.settings,
    '/skills': () => skills,
    '/version': () => f.version,
  }
  const write = async (route: Route, path: string, body: unknown): Promise<void> => {
    const next = NEXT[path]
    if (next !== undefined && step !== 'done') {
      step = next
    }
    if (path === '/setup' || path === '/setup/run') {
      await route.fulfill({ json: setup() })
      return
    }
    if (path === '/goal') {
      goal = typeof body === 'object' && body !== null && 'text' in body ? String(body.text) : ''
      await route.fulfill({ json: { goal } })
      return
    }
    const refused = f.refuse[path.replace('/settings/', '')]
    await route.fulfill(
      refused === undefined ? { json: f.settings } : { status: 400, json: { errors: refused } }
    )
  }
  await page.clock.setFixedTime(NOW)
  await page.route('**/api/v1/**', async (route: Route) => {
    const request = route.request()
    const url = new URL(request.url())
    const path = url.pathname.replace('/api/v1', '')
    const body: unknown = request.postDataJSON()
    sent.push({ method: request.method(), path: `${path}${url.search}`, body })
    if (request.method() === 'PUT') {
      await write(route, path, body)
    } else if (path === '/overview') {
      await route.fulfill(
        f.overview === null
          ? { status: 500, json: { errors: { request: 'down' } } }
          : {
              json: {
                ...f.overview,
                goal,
                // As the server does: an unusable configuration is the factory's state.
                status:
                  f.configError === null
                    ? f.overview.status
                    : {
                        ...f.overview.status,
                        state: 'settings-not-applied',
                        configError: f.configError,
                      },
              },
            }
      )
    } else {
      const answer = answers[path]
      await route.fulfill(
        answer === undefined
          ? { status: 404, json: { errors: { request: 'No such fixture.' } } }
          : { json: answer(url.searchParams) }
      )
    }
  })
  return sent
}
