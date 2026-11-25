import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, fn, userEvent, waitFor, within } from 'storybook/test'
import { running } from '../../fixtures/issue.ts'
import { NOW } from '../../fixtures/overview.ts'
import { RunPanel } from './RunPanel.tsx'

const meta = {
  component: RunPanel,
  args: { run: running, now: NOW, logUrl: '/api/v1/launches/x/log', onClose: fn() },
} satisfies Meta<typeof RunPanel>

export default meta
type Story = StoryObj<typeof meta>

/** Golden state 6: a running run shows everything the agent has said so far. */
export const Running: Story = {
  play: async ({ canvasElement, args }) => {
    const dialog = within(canvasElement.ownerDocument.body).getByRole('dialog')
    const panel = within(dialog)
    await expect(panel.getByRole('heading', { level: 2 })).toHaveTextContent(
      'Lead is judging the 4 findings'
    )
    await expect(panel.getByText('3 min so far')).toBeVisible()
    await expect(panel.getByText('Known when it ends')).toBeVisible()
    await expect(panel.getAllByRole('listitem')).toHaveLength(2)
    await expect(panel.getByRole('link', { name: /Open the full log/ })).toHaveAttribute(
      'target',
      '_blank'
    )
    await userEvent.click(panel.getByRole('button', { name: 'Close' }))
    // The dialog's close event is dispatched a task after it closes.
    await waitFor(() => expect(args.onClose).toHaveBeenCalled())
  },
}

/** An ended Codex run: its last message, and a cost that is not known. */
export const EndedOnCodex: Story = {
  args: {
    run: {
      ...running,
      cli: 'codex',
      endedAt: '2026-11-21T14:19:00Z',
      result: 'failed',
      failure: 'fatal: the model is overloaded',
    },
  },
  play: async ({ canvasElement }) => {
    const panel = within(within(canvasElement.ownerDocument.body).getByRole('dialog'))
    await expect(panel.getByText('Not known')).toBeVisible()
    await expect(panel.getByText('fatal: the model is overloaded')).toBeVisible()
    await expect(panel.getByText(/checking whether the SQL/)).toBeVisible()
  },
}

export const CouldNotBeRead: Story = {
  args: { run: null },
  play: async ({ canvasElement }) => {
    const panel = within(within(canvasElement.ownerDocument.body).getByRole('dialog'))
    await expect(panel.getByText('This run could not be read.')).toBeVisible()
  },
}

/** An agent writes Markdown: it reads as formatted text, and any HTML in it stays text. */
export const SaidInMarkdown: Story = {
  args: {
    run: {
      ...running,
      endedAt: '2026-11-21T14:19:00Z',
      result: 'done',
      messages: [
        'I accepted your PR comment.\n\n**Why it goes ahead:** it is your own request.\n\n- The decimal places are fixed.\n- A share above zero never reads `0`.\n\n<img src=x onerror=alert(1)>',
      ],
    },
  },
  play: async ({ canvasElement }) => {
    const panel = within(within(canvasElement.ownerDocument.body).getByRole('dialog'))
    await expect(panel.getByText('Why it goes ahead:', { selector: 'strong' })).toBeVisible()
    await expect(panel.getAllByRole('listitem').length).toBeGreaterThanOrEqual(2)
    await expect(panel.getByText('0', { selector: 'code' })).toBeVisible()
    await expect(panel.queryByRole('img')).toBeNull()
    await expect(panel.queryByText(/\*\*Why/)).toBeNull()
  },
}
