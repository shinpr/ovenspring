// @lane: fixture-e2e
// Area: setup's three steps over the production bundle, routed by the step the server reports.
//
// The token cookie is proven by the dashboard's integration tests, so each test starts on a
// signed-in page.
import { expect, test } from '@playwright/test'
import { serve } from './support.ts'

// AC: "While setup is not finished, the dashboard shall open on its first step not done: Connect
//     while there is no configuration, Run the product while `[app]` is not settled, Set the goal
//     while there is no goal" — docs/design/owner-interface-design.md § Acceptance Criteria >
//     Dashboard (PRD AC-002, AC-065, AC-066).
// Behavior: a first start opens setup from any address -> Connect with Codex chosen -> Run the
//     product keeps the started commands -> the goal starts the factory -> Now, with the goal band.
// @dependency: dashboard app (Shell routing, setup steps, API client), fixture API
test('a first start goes through Connect, Run the product and Set the goal to Now', async ({
  page,
}) => {
  const sent = await serve(page, { step: 'connect' })
  await page.goto('/settings')
  await expect(page).toHaveURL(/\/setup$/)
  await expect(page.getByRole('heading', { level: 2 })).toHaveText('Connect')

  await page.getByLabel('Bot token', { exact: true }).fill('xoxb-0001')
  await page.getByLabel('App-level token', { exact: true }).fill('xapp-0001')
  await page.getByRole('textbox', { name: 'Channel ID' }).fill('C0123456789')
  await page.getByRole('textbox', { name: 'Your Slack member ID' }).fill('U0123456789')
  await page.getByRole('radio', { name: 'Codex' }).check()
  await expect(page.locator('#setup-slack').getByText('Connected.')).toBeVisible()
  await page.getByRole('button', { name: 'Continue' }).click()

  await expect(page.getByRole('heading', { level: 2 })).toHaveText('Run the product')
  await expect(page.getByText(/Started on port 4101/)).toBeVisible()
  await page.getByRole('button', { name: 'Continue' }).click()

  await expect(page.getByRole('heading', { level: 2 })).toHaveText('Set the goal')
  await page.getByRole('textbox', { name: 'Goal' }).fill('Shoppers find the right product.')
  await page.getByRole('button', { name: 'Start' }).click()

  await expect(page).toHaveURL(/\/$/)
  await expect(page.getByRole('region', { name: 'Product goal' })).toContainText(
    'Shoppers find the right product.'
  )
  const saves = sent.filter((r) => r.method === 'PUT').map((r) => [r.path, r.body])
  expect(saves).toEqual([
    [
      '/setup',
      {
        cli: 'codex',
        tracker: { kind: 'github' },
        slack: {
          botToken: 'xoxb-0001',
          appToken: 'xapp-0001',
          channel: 'C0123456789',
          owner: 'U0123456789',
        },
        repo: { baseBranch: 'main', mergeBy: 'factory' },
        github: { token: null },
      },
    ],
    [
      '/setup/run',
      {
        runs: 'commands',
        setup: 'pnpm install --frozen-lockfile',
        start: 'pnpm dev --port $PORT',
        ready: 'http://localhost:$PORT/health',
      },
    ],
    ['/goal', { text: 'Shoppers find the right product.' }],
  ])
})
