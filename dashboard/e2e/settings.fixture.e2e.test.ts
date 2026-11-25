// @lane: fixture-e2e
// Area: Settings golden state for the role editor, and a refused connection.
//
// The token cookie is proven by the dashboard's integration tests, so each test starts on a
// signed-in page.
import { expect, test } from '@playwright/test'
import { serve } from './support.ts'

// AC: "When the owner saves a role's settings, the system shall write `.ovenspring/config.toml` and
//     the next launch of that role shall use them (AC-007 to AC-009)" —
//     docs/design/owner-interface-design.md § Acceptance Criteria > Dashboard. UI Spec RoleEditor:
//     Codex's effort is chosen from its levels; a change of agent clears the model and effort.
//     UI Spec § Visual Acceptance, golden state 7.
// Behavior: the owner opens Settings, Agents -> changes the lead to Codex with effort xhigh -> saves
//     -> the app sends `PUT /api/v1/settings/roles` with the lead's new agent.
// @dependency: dashboard app (Settings, RoleEditor, API client), fixture API
test('golden state 7: the lead moved to Codex', async ({ page }) => {
  const sent = await serve(page)
  await page.goto('/settings')
  await page.getByRole('link', { name: 'Agents' }).click()

  await page.getByRole('button', { name: 'Change Lead' }).click()
  const lead = page.getByRole('form', { name: 'Lead' })
  await lead.getByRole('radio', { name: 'Codex' }).check()
  await expect(lead.getByText(/do not apply to Codex, so they are cleared/)).toBeVisible()
  await lead.getByRole('combobox', { name: 'Effort' }).selectOption('xhigh')
  await lead.getByRole('button', { name: 'Save' }).click()
  await expect(lead).toBeHidden()

  expect(sent.filter((r) => r.method === 'PUT')).toEqual([
    {
      method: 'PUT',
      path: '/settings/roles',
      body: {
        role: 'lead',
        cli: 'codex',
        model: null,
        effort: 'xhigh',
        skills: [],
        append: '',
      },
    },
  ])
})

// AC: PRD AC-010 to AC-012: the owner changes the tracker and Slack connections in Settings; UI
//     Spec SettingRow: "A refused save shows the reason under the field."
// Behavior: the owner changes the tracker's team -> the server refuses the connection -> the form
//     stays open with the reason.
// @dependency: dashboard app (Settings, SettingRow, API client), fixture API
test('a connection the tracker refuses keeps the form open with the reason', async ({ page }) => {
  await serve(page, { refuse: { tracker: { tracker: 'Linear has no team ACMX.' } } })
  await page.goto('/settings/connections')

  await page.getByRole('button', { name: 'Change Issue tracker' }).click()
  const form = page.getByRole('form', { name: 'Issue tracker' })
  await form.getByRole('textbox', { name: 'Team key' }).fill('ACMX')
  await form.getByRole('button', { name: 'Save' }).click()

  await expect(form.getByRole('alert')).toHaveText('Linear has no team ACMX.')
})

// AC: UI Spec SettingRow: "leaving with an unsaved change discards it and says so"; owner-interface
//     design Client State: drafts are "discarded on Cancel or on leaving, with a notice".
// Behavior: the owner opens a row and leaves the section without typing -> no notice; opens it,
//     types, and leaves -> the notice names the row.
// @dependency: dashboard app (Settings, SettingRow, router), fixture API
test('leaving a section says a change was discarded only when one was typed', async ({ page }) => {
  await serve(page)
  await page.goto('/settings/merging')

  await page.getByRole('button', { name: 'Change Base branch' }).click()
  await page.getByRole('link', { name: 'Agents' }).click()
  await expect(page.getByRole('heading', { name: 'Agents', level: 2 })).toBeVisible()
  await expect(page.getByText(/was not saved/)).toBeHidden()

  await page.getByRole('link', { name: 'Merging' }).click()
  await page.getByRole('button', { name: 'Change Base branch' }).click()
  await page.getByRole('textbox', { name: 'Branch' }).fill('trunk')
  await page.getByRole('link', { name: 'Agents' }).click()
  await expect(page.getByRole('status')).toHaveText(
    'Your unsaved change to Base branch was not saved.'
  )
})
