// @lane: fixture-e2e
// Area: Now, the goal band and the way from Now into an issue and its running step.
//
// The token cookie is proven by the dashboard's integration tests, so each test starts on a
// signed-in page.
import { expect, test } from '@playwright/test'
import { inReview } from '../src/fixtures/issue.ts'
import { GOAL } from '../src/fixtures/overview.ts'
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
