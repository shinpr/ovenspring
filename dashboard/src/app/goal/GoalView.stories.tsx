import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, fn, userEvent, within } from 'storybook/test'
import { GOAL } from '../../fixtures/overview.ts'
import { GoalView } from './GoalView.tsx'

const meta = {
  component: GoalView,
  args: {
    goal: GOAL,
    onSave: fn(),
    onCancel: fn(),
  },
} satisfies Meta<typeof GoalView>

export default meta
type Story = StoryObj<typeof meta>

/** Save waits for a changed, non-empty text, and says why it waits. */
export const Rewrite: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    const save = canvas.getByRole('button', { name: 'Save the new goal' })
    await expect(save).toBeDisabled()
    await expect(canvas.getByText('Change the text to save.')).toBeVisible()
    const field = canvas.getByRole('textbox', { name: 'New goal' })
    await userEvent.clear(field)
    await expect(canvas.getByText('A goal cannot be empty.')).toBeVisible()
    await userEvent.type(field, 'Shoppers return a product without contacting support.')
    await userEvent.click(save)
    await expect(args.onSave).toHaveBeenCalledWith(
      'Shoppers return a product without contacting support.'
    )
  },
}

export const Refused: Story = {
  args: { onSave: fn(async (_text: string) => 'a goal cannot be empty') },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.type(canvas.getByRole('textbox', { name: 'New goal' }), ' more')
    await userEvent.click(canvas.getByRole('button', { name: 'Save the new goal' }))
    await expect(await canvas.findByText('a goal cannot be empty')).toBeVisible()
  },
}
