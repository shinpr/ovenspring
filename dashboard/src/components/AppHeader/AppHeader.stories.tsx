import type { Meta, StoryObj } from '@storybook/react-vite'
import { userEvent, within } from 'storybook/test'
import { AppHeader } from './AppHeader.tsx'

const meta = {
  component: AppHeader,
  args: { repository: 'storefront', current: 'now' },
} satisfies Meta<typeof AppHeader>

export default meta
type Story = StoryObj<typeof meta>

/** The running version, with nothing newer to move to. */
export const Current: Story = {
  args: { version: { running: '0.1.0', update: null } },
}

/** A newer release, with the steps to install it opened. */
export const Available: Story = {
  args: { version: { running: '0.1.0', update: { kind: 'available', version: '0.2.0' } } },
  play: async ({ canvasElement }) => {
    await userEvent.click(within(canvasElement).getByRole('button', { name: 'v0.2.0 is out' }))
  },
}

/** A newer version installed and waiting for a restart, with the steps opened. */
export const Restart: Story = {
  args: { version: { running: '0.1.0', update: { kind: 'restart', version: '0.2.0' } } },
  play: async ({ canvasElement }) => {
    await userEvent.click(
      within(canvasElement).getByRole('button', { name: 'Restart to run v0.2.0' })
    )
  },
}

/** Before setup is saved: only the wordmark and the version. */
export const DuringSetup: Story = {
  args: { navigation: false, version: { running: '0.1.0', update: null } },
}
