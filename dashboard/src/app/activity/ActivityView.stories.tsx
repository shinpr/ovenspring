import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, fn, userEvent, within } from 'storybook/test'
import { runs } from '../../fixtures/issue.ts'
import { NOW } from '../../fixtures/overview.ts'
import { ActivityView } from './ActivityView.tsx'

const meta = {
  component: ActivityView,
  args: {
    runs,
    sprints: [
      { value: '20261121-0905', label: 'This sprint' },
      { value: 'all', label: 'All sprints' },
    ],
    issues: [{ value: 'ACME-141', label: 'ACME-141 Search products by their attributes' }],
    sprint: '20261121-0905',
    issue: '',
    onFilter: fn(),
    onOpenRun: fn(),
    now: NOW,
  },
} satisfies Meta<typeof ActivityView>

export default meta
type Story = StoryObj<typeof meta>

/** What runs is under way, for how long; the rest is newest first under its hour, with counts. */
export const ThisSprint: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    const under = canvas.getByRole('region', { name: 'Under way' })
    await expect(under).toHaveTextContent('Lead is judging the 4 findings · for 3 min')
    await expect(canvas.getByText(/reviewed the code/)).toHaveTextContent('→ 3 findings')
    await expect(canvas.getByText(/→ failed/)).toBeVisible()
    await userEvent.selectOptions(canvas.getByRole('combobox', { name: 'Issue' }), 'ACME-141')
    await expect(args.onFilter).toHaveBeenCalledWith({
      sprint: '20261121-0905',
      issue: 'ACME-141',
    })
  },
}

/** Not answered yet, or not readable: no sentence says that nothing ran. */
export const NotRead: Story = {
  args: { runs: undefined },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByRole('heading', { name: 'Under way' })).toBeVisible()
    await expect(canvas.queryByText('Nothing is running for this filter.')).toBeNull()
    await expect(canvas.queryByText('Nothing has happened yet.')).toBeNull()
  },
}

export const Nothing: Story = {
  args: { runs: [] },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByText('Nothing is running for this filter.')).toBeVisible()
    await expect(canvas.getByText('Nothing has happened yet.')).toBeVisible()
  },
}
