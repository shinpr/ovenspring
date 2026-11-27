// Calls to the dashboard server. The cookie the server set when the dashboard was opened goes with
// each call. The answers' types are generated from the server's own (`types.gen.ts`), and the
// dashboard is served by the binary that answers it, so their shape is not checked again here.

import type {
  ApiErrors,
  CancelRequest,
  EndedSprint,
  EventsPage,
  GoalSaved,
  IssueDetail,
  LaunchDetail,
  LaunchRow,
  Overview,
  RunRequest,
  Settings,
  SettingsSection,
  SetupCheck,
  SetupFound,
  SetupRequest,
  SetupState,
  Skills,
  VersionState,
} from './types.gen.ts'

export class ApiError extends Error {
  override readonly name = 'ApiError'
  readonly status: number
  /** Why, by field or section, or by `request` for the request as a whole. */
  readonly errors: Readonly<Record<string, string>>

  constructor(status: number, errors: Readonly<Record<string, string>>) {
    super(`the dashboard server answered ${status}`)
    this.status = status
    this.errors = errors
  }
}

/** Sends a request and returns its answer; a refusal throws with its reasons. */
async function send(
  method: 'GET' | 'PUT' | 'POST',
  path: string,
  body?: unknown
): Promise<Response> {
  const init: RequestInit = { method, credentials: 'same-origin' }
  if (body !== undefined) {
    init.headers = { 'Content-Type': 'application/json' }
    init.body = JSON.stringify(body)
  }
  const response = await fetch(`/api/v1${path}`, init)
  if (!response.ok) {
    const refused: Partial<ApiErrors> = await response.json().catch(() => ({}))
    throw new ApiError(response.status, refused.errors ?? {})
  }
  return response
}

async function call<T>(method: 'GET' | 'PUT' | 'POST', path: string, body?: unknown): Promise<T> {
  const answer: T = await (await send(method, path, body)).json()
  return answer
}

const query = (filter: Record<string, string | undefined>): string => {
  const set = Object.entries(filter).filter((e): e is [string, string] => e[1] !== undefined)
  return set.length === 0 ? '' : `?${new URLSearchParams(set).toString()}`
}

export const api = {
  overview: (): Promise<Overview> => call('GET', '/overview'),
  issue: (key: string): Promise<IssueDetail> => call('GET', `/issues/${encodeURIComponent(key)}`),
  sprints: (): Promise<EndedSprint[]> => call('GET', '/sprints'),
  launches: (filter: { sprint?: string; issue?: string }): Promise<LaunchRow[]> =>
    call('GET', `/launches${query(filter)}`),
  /** The recorded events the filter keeps, from line `from` on, or the newest when it is unset. */
  events: (filter: { show?: string; issue?: string; from?: string }): Promise<EventsPage> =>
    call('GET', `/events${query(filter)}`),
  launch: (id: string): Promise<LaunchDetail> => call('GET', `/launches/${encodeURIComponent(id)}`),
  /** The launch's output as its CLI printed it. */
  logUrl: (id: string): string => `/api/v1/launches/${encodeURIComponent(id)}/log`,
  settings: (): Promise<Settings> => call('GET', '/settings'),
  /** Saves one section; a refusal carries the reasons by field or section. */
  saveSettings: (section: SettingsSection, body: unknown): Promise<Settings> =>
    call('PUT', `/settings/${section}`, body),
  skills: (): Promise<Skills> => call('GET', '/skills'),
  /** The version running, and a newer one installed or released. */
  version: (): Promise<VersionState> => call('GET', '/version'),
  /** Asks the factory to cancel the issue; it ends on the factory's next pass. */
  cancelIssue: async (key: string, words: string): Promise<void> => {
    const request: CancelRequest = { words }
    await send('POST', `/issues/${encodeURIComponent(key)}/cancel`, request)
  },
  saveGoal: async (text: string): Promise<string> =>
    (await call<GoalSaved>('PUT', '/goal', { text })).goal,
  setupState: (): Promise<SetupState> => call('GET', '/setup'),
  /** Checks one Connect block without saving; a refusal carries the reason under the block's name. */
  checkSetup: async (block: SetupCheck): Promise<string> =>
    (await call<SetupFound>('POST', '/setup/check', block)).found,
  saveSetup: (request: SetupRequest): Promise<SetupState> => call('PUT', '/setup', request),
  /** Settles how the product runs; changed commands are started once first. */
  saveRun: (request: RunRequest): Promise<SetupState> => call('PUT', '/setup/run', request),
  /** The records of `scope` (`sprint:<id>` or `issue:<key>`) as the file the server names. */
  download: async (scope: string): Promise<{ file: Blob; name: string }> => {
    const response = await fetch(`/api/v1/export?scope=${encodeURIComponent(scope)}`, {
      credentials: 'same-origin',
    })
    if (!response.ok) {
      throw new ApiError(response.status, {})
    }
    const disposition = response.headers.get('content-disposition') ?? ''
    const name = /filename="([^"]+)"/.exec(disposition)?.[1] ?? 'ovenspring.jsonl'
    return { file: await response.blob(), name }
  },
}
