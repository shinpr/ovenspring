import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, fn, userEvent, within } from 'storybook/test'
import { settings } from '../../fixtures/settings.ts'
import { AgentsSettings } from './AgentsSettings.tsx'

const meta = {
  component: AgentsSettings,
  args: {
    agents: settings.config.agents,
    available: settings.mcp,
    onSave: fn(async () => ({})),
  },
  decorators: [
    (Story) => (
      <ul style={{ listStyle: 'none', padding: 0 }}>
        <Story />
      </ul>
    ),
  ],
} satisfies Meta<typeof AgentsSettings>

export default meta
type Story = StoryObj<typeof meta>

/** Choosing Bypass says what it gives up, and the save carries it. */
export const BypassChosen: Story = {
  args: { open: 'permissions' },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    await expect(canvas.queryByText(/agents can delete files/)).toBeNull()
    await userEvent.click(canvas.getByRole('radio', { name: /Bypass/ }))
    await expect(canvas.getByText(/agents can delete files/)).toBeVisible()
    await userEvent.click(canvas.getByRole('button', { name: 'Save' }))
    await expect(args.onSave).toHaveBeenCalledWith({
      ...settings.config.agents,
      permissions: 'bypass',
    })
  },
}

/** A server that reaches one of the factory's own services says what agents could do through it. */
export const McpReachingGitHub: Story = {
  args: { open: 'mcp' },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByRole('checkbox', { name: /context7/ })).toBeChecked()
    await expect(canvas.getByRole('checkbox', { name: /github/ })).toHaveAccessibleDescription(
      'Reaches GitHub: agents could push, merge or approve through it.'
    )
  },
}
