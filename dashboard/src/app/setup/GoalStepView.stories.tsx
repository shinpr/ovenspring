import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, fn, userEvent, within } from 'storybook/test'
import { GoalStepView } from './GoalStepView.tsx'
import { SetupFrame } from './SetupFrame.tsx'

const meta = {
  component: GoalStepView,
  decorators: [
    (Story) => (
      <SetupFrame repository="storefront" step={3}>
        <Story />
      </SetupFrame>
    ),
  ],
  args: { onStart: fn(async () => undefined) },
} satisfies Meta<typeof GoalStepView>

export default meta
type Story = StoryObj<typeof meta>

/** Start needs a goal; with one, it starts the factory. */
export const SetTheGoal: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    await userEvent.click(canvas.getByRole('button', { name: 'Start' }))
    await expect(canvas.getByRole('textbox', { name: 'Goal' })).toHaveAccessibleDescription(
      /Enter the goal/
    )
    await expect(args.onStart).not.toHaveBeenCalled()
    await userEvent.type(
      canvas.getByRole('textbox', { name: 'Goal' }),
      'Shoppers find the right product.'
    )
    await userEvent.click(canvas.getByRole('button', { name: 'Start' }))
    await expect(args.onStart).toHaveBeenCalledWith('Shoppers find the right product.')
  },
}
