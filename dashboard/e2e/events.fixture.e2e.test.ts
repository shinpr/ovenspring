// @lane: fixture-e2e
// Area: Events (S-08), the header's Events link, the Error notices row in Settings, and their match
// with the binding prototype.
//
// Governing sources: docs/design/events-and-error-notices-design.md ("Design"), § Acceptance
// Criteria > Events page and Look, § Client State Design, § Selected Design > Sprints anchor,
// § Verification Strategy > Visual comparison; docs/ui-spec/oven-spring-ui-spec.md ("UI Spec") S-08,
// EventsPage, EventFilters, EventLine, ErrorNoticesRow, Keyboard Navigation, Visual Acceptance
// golden states 9-15; docs/prd/oven-spring-prd.md AC-076, AC-078.
//
// Proof contract shared by every test here: the production bundle in Chromium (playwright.config.ts,
// 1280x800, UTC), reached through the real router, `Shell` (its `area()`), `AppHeader`, `usePoll`
// and the API client. Only `/api/v1/**` is substituted, through `serve` in `e2e/support.ts`, whose
// `events` answer applies the window rule of Design § Events read step 6 to the lines of
// src/fixtures/events.ts. `events.jsonl` reading, attribution, folding and masking are proven by
// crates/cli/tests/errors_int.rs test 11 and are not re-proven here; each test below proves what
// only the browser shows: navigation, polling, the anchored window, focus, scroll and layout.
// Wording per kind (vocabulary.ts) and day headings (time.ts) are owned by their vitest tests;
// each state's markup and axe by the EventsView stories.
import { expect, type Locator, type Page, test } from '@playwright/test'
import type { EventLine } from '../src/api/types.gen.ts'
import {
  EVENTS,
  EVENTS_NOW,
  type EventsFixture,
  eventsPage,
  type Situation,
} from '../src/fixtures/events.ts'
import { ended, inReview } from '../src/fixtures/issue.ts'
import { working } from '../src/fixtures/overview.ts'
import { serve } from './support.ts'

/** A copy of a situation's lines a test can change while the page polls. */
const copy = (fixture: EventsFixture): EventsFixture => structuredClone(fixture)

/** The fixture's lines renumbered after a test added some: the oldest is 0. */
function renumber(fixture: EventsFixture): void {
  fixture.lines = fixture.lines.map((l, i) => ({ ...l, seq: fixture.lines.length - 1 - i }))
}

const lines = (page: Page): Locator =>
  page.getByRole('region', { name: 'What the factory did' }).getByRole('listitem')

const eventRequests = (sent: ReadonlyArray<{ path: string }>): string[] =>
  sent.map((r) => r.path).filter((p) => p.startsWith('/events'))

/** A line recorded at `at`, as `committed` records it. */
const committed = (at: string, issue = 'ACME-142'): EventLine => ({
  seq: 0,
  at,
  kind: 'committed',
  fields: { issue, after: 'quality', sha: 'abc1234' },
  issue: { key: issue, known: true },
  sprint: null,
  error: null,
})

// ---------------------------------------------------------------------------------------------
// 1.
// AC: "When the owner opens Events, the system shall list the newest 15 matching lines of
//     `events.jsonl`, newest first. Each line has its time, its kind's sentence and its subject."
//     and "When the issue filter drops an issue, the Events list shall show a drop line with the
//     key, the masked reason and a link named after the recorded URL's tracker. When the filter
//     fails, it shall show an Error line with the key, the command, what failed and the first line
//     of its stderr." (Design § Acceptance Criteria > Events page; PRD AC-076, AC-072)
// Behavior: from Now, the owner follows the header's "Events" link -> the Events page answers
//     with a fixture of more than 15 lines including a `filter-dropped` line with a GitHub URL and
//     a `filter` start line with `command` and `output`.
// @dependency: dashboard app (router, Shell, AppHeader, EventsPage, usePoll), fixture API
test('Events lists the newest lines, the drops and the filter failures, from the header link', async ({
  page,
}) => {
  const fixture = copy(EVENTS.open)
  const drop = fixture.lines.find((l) => l.issue?.key === 'ACME-150')
  if (drop === undefined) {
    throw new Error('the fixture holds no drop of ACME-150')
  }
  drop.fields['url'] = 'https://github.com/acme/acme-shop/issues/150'
  const sent = await serve(page, { events: fixture, now: EVENTS_NOW })
  await page.goto('/')

  await page.getByRole('banner').getByRole('link', { name: 'Events' }).click()

  await expect(page).toHaveURL('/events')
  const nav = page.getByRole('navigation', { name: 'Main' })
  await expect(nav.getByRole('link', { name: 'Events' })).toHaveAttribute('aria-current', 'page')
  await expect(nav.locator('[aria-current]')).toHaveCount(1)
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Events')
  await expect(lines(page)).toHaveCount(15)
  await expect(lines(page).first()).toContainText('Pushed its branch and updated pull request #88.')
  const dropLine = lines(page).filter({ hasText: 'The issue filter dropped ACME-150' })
  await expect(dropLine).toContainText(
    'The issue filter dropped ACME-150: Not about search; the shop does not run banners this quarter.'
  )
  await expect(
    dropLine.getByRole('link', { name: 'GitHub issue (opens in a new tab)' })
  ).toHaveAttribute('href', 'https://github.com/acme/acme-shop/issues/150')
  const failure = lines(page).filter({ hasText: 'The issue filter could not judge ACME-153' })
  await expect(failure).toContainText(
    'Error The issue filter could not judge ACME-153: ./.ovenspring/filter-issue.sh exited with 127 (jq: command not found).'
  )
  await expect(failure.getByRole('link', { name: 'ACME-153' })).toBeVisible()
  const oldestShown = Math.min(
    ...eventsPage(fixture, new URLSearchParams()).lines.map((l) => l.seq)
  )
  await expect.poll(() => eventRequests(sent).length, { timeout: 10_000 }).toBeGreaterThan(1)
  expect(eventRequests(sent).slice(0, 2)).toEqual(['/events', `/events?from=${oldestShown}`])
})

// ---------------------------------------------------------------------------------------------
// 2.
// AC: "If lines are recorded and none matches the filters, then the page shall show 'No event
//     matches this filter.'; if no line is recorded, then it shall show 'Nothing has been recorded
//     yet.'" (Design § Acceptance Criteria > Events page); UI Spec EventFilters and Keyboard
//     Navigation: "Changes the filter; the list updates without moving focus".
// @dependency: dashboard app (EventFilters, URL query, usePoll key), fixture API
test.describe('filters narrow the list from the URL and say when nothing matches or nothing is recorded', () => {
  test("the issue filter's lines of one issue, chosen from the focused selects", async ({
    page,
  }) => {
    const sent = await serve(page, { events: EVENTS.open, now: EVENTS_NOW })
    await page.goto('/events')
    await expect(lines(page)).toHaveCount(15)

    const show = page.getByRole('combobox', { name: 'Show' })
    await show.focus()
    await show.selectOption({ label: "The issue filter's drops and failures" })
    const issue = page.getByRole('combobox', { name: 'Issue' })
    await issue.focus()
    await issue.selectOption('ACME-153')

    await expect(page).toHaveURL('/events?show=filter&issue=ACME-153')
    await expect(lines(page)).toHaveCount(1)
    await expect(lines(page)).toContainText('The issue filter could not judge ACME-153')
    await expect(issue).toBeFocused()
    expect(eventRequests(sent)).toContain('/events?show=filter&issue=ACME-153')
  })

  test('lines are recorded and none matches', async ({ page }) => {
    await serve(page, { events: EVENTS.open, now: EVENTS_NOW })
    await page.goto('/events?show=filter&issue=ACME-144')

    await expect(page.getByText('No event matches this filter.')).toBeVisible()
  })

  test('nothing is recorded', async ({ page }) => {
    await serve(page, { events: EVENTS.new, now: EVENTS_NOW })
    await page.goto('/events')

    await expect(page.getByText('Nothing has been recorded yet.')).toBeVisible()
  })
})

// ---------------------------------------------------------------------------------------------
// 3.
// AC: "While the page is open, when a new line is recorded, the system shall add it at the top at
//     the next poll and keep every line already shown." (Design § Acceptance Criteria > Events page)
// Behavior: the page shows its first 15 lines -> the fixture gains one newer line -> the next poll
//     (the 5 s interval, waited for in real time).
// @dependency: dashboard app (EventsPage, usePoll, anchored `from`), fixture API
test('a line recorded while the page is open is added at the top and the shown lines stay', async ({
  page,
}) => {
  const fixture = copy(EVENTS.open)
  await serve(page, { events: fixture, now: EVENTS_NOW })
  await page.goto('/events')
  await expect(lines(page)).toHaveCount(15)
  const before = await lines(page).allInnerTexts()

  fixture.lines.unshift(committed('2026-11-27T14:19:30Z'))
  renumber(fixture)

  await expect(lines(page)).toHaveCount(16, { timeout: 10_000 })
  await expect(lines(page).first()).toContainText('Committed its work.')
  expect((await lines(page).allInnerTexts()).slice(1)).toEqual(before)
})

// ---------------------------------------------------------------------------------------------
// 4.
// AC: "When the owner chooses 'Show older events', the system shall add the next 50 older matching
//     lines below and keep the lines already shown, new arrivals included, while the older ones
//     load." (Design § Acceptance Criteria > Events page); UI Spec Keyboard Navigation: "focus stays
//     on the button, or moves to the first new line when the button goes away".
// Behavior: 15 lines shown and more than 15 match -> the answer to `from=<older>` is delayed -> the
//     owner activates "Show older events" with Enter -> the delayed answer arrives.
// @dependency: dashboard app (EventsPage, `from` ref and `refresh()`), fixture API with delay
test.describe('Show older events adds older lines below and keeps the shown ones while they load', () => {
  test('more than 50 older lines: 50 come, and focus stays on the button', async ({ page }) => {
    const fixture = copy(EVENTS.open)
    for (let minute = 0; minute < 60; minute += 1) {
      fixture.lines.push(committed(`2026-11-25T10:${String(59 - minute).padStart(2, '0')}:00Z`))
    }
    renumber(fixture)
    const older = eventsPage(fixture, new URLSearchParams()).older
    await serve(page, {
      events: fixture,
      now: EVENTS_NOW,
      delay: { [`/events?from=${older}`]: 2000 },
    })
    await page.goto('/events')
    await expect(lines(page)).toHaveCount(15)

    const button = page.getByRole('button', { name: 'Show older events' })
    await button.focus()
    await button.press('Enter')

    await expect(lines(page)).toHaveCount(15)
    await expect(lines(page)).toHaveCount(65, { timeout: 10_000 })
    await expect(button).toBeFocused()
  })

  test('fewer than 50 older lines: the button goes and focus moves to the first new line', async ({
    page,
  }) => {
    const older = eventsPage(EVENTS.open, new URLSearchParams()).older
    await serve(page, {
      events: EVENTS.open,
      now: EVENTS_NOW,
      delay: { [`/events?from=${older}`]: 1000 },
    })
    await page.goto('/events')
    await expect(lines(page)).toHaveCount(15)

    const button = page.getByRole('button', { name: 'Show older events' })
    await button.focus()
    await button.press('Enter')

    await expect(lines(page)).toHaveCount(EVENTS.open.lines.length, { timeout: 10_000 })
    await expect(button).toHaveCount(0)
    await expect(lines(page).nth(15)).toBeFocused()
  })

  test('a filter changed while older lines load keeps focus on the filter', async ({ page }) => {
    const older = eventsPage(EVENTS.open, new URLSearchParams()).older
    await serve(page, {
      events: EVENTS.open,
      now: EVENTS_NOW,
      delay: { [`/events?from=${older}`]: 1000 },
    })
    await page.goto('/events')
    await expect(lines(page)).toHaveCount(15)

    const button = page.getByRole('button', { name: 'Show older events' })
    await button.focus()
    await button.press('Enter')
    const show = page.getByRole('combobox', { name: 'Show' })
    await show.focus()
    // Its answer holds lines older than the oldest shown and no older ones.
    await show.selectOption({ label: 'Errors and when they cleared' })

    await expect(page).toHaveURL('/events?show=errors')
    await expect(lines(page)).toHaveCount(6)
    // Past the held-back older answer, which the new filter forgets.
    await page.waitForTimeout(1500)
    await expect(show).toBeFocused()
  })
})

// ---------------------------------------------------------------------------------------------
// 5. Subjects as links (UI Spec EventLine; Design § Selected Design > Sprints anchor).
// AC: "When the owner follows a sprint subject, S-03 shall open scrolled to that sprint's own place
//     once its data has loaded." (Design § Acceptance Criteria > Events page)
// @dependency: dashboard app (EventLine, router, SprintsPage scroll effect, Shell overview poll),
//     fixture API with per-path delay
test.describe('issue and sprint subjects lead to their pages, the sprint scrolled into view once loaded', () => {
  const sprintLine = (listed: boolean): EventLine => ({
    seq: 0,
    at: '2026-11-27T14:19:00Z',
    kind: 'merged',
    fields: { sprint: ended[0]?.id ?? '', sha: 'abc', merge_commit: 'def' },
    issue: null,
    sprint: { id: ended[0]?.id ?? '', purpose: ended[0]?.purpose ?? null, listed },
    error: null,
  })
  const withSprint = (listed: boolean): EventsFixture => {
    const fixture = copy(EVENTS.open)
    fixture.lines.unshift(sprintLine(listed))
    renumber(fixture)
    return fixture
  }

  test('an issue key the factory has a record of opens its page', async ({ page }) => {
    await serve(page, { events: EVENTS.open, now: EVENTS_NOW })
    await page.goto('/events')

    await lines(page).getByRole('link', { name: inReview.key }).first().click()

    await expect(page).toHaveURL(`/issues/${inReview.key}`)
  })

  test('an ended sprint opens Sprints at its row once the delayed answer arrives', async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1280, height: 400 })
    await serve(page, { events: withSprint(true), now: EVENTS_NOW, delay: { '/sprints': 1500 } })
    await page.goto('/events')

    await page.getByRole('link', { name: `Sprint: ${ended[0]?.purpose ?? ''}` }).click()

    await expect(page).toHaveURL(`/sprints#sprint-${ended[0]?.id ?? ''}`)
    await expect(page.locator(`[id="sprint-${ended[0]?.id ?? ''}"]`)).toBeInViewport({
      timeout: 10_000,
    })
  })

  test('the current sprint opened directly scrolls into view once the overview answers', async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1280, height: 200 })
    await serve(page, { now: EVENTS_NOW, delay: { '/overview': 2000 } })
    const id = working.sprint?.id ?? ''
    await page.goto(`/sprints#sprint-${id}`)

    await expect(page.getByRole('heading', { name: 'Ended sprints' })).toBeAttached()
    await expect(page.locator(`[id="sprint-${id}"]`)).toBeInViewport({ timeout: 10_000 })
  })

  test('a malformed fragment leaves the sprint list in place', async ({ page }) => {
    const errors: Error[] = []
    page.on('pageerror', (e) => errors.push(e))
    await serve(page, { now: EVENTS_NOW })
    await page.goto('/sprints#%E0%A4')

    await expect(page.getByRole('heading', { name: 'Ended sprints' })).toBeVisible()
    expect(errors).toEqual([])
  })

  test('a sprint Sprints does not list is named in plain text', async ({ page }) => {
    await serve(page, { events: withSprint(false), now: EVENTS_NOW })
    await page.goto('/events')

    const name = `Sprint: ${ended[0]?.purpose ?? ''}`
    await expect(lines(page).first().getByText(name)).toBeVisible()
    await expect(page.getByRole('link', { name })).toHaveCount(0)
  })
})

// ---------------------------------------------------------------------------------------------
// 6. Error notices row (UI Spec ErrorNoticesRow; Design § Data Contracts > Notices section).
// AC: "When the owner saves Error notices in Settings, the system shall store `[notices] errors`
//     without checking any connection." (Design § Acceptance Criteria > Error notices; PRD AC-078)
// Cases (reported distinctly):
//     a. on Settings > Connections, the Error notices row (after Slack, before Owners) shows On ->
//        the owner chooses Change, moves to Off with the arrow keys and saves with Enter. Result:
//        one `PUT /settings/notices` with body `{"errors": false}`; the row's summary reads Off
//        and "Applies at once." is shown.
//     b. the save is refused (`refuse.notices`). Result: the reason is shown under the radios and
//        the form stays open.
// @dependency: dashboard app (Settings, SettingRow, Choices, lib/settings.ts PAGE_OF), fixture API
test.describe('error notices are turned off from Settings and a refused save keeps the reason in view', () => {
  const row = (page: Page): Locator =>
    page
      .getByRole('listitem')
      .filter({ has: page.getByRole('button', { name: 'Change Error notices' }) })

  test('the owner turns them off with the keyboard and the row reads Off', async ({ page }) => {
    const sent = await serve(page)
    await page.goto('/settings/connections')
    const changes = page.getByRole('button', { name: /^Change / })
    await expect(changes).toHaveCount(5)
    expect(
      await changes.evaluateAll((buttons) => buttons.map((b) => b.getAttribute('aria-label')))
    ).toEqual([
      'Change GitHub',
      'Change Issue tracker',
      'Change Slack',
      'Change Error notices',
      'Change Who the factory answers',
    ])
    await expect(row(page).locator('b')).toHaveText('On')

    await page.getByRole('button', { name: 'Change Error notices' }).focus()
    await page.keyboard.press('Enter')
    await expect(page.getByRole('radio', { name: 'On (default)' })).toBeFocused()
    await page.keyboard.press('ArrowDown')
    await expect(page.getByRole('radio', { name: 'Off' })).toBeChecked()
    await expect(page.getByText('Applies at once.')).toBeVisible()
    await page.keyboard.press('Enter')

    await expect(page.getByRole('form', { name: 'Error notices' })).toBeHidden()
    expect(sent.filter((r) => r.method === 'PUT')).toEqual([
      { method: 'PUT', path: '/settings/notices', body: { errors: false } },
    ])
    await expect(row(page)).toContainText('Off. Errors still show on Events.')
    await expect(row(page).getByRole('link', { name: 'Events' })).toHaveAttribute('href', '/events')
  })

  test('a refused save shows the reason and keeps the choice', async ({ page }) => {
    await serve(page, {
      refuse: { notices: { request: 'The settings file could not be written.' } },
    })
    await page.goto('/settings/connections')

    await page.getByRole('button', { name: 'Change Error notices' }).click()
    const form = page.getByRole('form', { name: 'Error notices' })
    await form.getByRole('radio', { name: 'Off' }).check()
    await form.getByRole('button', { name: 'Save' }).click()

    await expect(form.getByRole('alert')).toHaveText('The settings file could not be written.')
    await expect(form.getByRole('radio', { name: 'Off' })).toBeChecked()
  })
})

// ---------------------------------------------------------------------------------------------
// 7. Visual comparison with the binding prototype (Design § Verification Strategy > Visual
//    comparison; UI Spec Visual Acceptance golden states 9-15).
// AC: "When the Events golden states 9-14 ... are rendered with the prototype's own data, their
//     visible words, from the lost-contact line through the end of the list, and the positions of
//     the compared boxes shall match the prototype's same situation at 1280px, with the UI Spec's
//     stated wording overrides and the Issue options derived from available events. The Events
//     list's words and layout shall also match at 600px, where the subject sits above the
//     sentence." (Design § Acceptance Criteria > Look)
// Behavior: per situation, the prototype is opened from its repository path in the same browser
//     with its situation select set, and the production page is opened under the fixture answers
//     transcribed once from the prototype's data (src/fixtures/events.ts), the clock at 14:20 on
//     2026-11-27.
// Compared, per situation at 1280px and in `open` at 600px:
//     a. the fixture's lines carry the fields the real writers emit (merged {pr, base}, review-round-
//        ended {round, findings}, decision {posted, sprint, proposal}, paused {reason, user}), so
//        the list's sentences come from writer-shaped records (crates/cli/tests/issue_flow_int.rs,
//        owner_channel_int.rs and resilience_int.rs prove the writers);
//     b. the page's words from the lost-contact line through the end of the list equal the
//        prototype's after OVERRIDES, every EventLine's words named by its place;
//     c. in `unreadable`, the reference's Issue select keeps only "All issues" (UI Spec
//        EventFilters: the options come from events read), and the rest is compared whole;
//     d. the labels, selected values and option texts of the Show and Issue selects;
//     e. every EventLine's clock, subject and sentence boxes (left edge and width within 1px), the
//        gap between the filter controls, and, with no EventLine, the filter row's and the empty
//        text's boxes measured from the page title (the header above it is not the prototype's);
//        at 600px each subject above its sentence;
//     f. state 15, the open Error notices row in Settings > Connections: its label, what-line,
//        legend, both option texts, buttons and "Applies at once.", each element's words and box
//        measured from the row's top left (the rows above it carry other data).
//     Pass: every observation equal; a difference names the element and both values.
// @dependency: dashboard production bundle, fixture API, the prototype file, Playwright clock

const PROTOTYPE = new URL('../../docs/ui-spec/assets/events/prototype.html', import.meta.url).href
const HANKEN = new URL(
  '../node_modules/@fontsource-variable/hanken-grotesk/files/hanken-grotesk-latin-wght-normal.woff2',
  import.meta.url
).href

/** The UI Spec's stated changes to the prototype's words, each with the passage that states it. */
const OVERRIDES: ReadonlyArray<{ passage: string; prototype: string; product: string }> = [
  {
    passage: 'Vocabulary > Errors, <interval>',
    prototype: 'The factory tries again every 30 seconds.',
    product: 'The factory tries again.',
  },
  {
    passage: 'Vocabulary > Errors, tracker refuses',
    prototype: 'No new issue is read and no sprint is proposed until it is fixed.',
    product: 'No new issue or change is read from it until it is fixed.',
  },
]

/** The words a locator shows, as the owner sees them: hidden notes left out, spaces normalized. */
const words = async (shown: Locator): Promise<string> =>
  (await shown.allInnerTexts()).join(' ').replace(/\s+/g, ' ').trim()

async function prototype(page: Page, situation: Situation): Promise<void> {
  await page.goto(PROTOTYPE)
  await page.locator('#sit').selectOption(situation)
  if (situation === 'unreadable') {
    // The product's Issue options come from the events it read, and it read none.
    await page.locator('#ev-issue option:not([value="all"])').evaluateAll((options) => {
      for (const option of options) {
        option.remove()
      }
    })
  }
}

async function product(page: Page, situation: Situation): Promise<void> {
  await serve(page, { events: EVENTS[situation], now: EVENTS_NOW })
  await page.goto('/events')
  await expect(page.getByRole('heading', { name: 'What the factory did' })).toBeVisible()
  if (situation === 'lost') {
    await expect(lines(page)).toHaveCount(15)
    await page.route('**/api/v1/events*', (route) =>
      route.fulfill({ status: 500, json: { errors: { request: 'down' } } })
    )
    await expect(page.getByRole('status')).toBeVisible({ timeout: 10_000 })
  }
}

/** The prototype's words with the overrides applied; an override not found fails. */
function overridden(text: string, used: Set<string>): string {
  let result = text
  for (const o of OVERRIDES) {
    if (result.includes(o.prototype)) {
      used.add(o.passage)
      result = result.replaceAll(o.prototype, o.product)
    }
  }
  return result
}

type Box = { x: number; width: number; y: number; height: number }

/** Each event line's clock, subject and sentence boxes. */
async function lineBoxes(scope: Locator): Promise<Box[][]> {
  return scope.evaluateAll((items) =>
    items.map((li) =>
      Array.from(li.children, (span) => {
        const r = span.getBoundingClientRect()
        return { x: r.x, width: r.width, y: r.y, height: r.height }
      })
    )
  )
}

function expectSameColumns(ours: Box[][], theirs: Box[][], where: string): void {
  expect(ours.length, `${where}: lines`).toBe(theirs.length)
  ours.forEach((line, i) => {
    line.forEach((box, j) => {
      const other = theirs[i]?.[j]
      const name = `${where}: line ${i + 1}, ${['clock', 'subject', 'sentence'][j]}`
      expect(Math.abs(box.x - (other?.x ?? Number.NaN)), `${name} left`).toBeLessThanOrEqual(1)
      expect(
        Math.abs(box.width - (other?.width ?? Number.NaN)),
        `${name} width`
      ).toBeLessThanOrEqual(1)
    })
  })
}

/** The gap between the Show and Issue controls, on one row. */
async function filterGap(page: Page): Promise<number> {
  const label = (name: string): Locator =>
    page.locator('label', { has: page.getByRole('combobox', { name }) })
  const show = await label('Show').boundingBox()
  const issue = await label('Issue').boundingBox()
  return (issue?.x ?? 0) - ((show?.x ?? 0) + (show?.width ?? 0))
}

/** Each select's label, chosen option and option texts, in order. */
async function controls(page: Page): Promise<string[]> {
  const list = page.getByRole('region', { name: 'What the factory did' })
  return list.locator('label:has(select)').evaluateAll((labels) =>
    labels.map((label) => {
      const select = label.querySelector('select')
      const options = Array.from(select?.options ?? [], (o) => o.text.trim())
      const name = label.firstChild?.textContent?.trim() ?? ''
      return `${name} [${select?.selectedOptions[0]?.text.trim() ?? ''}]: ${options.join(' | ')}`
    })
  )
}

/** A box measured from the page title's top left, so the header above it does not count. */
async function fromTitle(page: Page, shown: Locator): Promise<Box> {
  const title = await page.getByRole('heading', { level: 1, name: 'Events' }).boundingBox()
  const box = await shown.boundingBox()
  expect(box, 'compared box shown').not.toBeNull()
  return {
    x: (box?.x ?? 0) - (title?.x ?? 0),
    y: (box?.y ?? 0) - (title?.y ?? 0),
    width: box?.width ?? 0,
    height: box?.height ?? 0,
  }
}

/** With no event line: the filter row and the empty text. */
async function emptyLayout(page: Page): Promise<Record<'filters' | 'empty', Box>> {
  const list = page.getByRole('region', { name: 'What the factory did' })
  const filters = list.locator('div:has(> label > select)')
  const empty = list.locator('p')
  await expect(empty).toHaveCount(1)
  return { filters: await fromTitle(page, filters), empty: await fromTitle(page, empty) }
}

function expectSameBox(ours: Box, theirs: Box, name: string): void {
  for (const side of ['x', 'y', 'width', 'height'] as const) {
    expect(
      Math.abs(ours[side] - theirs[side]),
      `${name} ${side}: ours ${ours[side]}, prototype ${theirs[side]}`
    ).toBeLessThanOrEqual(1)
  }
}

/**
 * Each event line's words, then the page's, compared with the prototype's after the overrides.
 * The product's side is awaited: its heading and filters show before the polled events arrive.
 */
async function expectSameWords(page: Page, reference: Page, used: Set<string>, where: string) {
  const normal = (t: string): string => t.replace(/\s+/g, ' ').trim()
  const theirs = (await reference.locator('li.ev').allInnerTexts()).map((text) =>
    overridden(normal(text), used)
  )
  await expect(page.locator('li[id^="event-"]'), `${where}: lines`).toHaveText(theirs, {
    useInnerText: true,
  })
  await expect(page.locator('main'), `${where}: the page`).toHaveText(
    overridden(await words(reference.locator('#app')), used),
    { useInnerText: true }
  )
}

test.describe('Events match the binding prototype in every golden state', () => {
  // One worker runs the group in order, so the check after it sees every override each test used.
  test.describe.configure({ mode: 'serial' })
  const used = new Set<string>()

  for (const situation of ['open', 'clear', 'off', 'new', 'unreadable', 'lost'] as const) {
    test(`${situation}: the words, the controls, the line boxes and the filter layout`, async ({
      page,
      context,
    }) => {
      const reference = await context.newPage()
      await prototype(reference, situation)
      await product(page, situation)

      await expectSameWords(page, reference, used, situation)
      expect(await controls(page), `${situation}: controls`).toEqual(await controls(reference))
      if (situation === 'new' || situation === 'unreadable') {
        const ours = await emptyLayout(page)
        const theirs = await emptyLayout(reference)
        expectSameBox(ours.filters, theirs.filters, `${situation}: filter row`)
        expectSameBox(ours.empty, theirs.empty, `${situation}: empty text`)
      }
      expectSameColumns(
        await lineBoxes(page.locator('li[id^="event-"]')),
        await lineBoxes(reference.locator('li.ev')),
        situation
      )
      expect(Math.abs((await filterGap(page)) - (await filterGap(reference)))).toBeLessThanOrEqual(
        1
      )
    })
  }

  test('open at 600px: the words, the line boxes, each subject above its sentence', async ({
    page,
    context,
  }) => {
    await page.setViewportSize({ width: 600, height: 800 })
    const reference = await context.newPage()
    await reference.setViewportSize({ width: 600, height: 800 })
    await prototype(reference, 'open')
    await product(page, 'open')

    await expectSameWords(page, reference, used, '600px')
    const ours = await lineBoxes(page.locator('li[id^="event-"]'))
    expectSameColumns(ours, await lineBoxes(reference.locator('li.ev')), '600px')
    ours.forEach(([, subject, sentence], i) => {
      expect(
        (subject?.y ?? 0) + (subject?.height ?? 0),
        `line ${i + 1}: subject above sentence`
      ).toBeLessThanOrEqual((sentence?.y ?? 0) + 0.5)
    })
  })

  test('15: the open Error notices row, its words and boxes', async ({ page, context }) => {
    const WHAT = 'A message in Slack when an error starts and when it clears.'
    const reference = await context.newPage()
    await reference.goto(`${PROTOTYPE}#/settings/connections`)
    // The prototype names the product's font without loading it; opened from its file, it would
    // measure its words in a fallback font.
    await reference.addStyleTag({
      content: `@font-face { font-family: "Hanken Grotesk Variable"; font-weight: 100 900; src: url("${HANKEN}") format("woff2"); }`,
    })
    await reference.evaluate(() => document.fonts.ready)
    const theirs = reference.locator('#row-notices')
    await theirs.getByRole('button', { name: 'Change' }).click()
    await serve(page)
    await page.goto('/settings/connections')
    await page.getByRole('button', { name: 'Change Error notices' }).click()
    const ours = page
      .getByRole('listitem')
      .filter({ has: page.getByRole('form', { name: 'Error notices' }) })

    const parts = (row: Locator): Record<string, Locator> => ({
      // The name with its what-line beneath it: the what-line's parent.
      label: row.getByText(WHAT).locator('..'),
      what: row.getByText(WHAT),
      legend: row.locator('legend'),
      on: row.locator('label', { has: row.page().getByRole('radio', { name: 'On (default)' }) }),
      off: row.locator('label', { has: row.page().getByRole('radio', { name: 'Off' }) }),
      save: row.getByRole('button', { name: 'Save' }),
      cancel: row.getByRole('button', { name: 'Cancel' }),
      applies: row.getByText('Applies at once.'),
    })
    const measure = async (row: Locator): Promise<Record<string, { text: string; box: Box }>> => {
      const origin = await row.boundingBox()
      const result: Record<string, { text: string; box: Box }> = {}
      for (const [name, shown] of Object.entries(parts(row))) {
        const box = await shown.boundingBox()
        expect(box, `15: ${name} shown`).not.toBeNull()
        result[name] = {
          text: await words(shown),
          box: {
            x: (box?.x ?? 0) - (origin?.x ?? 0),
            y: (box?.y ?? 0) - (origin?.y ?? 0),
            width: box?.width ?? 0,
            height: box?.height ?? 0,
          },
        }
      }
      return result
    }
    const built = await measure(ours)
    const approved = await measure(theirs)

    for (const [name, { text, box }] of Object.entries(approved)) {
      expect(built[name]?.text, `15: ${name} words`).toBe(text)
      expectSameBox(built[name]?.box ?? box, box, `15: ${name}`)
    }
  })

  test.afterAll(() => {
    // An override none of the compared prototype words contained is stale. A run filtered with
    // `-g` may leave out the test that uses it.
    for (const o of OVERRIDES) {
      expect(used, `override not found in the prototype: ${o.passage}`).toContain(o.passage)
    }
  })
})
