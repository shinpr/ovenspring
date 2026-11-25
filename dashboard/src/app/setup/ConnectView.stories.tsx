import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, fn, userEvent, waitFor, within } from 'storybook/test'
import { ApiError } from '../../api/client.ts'
import type { SetupCheck } from '../../api/types.gen.ts'
import { ConnectView } from './ConnectView.tsx'
import { SetupFrame } from './SetupFrame.tsx'

/** Checks that pass, as the server answers for each block. */
const passing = (block: SetupCheck): string =>
  ({
    github: 'Using the token of your signed-in gh, as kai.',
    tracker: 'This repository, 9 open issues.',
    slack: 'Connected to #acme-factory as Oven Spring.',
  })[block.section]

const meta = {
  component: ConnectView,
  decorators: [
    (Story) => (
      <SetupFrame repository="storefront" step={1}>
        <Story />
      </SetupFrame>
    ),
  ],
  args: {
    manifest: '{\n  "display_information": { "name": "Oven Spring" }\n}',
    origin: 'acme/storefront',
    onCheck: fn(async (block: SetupCheck) => passing(block)),
    onSave: fn(async () => undefined),
  },
} satisfies Meta<typeof ConnectView>

export default meta
type Story = StoryObj<typeof meta>

type Canvas = ReturnType<typeof within>

async function fillSlack(canvas: Canvas): Promise<void> {
  await userEvent.type(canvas.getByLabelText('Bot token'), 'xoxb-0001')
  await userEvent.type(canvas.getByLabelText('App-level token'), 'xapp-0001')
  await userEvent.type(canvas.getByRole('textbox', { name: 'Channel ID' }), 'C0123456789')
  await userEvent.type(canvas.getByRole('textbox', { name: 'Your Slack member ID' }), 'U0123456789')
}

/** Golden state 8 (Connect part): GitHub and GitHub Issues pass on their own; Continue lists the rest. */
export const FirstStart: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByText(/^This folder's/)).toHaveTextContent(
      "This folder's origin: github.com/acme/storefront"
    )
    await expect(await canvas.findByText(/as kai/, {}, { timeout: 3000 })).toBeVisible()
    await expect(canvas.getByText('This repository, 9 open issues.')).toBeVisible()
    await userEvent.click(canvas.getByRole('button', { name: 'Continue' }))
    await expect(canvas.getByRole('alert')).toHaveFocus()
    const missing = within(canvas.getByRole('alert'))
    await expect(missing.getByText('Slack: Enter the bot token.')).toBeVisible()
    await expect(missing.getByText('Agents: choose the agent.')).toBeVisible()
    await userEvent.type(canvas.getByLabelText('Bot token'), 'xoxb-0001')
    await expect(missing.queryByText('Slack: Enter the bot token.')).toBeNull()
    await expect(args.onSave).not.toHaveBeenCalled()
  },
}

/** A filled form is checked block by block and saved with the agent chosen; no agent is chosen beforehand. */
export const Filled: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    for (const radio of canvas.getAllByRole('radio', { name: /Claude Code|Codex|OpenCode/ })) {
      await expect(radio).not.toBeChecked()
    }
    await fillSlack(canvas)
    await userEvent.tab()
    await expect(await canvas.findByText(/Connected to #acme-factory/)).toBeVisible()
    await userEvent.click(canvas.getByRole('radio', { name: 'Codex' }))
    await userEvent.click(canvas.getByRole('radio', { name: /^You/ }))
    await waitFor(() => expect(canvas.getByText(/as kai/)).toBeVisible())
    await userEvent.click(canvas.getByRole('button', { name: 'Continue' }))
    await expect(args.onSave).toHaveBeenCalledWith({
      cli: 'codex',
      tracker: { kind: 'github' },
      slack: {
        botToken: 'xoxb-0001',
        appToken: 'xapp-0001',
        channel: 'C0123456789',
        owner: 'U0123456789',
      },
      repo: { baseBranch: 'main', mergeBy: 'owner' },
      github: { token: null },
    })
  },
}

/** A refused check shows what the service answered under its block, with Check again. */
export const SlackRefused: Story = {
  args: {
    onCheck: fn(async (block: SetupCheck) => {
      if (block.section === 'slack') {
        throw new ApiError(400, { slack: 'Slack answered not_in_channel for C0123456789.' })
      }
      return passing(block)
    }),
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await fillSlack(canvas)
    await userEvent.tab()
    await expect(
      await canvas.findByText('Slack answered not_in_channel for C0123456789.')
    ).toBeVisible()
    await expect(canvas.getByRole('button', { name: 'Check again' })).toBeVisible()
  },
}

/** Tokens are hidden while typed, and shown on request to check what was pasted. */
export const ShowToken: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    const token = canvas.getByLabelText('Bot token')
    await userEvent.type(token, 'xoxb-0001')
    await expect(token).toHaveAttribute('type', 'password')
    await userEvent.click(canvas.getByRole('button', { name: 'Show Bot token' }))
    await expect(token).toHaveAttribute('type', 'text')
    await expect(canvas.getByRole('button', { name: 'Hide Bot token' })).toHaveAttribute(
      'aria-pressed',
      'true'
    )
  },
}

/** Continue pressed while a check still answers waits for it, then saves once every block passed. */
export const ContinueWaitsForChecks: Story = {
  args: {
    onCheck: fn(
      (block: SetupCheck) =>
        new Promise<string>((resolve) => setTimeout(() => resolve(passing(block)), 400))
    ),
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    await expect(await canvas.findByText(/as kai/, {}, { timeout: 3000 })).toBeVisible()
    await userEvent.click(canvas.getByRole('radio', { name: 'OpenCode' }))
    await fillSlack(canvas)
    await userEvent.click(canvas.getByRole('button', { name: 'Continue' }))
    await expect(canvas.getByRole('button', { name: 'Checking…' })).toBeDisabled()
    await expect(canvas.queryByRole('alert')).toBeNull()
    await waitFor(() =>
      expect(args.onSave).toHaveBeenCalledWith(expect.objectContaining({ cli: 'opencode' }))
    )
  },
}
