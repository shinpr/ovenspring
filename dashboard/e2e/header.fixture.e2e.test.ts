// @lane: fixture-e2e
// Area: the header's version, and the way to a newer one.
import { expect, test } from '@playwright/test'
import { serve } from './support.ts'

// AC: PRD AC-071: "The dashboard shows which version of Oven Spring the repository runs. When a
//     newer release is out, or a newer executable is installed and waits for a restart, it says
//     so and how to move to it." UI Spec AppHeader, Version and update.
// Behavior: the owner opens any screen -> the header shows the running version and that a newer
//     release is out -> opening it shows the install command and the restart.
// @dependency: dashboard app (Shell, AppHeader, API client), fixture API
test('a newer release is offered in the header with how to install it', async ({ page }) => {
  await serve(page, {
    version: { running: '0.1.0', update: { kind: 'available', version: '0.2.0' } },
  })
  await page.goto('/activity')

  const header = page.getByRole('banner')
  await expect(header.getByText('v0.1.0', { exact: true })).toBeVisible()
  const offer = header.getByRole('button', { name: 'v0.2.0 is out' })
  await offer.click()

  await expect(offer).toHaveAttribute('aria-expanded', 'true')
  await expect(header.getByText('Oven Spring 0.2.0 is out.')).toBeVisible()
  await expect(header.getByRole('link', { name: /What's new/ })).toHaveAttribute(
    'href',
    'https://github.com/shinpr/ovenspring/releases/tag/v0.2.0'
  )
  await expect(header.locator('code')).toHaveText(
    'curl -fsSL https://github.com/shinpr/ovenspring/releases/latest/download/install.sh | sh'
  )
  await expect(header.getByText(/in the folder of each repository/)).toBeVisible()
})

// AC: PRD AC-071 (restart part), as above.
// Behavior: a newer executable is installed but this repository's factory still runs the old
//     one -> the header asks for a restart -> opening it says how, in this repository's folder.
// @dependency: dashboard app (Shell, AppHeader, API client), fixture API
test('an installed version that is not running yet asks for a restart', async ({ page }) => {
  await serve(page, {
    version: { running: '0.1.0', update: { kind: 'restart', version: '0.2.0' } },
  })
  await page.goto('/')

  const header = page.getByRole('banner')
  await header.getByRole('button', { name: 'Restart to run v0.2.0' }).click()

  await expect(
    header.getByText('Oven Spring 0.2.0 is installed, but this repository still runs 0.1.0.')
  ).toBeVisible()
  await expect(header.getByText(/in this repository's folder/)).toBeVisible()
  await expect(header.locator('code')).toHaveCount(0)
})

// AC: Design § Fact Disposition Table, AppHeader LINKS: "`Shell.tsx` `area()` maps `/events` to
//     'events', which is what makes the Events link current". UI Spec AppHeader navigation
//     "Now · Activity · Events · Sprints · Settings".
// Behavior: the owner opens Events -> the header marks Events as the current page, and only it.
// @dependency: dashboard app (Shell area(), AppHeader), fixture API
test('on Events, the Events link is the current page', async ({ page }) => {
  await serve(page)
  await page.goto('/events')

  const nav = page.getByRole('navigation', { name: 'Main' })
  await expect(nav.getByRole('link')).toHaveText([
    'Now',
    'Activity',
    'Events',
    'Sprints',
    'Settings',
  ])
  await expect(nav.getByRole('link', { name: 'Events' })).toHaveAttribute('aria-current', 'page')
  await expect(nav.locator('[aria-current]')).toHaveCount(1)
})
