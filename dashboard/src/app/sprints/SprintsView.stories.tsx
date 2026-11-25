import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, fn, userEvent, within } from 'storybook/test'
import { ended, runs } from '../../fixtures/issue.ts'
import { working } from '../../fixtures/overview.ts'
import { SprintsView } from './SprintsView.tsx'

const meta = {
  component: SprintsView,
  args: {
    current: working.sprint,
    runs,
    ended,
    onDownload: fn(async () => undefined),
  },
} satisfies Meta<typeof SprintsView>

export default meta
type Story = StoryObj<typeof meta>

/** The running sprint: when it started, its cost by role; an ended sprint opens to its results. */
export const WithEndedSprint: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByText(/^Started Nov 21, \d{2}:05 · 2 of 7 issues ended$/)).toBeVisible()
    await expect(canvas.getByText(/^2 runs on Codex/)).toBeVisible()
    await expect(canvas.getAllByText('no known cost').length).toBeGreaterThan(0)
    await userEvent.click(canvas.getByText(/1 of 2 key results met/))
    await expect(canvas.getByText('Not met')).toBeVisible()
    await expect(canvas.getByText('1 held')).toBeVisible()
  },
}

export const NoSprint: Story = {
  args: { current: null, runs: [], ended: [] },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.queryByRole('heading', { name: 'This sprint' })).toBeNull()
    await expect(canvas.getByText('No sprint has ended yet.')).toBeVisible()
  },
}
