// @lane: fixture-e2e
// Area: Now, the goal band and the way from Now into an issue and its running step.
//
// The token cookie is proven by the dashboard's integration tests, so each test starts on a
// signed-in page.
import { expect, type Page, test } from '@playwright/test'
import {
  addressing,
  addressingRun,
  addressingRuns,
  detailOf,
  inReview,
  notStartedRuns,
  reviewFailed,
  reviewing,
  reviewingRuns,
  runs,
} from '../src/fixtures/issue.ts'
import { addressingFeedback, GOAL } from '../src/fixtures/overview.ts'
import { serve } from './support.ts'

// AC: PRD AC-043: "The dashboard's first screen shows the product goal, states whether the factory
//     is working or paused ... whether anything waits on the owner, shows how far the current
//     sprint has come, and lists the sprint's issues." PRD AC-045 (part): the owner opens a run's
//     log. UI Spec § Visual Acceptance, golden states 1, 5 and 6.
// Behavior: the owner opens Now -> follows the issue in review -> its page shows the path with
//     Review now -> "follow" opens the running step with what the agent has said.
// @dependency: dashboard app (Shell, Now, Issue, RunPanel, API client), fixture API
test('golden states 1, 5 and 6: from Now to an issue and the run under way', async ({ page }) => {
  await serve(page)
  await page.goto('/')

  await expect(page.getByRole('region', { name: 'Product goal' })).toContainText(GOAL)
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Working')
  await expect(page.getByText('2 of 7 ended')).toBeVisible()
  await page.getByRole('link', { name: inReview.title }).click()

  await expect(page.getByRole('heading', { level: 1 })).toHaveText(inReview.title)
  await expect(page.getByRole('region', { name: 'Product goal' })).toContainText(GOAL)
  const path = page.getByRole('region', { name: 'Path' })
  await path.getByRole('button', { name: 'follow' }).click()
  const run = page.getByRole('dialog')
  await expect(run.getByRole('heading', { level: 2 })).toHaveText(
    'Executor is judging the 4 findings'
  )
  await expect(run.getByText(/checking whether the SQL is built/)).toBeVisible()
  await page.keyboard.press('Escape')
  await expect(run).toBeHidden()
})

// AC: PRD AC-056: "The product goal is shown in the same place on every screen, and the owner can
//     rewrite it from there." UI Spec G-01 and S-07.
// Behavior: the owner presses "Rewrite…" in the band -> the goal page says what changes -> a new
//     text is saved -> Now shows it in the band -> `PUT /api/v1/goal` carried it.
// @dependency: dashboard app (GoalBand, Goal page), fixture API
test('the goal is rewritten from the band on its own page', async ({ page }) => {
  const sent = await serve(page)
  await page.goto('/activity')

  await page.getByRole('link', { name: 'Rewrite…' }).click()
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Rewrite the product goal')
  await expect(page.getByText(/sprint under way keeps its purpose/)).toBeVisible()
  const field = page.getByRole('textbox', { name: 'New goal' })
  await field.fill('Shoppers return a product without contacting support.')
  await page.getByRole('button', { name: 'Save the new goal' }).click()

  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Working')
  await expect(page.getByRole('region', { name: 'Product goal' })).toContainText(
    'Shoppers return a product without contacting support.'
  )
  expect(sent.filter((r) => r.method === 'PUT')).toEqual([
    {
      method: 'PUT',
      path: '/goal',
      body: { text: 'Shoppers return a product without contacting support.' },
    },
  ])
})

// AC: UI Spec FactoryStatus "Lost contact": "The dashboard lost contact with the factory ... if the
//     factory stopped, run `ovenspring start`".
// Behavior: the overview does not answer -> Now says the dashboard cannot reach the factory and
//     where to start it.
// @dependency: dashboard app (Shell, Now), fixture API
test('a factory that does not answer is said to be out of contact', async ({ page }) => {
  await serve(page, { overview: null })
  await page.goto('/')

  await expect(page.getByRole('alert')).toContainText('ovenspring start')
})

// AC: UI Spec FactoryStatus "Lost contact": "Last contact <clock>. Everything below is as it was
//     then."
// Behavior: the factory answers, then stops answering -> Now keeps what it last showed and names
//     the time of that last answer, which does not move while contact stays lost.
// @dependency: dashboard app (Shell, Now, usePoll), fixture API
test('contact lost after an answer keeps the page and names the time of the last answer', async ({
  page,
}) => {
  await serve(page)
  await page.goto('/')
  await expect(page.getByText('Results page for attribute search')).toBeVisible()

  await page.route('**/api/v1/overview', (route) =>
    route.fulfill({ status: 500, json: { errors: { request: 'down' } } })
  )
  await expect(page.getByText('The dashboard lost contact with the factory')).toBeVisible({
    timeout: 10_000,
  })
  // The page's clock is fixed: the time named is the last answer's, in the browser's zone.
  await expect(page.getByText(/Last contact \d{1,2}:\d{2}\./)).toBeVisible()
  await expect(page.getByText('Results page for attribute search')).toBeVisible()
})

// AC: UI Spec FactoryStatus "Settings not applied": what the configuration stops, said once.
// Behavior: the configuration cannot be used once setup is done -> Now's status says so, and the
//     shell adds no second, contradicting banner.
// @dependency: dashboard app (Shell, Now), fixture API
test('a configuration the factory cannot use is said once, by the status', async ({ page }) => {
  await serve(page, { configError: 'roles.lead.skills: payments-domain is not in the repository' })
  await page.goto('/')

  await expect(page.getByRole('heading', { name: 'Settings not applied', level: 1 })).toBeVisible()
  await expect(page.getByText(/The configuration cannot be used, so no work starts/)).toBeHidden()
})

// The review-feedback cases serve the production bundle with only the API as a fixture. Wording
// rules for every review name are the vocabulary unit tests'; these keep one representative name.

declare global {
  interface Window {
    /** The URLs of the answers the page has parsed, each recorded after the parse's continuations. */
    parsed?: string[]
  }
}

/** The issue page's status line, by how it starts. */
const statusLine = (page: Page, starts: RegExp) => page.locator('p').filter({ hasText: starts })

/** A class from the `fail` rule of a CSS module, as the production bundle names it. */
const FAIL = /(^|\s)_fail_/

// AC: docs/design/review-feedback-progress-design.md § Acceptance Criteria > Addressing feedback:
//     the issue page shows "Lead is addressing design review feedback · for <duration>" with
//     "follow", without color-fail; the overview row shows "Addressing design review feedback" as
//     the follow button that opens the lead's run, without "▲ Problem". GH-38 AC 1.
// Behavior: open Now -> follow the row -> the run dialog opens the lead's exception run -> close
//     -> open the issue.
// @dependency: dashboard app (Shell, Now, Glance, Issue, RunPanel, API client), fixture API
test('an unsettled review reads as the lead addressing its feedback, and follows the lead', async ({
  page,
}) => {
  const sent = await serve(page, {
    overview: addressingFeedback,
    issue: addressing,
    launches: [...runs, ...addressingRuns],
    launch: detailOf(addressingRun),
  })
  await page.goto('/')

  await page.getByRole('button', { name: 'Addressing design review feedback' }).click()
  const run = page.getByRole('dialog')
  await expect(run.getByRole('heading', { level: 2 })).toHaveText('Lead is looking into a problem')
  expect(sent.map((r) => r.path)).toContain(`/launches/${addressingRun.id}`)
  await expect(page.getByText(/Problem:/)).toHaveCount(0)
  await page.keyboard.press('Escape')
  await expect(run).toBeHidden()

  await page.getByRole('link', { name: addressing.title }).click()
  const line = statusLine(page, /^Lead is addressing design review feedback · for /)
  await expect(line).toBeVisible()
  await expect(line.getByRole('button', { name: 'follow' })).toBeVisible()
  await expect(line).not.toHaveClass(FAIL)
  await expect(line.locator('[class*="_fail_"]')).toHaveCount(0)
})

// AC: § Acceptance Criteria > Addressing feedback: with no running exception launch, following the
//     overview row opens no run, and the issue page shows "Addressing design review feedback · for
//     <duration>" without naming the reviewer and without "follow". The owner: "Until it starts,
//     clicking Follow should open nothing. Combine the two pending-run browser cases."
// Behavior: the launches hold, newest first, a running design review, an ended exception run and
//     the finished design review -> follow the row -> wait until its launches are parsed and the
//     follow's continuation has run -> open ACME-141's run, a later request -> open the issue.
// Observable result: no request for any of the three runs' details; the issue page names no one.
// @dependency: dashboard app (Shell, Now, Glance, Issue, RunPanel, API client), fixture API
test("before the lead's run starts, an unsettled review follows no run and names no one", async ({
  page,
}) => {
  const sent = await serve(page, {
    overview: addressingFeedback,
    issue: addressing,
    launches: [...runs, ...notStartedRuns],
  })
  // No network event marks when the follow has acted on its answer, which is parsed in the page:
  // record each parsed answer's URL from a timer, which runs after the parse's continuations.
  await page.addInitScript(() => {
    const parsed: string[] = []
    window.parsed = parsed
    const json = Response.prototype.json
    Response.prototype.json = async function (this: Response): Promise<unknown> {
      const body: unknown = await json.call(this)
      setTimeout(() => parsed.push(this.url), 0)
      return body
    }
  })
  await page.goto('/')

  await page.getByRole('button', { name: 'Addressing design review feedback' }).click()
  await page.waitForFunction(() =>
    window.parsed?.some((url) => url.endsWith('/launches?issue=ACME-148'))
  )
  await page.getByRole('button', { name: 'Judging findings' }).click()
  const run = page.getByRole('dialog')
  await expect(run.getByRole('heading', { level: 2 })).toHaveText(
    'Executor is judging the 4 findings'
  )
  const opened = sent.map((r) => r.path)
  for (const row of notStartedRuns) {
    expect(opened).not.toContain(`/launches/${row.id}`)
  }
  await page.keyboard.press('Escape')
  await expect(run).toBeHidden()

  await page.getByRole('link', { name: addressing.title }).click()
  const line = statusLine(page, /^Addressing design review feedback · for /)
  await expect(line).toBeVisible()
  await expect(line).not.toContainText('Reviewer')
  await expect(line.getByRole('button', { name: 'follow' })).toHaveCount(0)
})

// AC: § Acceptance Criteria > Next review: the issue then shows the review's activity. GH-38 AC 2.
// Behavior: the lead retried and the next design review runs -> open the issue -> follow.
// Observable result: the line names the reviewer reviewing the design, not as a failure, and
//     "follow" opens the design review, not the lead's ended run.
// @dependency: dashboard app (Issue, RunPanel, API client), fixture API
test('once the next review starts, the issue page names the review under way', async ({ page }) => {
  const next = reviewingRuns[0] ?? addressingRun
  await serve(page, {
    issue: reviewing,
    launches: reviewingRuns,
    launch: detailOf(next),
  })
  await page.goto(`/issues/${reviewing.key}`)

  const line = statusLine(page, /^Reviewer is reviewing the design/)
  await expect(line).toBeVisible()
  await expect(line).not.toHaveClass(FAIL)
  await line.getByRole('button', { name: 'follow' }).click()
  await expect(page.getByRole('dialog').getByRole('heading', { level: 2 })).toHaveText(
    'Reviewer is reviewing the design'
  )
})

// AC: § Acceptance Criteria > Failures and owner input: a review that could not run shows "The
//     lead is handling a problem: Design review stopped" in color-fail. GH-38 AC 3.
// Behavior: open the issue whose design review could not run.
// @dependency: dashboard app (Issue, API client), fixture API
test('a review that could not run still reads as a problem on the issue page', async ({ page }) => {
  await serve(page, { issue: reviewFailed, launches: [] })
  await page.goto(`/issues/${reviewFailed.key}`)

  await expect(page.getByText('The lead is handling a problem: Design review stopped')).toHaveClass(
    FAIL
  )
})
