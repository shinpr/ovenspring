import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, fn, userEvent, within } from 'storybook/test'
import { CancelPanel } from './CancelPanel.tsx'

const meta = {
  component: CancelPanel,
  args: { tracker: 'Linear', onCancel: fn(async () => undefined) },
} satisfies Meta<typeof CancelPanel>

export default meta
type Story = StoryObj<typeof meta>

/** The owner says why and cancels; the reason goes with the request, and the button takes no second press. */
export const Canceled: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByText(/marks it canceled on Linear/)).toBeVisible()
    await userEvent.type(canvas.getByLabelText(/Why/), 'Billing takes this over.')
    const cancel = canvas.getByRole('button', { name: 'Cancel the issue' })
    await userEvent.click(cancel)
    await expect(args.onCancel).toHaveBeenCalledWith('Billing takes this over.')
    await expect(canvas.getByRole('status')).toHaveTextContent(
      'Canceled. The work stops within seconds.'
    )
    await expect(cancel).toBeDisabled()
  },
}

/** A refused or lost request says so and can be tried again. */
export const Failed: Story = {
  args: { onCancel: fn(async () => Promise.reject(new Error('409'))) },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.click(canvas.getByRole('button', { name: 'Cancel the issue' }))
    await expect(canvas.getByRole('alert')).toHaveTextContent('Canceling failed. Try again.')
    await expect(canvas.getByRole('button', { name: 'Cancel the issue' })).toBeEnabled()
  },
}
