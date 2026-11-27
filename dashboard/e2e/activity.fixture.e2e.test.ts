// @lane: fixture-e2e
// Area: Activity, reached from an issue, and its filter.
//
// The token cookie is proven by the dashboard's integration tests, so each test starts on a
// signed-in page.
import { expect, test } from '@playwright/test'
import type { LaunchRow } from '../src/api/types.gen.ts'
import { inReview, runs } from '../src/fixtures/issue.ts'
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

// AC: GH-10 Done when: "An issue whose ... steps each ran a preparation run and a work run shows
//     one line for each of those steps on its page" and "Activity shows both runs of each step,
//     worded differently, and the preparation run that asked a question says so."
// Behavior: a design step that ran a preparation turn that asked the owner, then its work -> the
//     issue page lists the step once -> Activity lists both, the preparation saying it asked.
// @dependency: dashboard app (Issue, Activity, API client), fixture API
test('a step that prepared reads once on its issue and twice in Activity', async ({ page }) => {
  const [base] = runs
  if (base === undefined) {
    throw new Error('the fixture has no runs')
  }
  const analysis = (id: string, change: Partial<LaunchRow>): LaunchRow => ({
    ...base,
    id,
    endedAt: '2026-11-21T10:30:00Z',
    role: 'lead',
    step: 'codebase-analysis',
    phase: 'design',
    cli: 'claude',
    result: 'done',
    ...change,
  })
  await serve(page, {
    launches: [
      ...runs,
      analysis('20261121-101000-codebase-analysis-ACME-141.1', {}),
      analysis('20261121-100000-codebase-analysis-ACME-141.1', {
        endedAt: '2026-11-21T10:05:00Z',
        preparation: 'asked',
      }),
    ],
  })
  await page.goto(`/issues/${inReview.key}`)
  const design = page
    .getByRole('region', { name: 'Path' })
    .getByRole('listitem')
    .filter({ has: page.getByRole('heading', { name: 'Design' }) })

  await design.getByText('1 run').click()

  await expect(design.getByText(/read the code/)).toHaveCount(1)
  await expect(design.getByText(/asked you a question/)).toHaveCount(0)

  await page.getByRole('link', { name: 'See them in Activity' }).click()

  const earlier = page.getByRole('region', { name: 'Earlier' })
  await expect(earlier.getByText(/read the code/)).toHaveCount(1)
  await expect(earlier.getByText('asked you a question before reading the code')).toHaveCount(1)
})
