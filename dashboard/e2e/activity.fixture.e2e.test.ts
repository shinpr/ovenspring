// @lane: fixture-e2e
// Area: Activity, reached from an issue, and its filter.
//
// The token cookie is proven by the dashboard's integration tests, so each test starts on a
// signed-in page.
import { expect, test } from '@playwright/test'
import { inReview } from '../src/fixtures/issue.ts'
import { working } from '../src/fixtures/overview.ts'
import { serve } from './support.ts'

// AC: PRD AC-045: "The owner can list the factory's launches filtered by sprint and issue, and open
//     each one's model, duration, cost and log." UI Spec S-02 "See them in Activity" and S-04.
// Behavior: from the issue page the owner follows "See them in Activity" -> Activity lists that
//     issue's runs of this sprint, a failed one in its colour -> the app asked for them by sprint
//     and issue.
// @dependency: dashboard app (Issue, Activity, API client), fixture API
test("an issue's runs are listed in Activity, failures included", async ({ page }) => {
  const sent = await serve(page)
  await page.goto(`/issues/${inReview.key}`)

  await page.getByRole('link', { name: 'See them in Activity' }).click()

  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Activity')
  await expect(page.getByRole('combobox', { name: 'Issue' })).toHaveValue(inReview.key)
  await expect(page.getByText('→ failed')).toBeVisible()
  await expect(page.getByRole('region', { name: 'Under way' })).toContainText(
    'Executor is judging the 4 findings'
  )
  expect(sent.map((r) => r.path)).toContain(
    `/launches?sprint=${working.sprint?.id ?? ''}&issue=${inReview.key}`
  )
})
